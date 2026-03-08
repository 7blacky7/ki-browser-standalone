//! H.264 NVENC hardware encoder for low-latency browser viewport streaming.
//!
//! Wraps FFmpeg's h264_nvenc encoder for hardware-accelerated encoding of
//! BGRA frame buffers into H.264 NAL units suitable for WebSocket transport.
//! Falls back to libx264 software encoding if NVENC is unavailable.

use tracing::{info, warn};
#[cfg(feature = "h264")]
use tracing::debug;

/// Byte prefix for binary WebSocket messages to identify the codec.
pub const PREFIX_JPEG: u8 = 0x00;
pub const PREFIX_H264_CONFIG: u8 = 0x01;
pub const PREFIX_H264_FRAME: u8 = 0x02;

/// Trait for frame encoders used by the viewer WebSocket handler.
pub trait FrameEncoder: Send {
    /// Encode a BGRA frame. Returns prefixed binary messages to send over WebSocket.
    /// May return multiple messages (e.g., config + frame for keyframes).
    fn encode(&mut self, bgra: &[u8], width: u32, height: u32) -> Vec<Vec<u8>>;

    /// Returns codec config data that should be sent once at connection start.
    fn codec_config(&self) -> Option<Vec<u8>>;
}

/// JPEG encoder (existing behavior, no external dependencies).
pub struct JpegEncoder {
    quality: u8,
}

impl JpegEncoder {
    pub fn new(quality: u8) -> Self {
        Self { quality }
    }
}

impl FrameEncoder for JpegEncoder {
    fn encode(&mut self, bgra: &[u8], width: u32, height: u32) -> Vec<Vec<u8>> {
        let pixel_count = (width as usize) * (height as usize);
        let expected = pixel_count * 4;
        let len = bgra.len().min(expected);

        // BGRA → RGB
        let mut rgb = Vec::with_capacity(pixel_count * 3);
        for chunk in bgra[..len].chunks_exact(4) {
            rgb.push(chunk[2]); // R
            rgb.push(chunk[1]); // G
            rgb.push(chunk[0]); // B
        }

        let img = match image::ImageBuffer::<image::Rgb<u8>, _>::from_raw(width, height, rgb) {
            Some(img) => img,
            None => return Vec::new(),
        };

        let mut buf = Vec::with_capacity(64 * 1024);
        buf.push(PREFIX_JPEG); // Prefix byte
        let mut cursor = std::io::Cursor::new(&mut buf);
        cursor.set_position(1); // Skip prefix byte
        if img
            .write_to(&mut cursor, image::ImageOutputFormat::Jpeg(self.quality))
            .is_err()
        {
            warn!("Failed to encode JPEG frame");
            return Vec::new();
        }

        vec![buf]
    }

    fn codec_config(&self) -> Option<Vec<u8>> {
        None
    }
}

/// H.264 NVENC hardware encoder using FFmpeg.
#[cfg(feature = "h264")]
pub struct H264NvencEncoder {
    encoder: ffmpeg_next::encoder::video::Encoder,
    scaler: ffmpeg_next::software::scaling::Context,
    input_frame: ffmpeg_next::util::frame::Video,
    scaled_frame: ffmpeg_next::util::frame::Video,
    frame_count: i64,
    width: u32,
    height: u32,
    extradata: Vec<u8>,
}

// SAFETY: H264NvencEncoder is only used within a single tokio task (send_task).
// The inner FFmpeg scaler contains a raw pointer (*mut SwsContext) that prevents
// auto-Send. Since the encoder is never shared across threads, this is safe.
#[cfg(feature = "h264")]
unsafe impl Send for H264NvencEncoder {}

