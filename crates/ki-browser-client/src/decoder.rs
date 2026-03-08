//! H.264 hardware decoder for receiving encoded frames from the server.
//!
//! Uses FFmpeg's h264_cuvid (NVIDIA) or software fallback to decode
//! H.264 NAL units into RGBA pixel data for egui texture upload.

#[cfg(feature = "h264")]
use tracing::{debug, info, warn};

/// Decoded frame ready for GPU texture upload.
pub struct DecodedFrame {
    pub rgba: Vec<u8>,
    pub width: u32,
    pub height: u32,
}

/// Trait for frame decoders (JPEG software, H.264 hardware).
pub trait FrameDecoder: Send {
    /// Process codec configuration data (e.g., H.264 SPS/PPS).
    fn set_config(&mut self, data: &[u8]);
    /// Decode a frame. Returns None if frame is not yet complete.
    fn decode(&mut self, data: &[u8]) -> Option<DecodedFrame>;
}

/// Software JPEG decoder (always available, no extra dependencies).
pub struct JpegDecoder;

impl FrameDecoder for JpegDecoder {
    fn set_config(&mut self, _data: &[u8]) {
        // JPEG has no codec config.
    }

    fn decode(&mut self, data: &[u8]) -> Option<DecodedFrame> {
        let img = image::load_from_memory_with_format(data, image::ImageFormat::Jpeg).ok()?;
        let rgba = img.to_rgba8();
        Some(DecodedFrame {
            width: rgba.width(),
            height: rgba.height(),
            rgba: rgba.into_raw(),
        })
    }
}

/// H.264 decoder using FFmpeg (hardware-accelerated when available).
#[cfg(feature = "h264")]
pub struct H264Decoder {
    decoder: ffmpeg_next::decoder::video::Video,
    scaler: Option<ffmpeg_next::software::scaling::Context>,
    config_data: Vec<u8>,
}

#[cfg(feature = "h264")]
// SAFETY: H264Decoder is used within a single task. The inner FFmpeg scaler
// contains raw pointers that prevent auto-Send, but we never share across threads.
unsafe impl Send for H264Decoder {}

#[cfg(feature = "h264")]
impl H264Decoder {
    /// Create a new H.264 decoder. Tries hardware (cuvid) first, then software.
    pub fn new() -> Result<Self, String> {
        ffmpeg_next::init().map_err(|e| format!("FFmpeg init: {e}"))?;

        let codec = Self::find_decoder()?;
        info!("Using H.264 decoder: {}", codec.name());

        let context = ffmpeg_next::codec::Context::new_with_codec(codec);
        let decoder = context
            .decoder()
            .video()
            .map_err(|e| format!("Decoder setup: {e}"))?;

        Ok(Self {
            decoder,
            scaler: None,
            config_data: Vec::new(),
        })
    }

    fn find_decoder() -> Result<ffmpeg_next::Codec, String> {
        // Try hardware decoders first.
        if let Some(codec) = ffmpeg_next::decoder::find_by_name("h264_cuvid") {
            return Ok(codec);
        }
        info!("h264_cuvid not available, using software decoder");
        ffmpeg_next::decoder::find(ffmpeg_next::codec::Id::H264)
            .ok_or_else(|| "No H.264 decoder found".into())
    }

    /// Ensure scaler is initialized for the decoder's output format → RGBA.
    fn ensure_scaler(&mut self, width: u32, height: u32, format: ffmpeg_next::format::Pixel) {
        if self.scaler.is_some() {
            return;
        }
        match ffmpeg_next::software::scaling::Context::get(
            format,
            width,
            height,
            ffmpeg_next::format::Pixel::RGBA,
            width,
            height,
            ffmpeg_next::software::scaling::Flags::FAST_BILINEAR,
        ) {
            Ok(s) => self.scaler = Some(s),
            Err(e) => warn!("Failed to create scaler: {e}"),
        }
    }
}

#[cfg(feature = "h264")]
impl FrameDecoder for H264Decoder {
    fn set_config(&mut self, data: &[u8]) {
        debug!("H.264 config received: {} bytes", data.len());
        self.config_data = data.to_vec();
        // Config data (SPS/PPS) would be fed to decoder as extradata.
        // For Annex B stream, it's included in the first keyframe.
    }

    fn decode(&mut self, data: &[u8]) -> Option<DecodedFrame> {
        let combined;
        let packet_data = if !self.config_data.is_empty() {
            debug!("Prepending {} bytes SPS/PPS config to first H.264 packet", self.config_data.len());
            combined = [&self.config_data[..], data].concat();
            self.config_data.clear();
            &combined
        } else {
            data
        };
        let packet = ffmpeg_next::Packet::copy(packet_data);
        if self.decoder.send_packet(&packet).is_err() {
            debug!("Failed to send packet to decoder");
            return None;
        }

        let mut decoded = ffmpeg_next::util::frame::Video::empty();
        if self.decoder.receive_frame(&mut decoded).is_err() {
            // First receive_frame may fail because some decoders (e.g. h264_cuvid)
            // buffer the first packet. Retry once — the decoder may have flushed
            // a frame from a previously buffered packet.
            debug!("First receive_frame returned no output, retrying once");
            if self.decoder.receive_frame(&mut decoded).is_err() {
                return None; // Frame not yet complete (normal for buffered decoders).
            }
        }

        let width = decoded.width();
        let height = decoded.height();
        let format = decoded.format();

        self.ensure_scaler(width, height, format);
        let scaler = self.scaler.as_mut()?;

        let mut rgba_frame = ffmpeg_next::util::frame::Video::new(
            ffmpeg_next::format::Pixel::RGBA,
            width,
            height,
        );

        if scaler.run(&decoded, &mut rgba_frame).is_err() {
            warn!("YUV→RGBA scaling failed");
            return None;
        }

        // Copy RGBA data from frame (may have stride padding).
        let stride = rgba_frame.stride(0);
        let row_bytes = (width as usize) * 4;
        let mut rgba = Vec::with_capacity(row_bytes * height as usize);
        let src = rgba_frame.data(0);
        for y in 0..height as usize {
            let offset = y * stride;
            let end = offset + row_bytes;
            if end <= src.len() {
                rgba.extend_from_slice(&src[offset..end]);
            }
        }

        debug!("Decoded H.264 frame: {width}x{height}");
        Some(DecodedFrame {
            rgba,
            width,
            height,
        })
    }
}

/// Create the best available decoder.
pub fn create_decoder() -> Box<dyn FrameDecoder> {
    #[cfg(feature = "h264")]
    {
        match H264Decoder::new() {
            Ok(dec) => return Box::new(dec),
            Err(e) => {
                tracing::warn!("H.264 decoder unavailable ({e}), using JPEG only");
            }
        }
    }
    Box::new(JpegDecoder)
}
