//! CEF BGRA frame buffer to PNG conversion and OCR bounding box rendering.
//!
//! Provides `frame_buffer_to_png` for efficient BGRA-to-RGBA channel swapping
//! and PNG encoding, and `draw_ocr_bounding_boxes` for overlaying red rectangles
//! onto screenshots to highlight OCR-detected text regions.

use std::sync::Arc;

use parking_lot::RwLock;

use super::super::devtools;

/// Converts a CEF BGRA frame buffer to PNG bytes.
///
/// Uses `chunks_exact(4)` for efficient BGRA->RGBA channel swapping, matching
/// the conversion approach in cef_render.rs. Releases the frame buffer lock
/// before PNG encoding to minimise lock contention with the render thread.
///
/// Used by OCR and Vision to get screenshots without going through the REST API.
pub(in crate::gui) fn frame_buffer_to_png(
    frame_buffer: &Arc<RwLock<Vec<u8>>>,
    frame_size: &Arc<RwLock<(u32, u32)>>,
) -> Result<Vec<u8>, String> {
    use image::{ImageBuffer, ImageOutputFormat, Rgba};

    let fb = frame_buffer.read();
    let (w, h) = *frame_size.read();

    tracing::debug!("frame_buffer_to_png: converting {}x{} frame", w, h);

    if fb.is_empty() || w == 0 || h == 0 {
        return Err("No frame buffer available (page not loaded yet?)".to_string());
    }

    let expected_len = (w as usize) * (h as usize) * 4;
    if fb.len() < expected_len {
        return Err(format!(
            "Frame buffer too small: {} bytes for {}x{} (expected {})",
            fb.len(),
            w,
            h,
            expected_len
        ));
    }

    // Efficient BGRA -> RGBA conversion using chunks_exact (avoids per-pixel indexing overhead).
    // CEF delivers frames in BGRA order: [B=0, G=1, R=2, A=3].
    // PNG expects RGBA order, so we swap B<->R channels.
    let mut rgba = Vec::with_capacity(expected_len);
    for chunk in fb[..expected_len].chunks_exact(4) {
        rgba.push(chunk[2]); // R <- BGRA[2]
        rgba.push(chunk[1]); // G <- BGRA[1]
        rgba.push(chunk[0]); // B <- BGRA[0]
        rgba.push(chunk[3]); // A <- BGRA[3]
    }
    drop(fb); // Release frame buffer lock early before PNG encoding

    let img: ImageBuffer<Rgba<u8>, Vec<u8>> = ImageBuffer::from_raw(w, h, rgba)
        .ok_or_else(|| "ImageBuffer::from_raw failed".to_string())?;

    let mut output = Vec::new();
    img.write_to(&mut std::io::Cursor::new(&mut output), ImageOutputFormat::Png)
        .map_err(|e| format!("PNG encoding failed: {}", e))?;

    tracing::debug!("frame_buffer_to_png: PNG {} bytes", output.len());
    Ok(output)
}

/// Draws red bounding boxes with 1-based region indices onto a PNG screenshot.
///
/// Decodes the source PNG, draws a 2-pixel red rectangle around each OCR region,
/// re-encodes to PNG and returns the annotated bytes. Used to produce the
/// `ocr_image` shown in DevTools above the per-region table.
pub(in crate::gui) fn draw_ocr_bounding_boxes(
    png_data: &[u8],
    regions: &[devtools::OcrDisplayRegion],
) -> Result<Vec<u8>, String> {
    use image::{ImageOutputFormat, Rgba};

    let mut img = image::load_from_memory(png_data)
        .map_err(|e| format!("PNG decode failed: {}", e))?
        .to_rgba8();

    let red = Rgba([220u8, 50u8, 50u8, 255u8]);

    for region in regions {
        let x0 = region.x.max(0.0) as u32;
        let y0 = region.y.max(0.0) as u32;
        let x1 = (region.x + region.w).max(0.0) as u32;
        let y1 = (region.y + region.h).max(0.0) as u32;
        let img_w = img.width();
        let img_h = img.height();

        // Draw the four sides of the bounding box rectangle (2 px thick).
        for thickness in 0u32..2 {
            let top = y0.saturating_add(thickness).min(img_h.saturating_sub(1));
            let bottom = y1.saturating_add(thickness).min(img_h.saturating_sub(1));
            let left = x0.saturating_add(thickness).min(img_w.saturating_sub(1));
            let right = x1.saturating_add(thickness).min(img_w.saturating_sub(1));

            // Top and bottom horizontal lines
            for x in left..=right.min(img_w.saturating_sub(1)) {
                img.put_pixel(x, top, red);
                img.put_pixel(x, bottom, red);
            }
            // Left and right vertical lines
            for y in top..=bottom.min(img_h.saturating_sub(1)) {
                img.put_pixel(left, y, red);
                img.put_pixel(right, y, red);
            }
        }
    }

    let mut output = Vec::new();
    img.write_to(&mut std::io::Cursor::new(&mut output), ImageOutputFormat::Png)
        .map_err(|e| format!("PNG encode failed: {}", e))?;
    Ok(output)
}