#[cfg(feature = "h264")]
impl H264NvencEncoder {
    /// Create a new H.264 encoder. Tries NVENC first, falls back to libx264.
    pub fn new(width: u32, height: u32) -> Result<Self, String> {
        ffmpeg_next::init().map_err(|e| format!("FFmpeg init failed: {e}"))?;

        let codec = Self::find_encoder()?;
        info!("Using H.264 encoder: {}", codec.name());

        let context = ffmpeg_next::codec::Context::new_with_codec(codec);
        let mut encoder = context.encoder().video().map_err(|e| format!("Encoder setup failed: {e}"))?;

        encoder.set_width(width);
        encoder.set_height(height);
        encoder.set_format(ffmpeg_next::format::Pixel::NV12);
        encoder.set_time_base(ffmpeg_next::Rational::new(1, 30));
        encoder.set_frame_rate(Some(ffmpeg_next::Rational::new(30, 1)));
        encoder.set_gop(60); // IDR every 2 seconds at 30fps
        encoder.set_max_b_frames(0); // No B-frames for low latency
        encoder.set_bit_rate(4_000_000); // 4 Mbps

        // Low-latency preset via private options.
        let mut opts = ffmpeg_next::Dictionary::new();
        if codec.name() == "h264_nvenc" {
            opts.set("preset", "p1");         // Fastest NVENC preset
            opts.set("tune", "ull");          // Ultra low latency
            opts.set("zerolatency", "1");
            opts.set("rc", "cbr");            // Constant bitrate
            opts.set("delay", "0");
        } else {
            // libx264 fallback
            opts.set("preset", "ultrafast");
            opts.set("tune", "zerolatency");
        }

        let encoder = encoder
            .open_with(opts)
            .map_err(|e| format!("Failed to open encoder: {e}"))?;

        // SAFETY: The encoder owns the AVCodecContext; extradata is valid after open_with().
        let extradata = unsafe {
            let ctx = encoder.as_ptr();
            let ptr = (*ctx).extradata;
            let size = (*ctx).extradata_size;
            if !ptr.is_null() && size > 0 {
                let data = std::slice::from_raw_parts(ptr, size as usize).to_vec();
                info!("H.264 extradata extracted: {} bytes (SPS/PPS)", data.len());
                data
            } else {
                info!("H.264 extradata empty — SPS/PPS will be in-stream with IDR frames");
                Vec::new()
            }
        };

        // Scaler: BGRA → NV12
        let scaler = ffmpeg_next::software::scaling::Context::get(
            ffmpeg_next::format::Pixel::BGRA,
            width,
            height,
            ffmpeg_next::format::Pixel::NV12,
            width,
            height,
            ffmpeg_next::software::scaling::Flags::FAST_BILINEAR,
        )
        .map_err(|e| format!("Scaler init failed: {e}"))?;

        let input_frame = ffmpeg_next::util::frame::Video::new(
            ffmpeg_next::format::Pixel::BGRA,
            width,
            height,
        );
        let scaled_frame = ffmpeg_next::util::frame::Video::new(
            ffmpeg_next::format::Pixel::NV12,
            width,
            height,
        );

        info!("H.264 encoder initialized: {width}x{height}, NVENC={}", codec.name() == "h264_nvenc");

        Ok(Self {
            encoder,
            scaler,
            input_frame,
            scaled_frame,
            frame_count: 0,
            width,
            height,
            extradata,
        })
    }

    /// Find best available H.264 encoder (NVENC preferred).
    fn find_encoder() -> Result<ffmpeg_next::Codec, String> {
        // Try NVENC first.
        if let Some(codec) = ffmpeg_next::encoder::find_by_name("h264_nvenc") {
            return Ok(codec);
        }
        info!("NVENC not available, falling back to libx264");
        if let Some(codec) = ffmpeg_next::encoder::find_by_name("libx264") {
            return Ok(codec);
        }
        Err("No H.264 encoder found (tried h264_nvenc, libx264)".into())
    }
}

#[cfg(feature = "h264")]
impl FrameEncoder for H264NvencEncoder {
    fn encode(&mut self, bgra: &[u8], width: u32, height: u32) -> Vec<Vec<u8>> {
        if width != self.width || height != self.height {
            warn!("Frame size changed ({width}x{height}), encoder expects {}x{}", self.width, self.height);
            return Vec::new();
        }

        // Copy BGRA data into input frame.
        let stride = self.input_frame.stride(0);
        let dst = self.input_frame.data_mut(0);
        let row_bytes = (width as usize) * 4;
        for y in 0..height as usize {
            let src_offset = y * row_bytes;
            let dst_offset = y * stride;
            let src_end = (src_offset + row_bytes).min(bgra.len());
            let dst_end = dst_offset + row_bytes;
            if src_end > src_offset && dst_end <= dst.len() {
                dst[dst_offset..dst_end].copy_from_slice(&bgra[src_offset..src_end]);
            }
        }

        // Scale BGRA → NV12.
        if self.scaler.run(&self.input_frame, &mut self.scaled_frame).is_err() {
            warn!("BGRA→NV12 scaling failed");
            return Vec::new();
        }

        self.scaled_frame.set_pts(Some(self.frame_count));
        self.frame_count += 1;

        // Encode.
        let mut messages = Vec::new();
        if self.encoder.send_frame(&self.scaled_frame).is_err() {
            warn!("Failed to send frame to encoder");
            return Vec::new();
        }

        let mut packet = ffmpeg_next::Packet::empty();
        while self.encoder.receive_packet(&mut packet).is_ok() {
            let data = packet.data().unwrap_or(&[]);
            let mut msg = Vec::with_capacity(1 + data.len());
            msg.push(PREFIX_H264_FRAME);
            msg.extend_from_slice(data);
            messages.push(msg);
            debug!("H.264 packet: {} bytes, keyframe={}", data.len(), packet.is_key());
        }

        messages
    }

    fn codec_config(&self) -> Option<Vec<u8>> {
        if self.extradata.is_empty() {
            return None;
        }
        let mut msg = Vec::with_capacity(1 + self.extradata.len());
        msg.push(PREFIX_H264_CONFIG);
        msg.extend_from_slice(&self.extradata);
        Some(msg)
    }
}

/// Create the best available encoder for the given dimensions.
#[allow(unused_variables)]
pub fn create_encoder(width: u32, height: u32) -> Box<dyn FrameEncoder> {
    #[cfg(feature = "h264")]
    {
        match H264NvencEncoder::new(width, height) {
            Ok(enc) => {
                info!("Using H.264 hardware encoder");
                return Box::new(enc);
            }
            Err(e) => {
                warn!("H.264 encoder init failed ({e}), falling back to JPEG");
            }
        }
    }

    info!("Using JPEG encoder (quality 75)");
    Box::new(JpegEncoder::new(75))
}
