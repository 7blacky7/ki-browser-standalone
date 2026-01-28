//! Off-screen rendering (OSR) handler for CEF browser automation.
//!
//! This module provides off-screen rendering capabilities for CEF, enabling:
//! - Headless browser operation without a visible window
//! - Screenshot capture of rendered web pages
//! - Visual automation for testing and scraping
//!
//! # Architecture
//!
//! The `OffScreenRenderHandler` implements CEF's render handler interface,
//! receiving rendered pixels from the browser and storing them in a frame buffer.
//! Double buffering is used to ensure smooth updates and thread-safe access.
//!
//! # Example
//!
//! ```rust,ignore
//! use ki_browser::browser::cef_render::{OffScreenRenderHandler, ScreenInfo};
//!
//! let screen_info = ScreenInfo::default();
//! let handler = OffScreenRenderHandler::new(1920, 1080, screen_info);
//!
//! // After CEF paints to the handler...
//! let screenshot = handler.capture_screenshot(ScreenshotFormat::Png, 80)?;
//! ```

#[cfg(feature = "cef-browser")]
use anyhow::{anyhow, Result};
#[cfg(feature = "cef-browser")]
use base64::{engine::general_purpose::STANDARD as BASE64, Engine as _};
#[cfg(feature = "cef-browser")]
use image::{ImageBuffer, ImageEncoder, Rgba, RgbaImage};
#[cfg(feature = "cef-browser")]
use parking_lot::RwLock;
#[cfg(feature = "cef-browser")]
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
#[cfg(feature = "cef-browser")]
use std::sync::Arc;

#[cfg(feature = "cef-browser")]
use crate::browser::screenshot::ScreenshotFormat;

/// Represents a rectangular region that has been modified.
#[cfg(feature = "cef-browser")]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DirtyRect {
    /// X coordinate of the top-left corner.
    pub x: i32,
    /// Y coordinate of the top-left corner.
    pub y: i32,
    /// Width of the dirty region.
    pub width: i32,
    /// Height of the dirty region.
    pub height: i32,
}

#[cfg(feature = "cef-browser")]
impl DirtyRect {
    /// Creates a new dirty rectangle.
    pub fn new(x: i32, y: i32, width: i32, height: i32) -> Self {
        Self { x, y, width, height }
    }

    /// Creates a dirty rect covering the entire viewport.
    pub fn full(width: i32, height: i32) -> Self {
        Self {
            x: 0,
            y: 0,
            width,
            height,
        }
    }

    /// Returns the area of the dirty region.
    pub fn area(&self) -> i32 {
        self.width * self.height
    }

    /// Checks if this rect intersects with another.
    pub fn intersects(&self, other: &DirtyRect) -> bool {
        self.x < other.x + other.width
            && self.x + self.width > other.x
            && self.y < other.y + other.height
            && self.y + self.height > other.y
    }

    /// Returns the union of two rectangles (smallest rect containing both).
    pub fn union(&self, other: &DirtyRect) -> DirtyRect {
        let x = self.x.min(other.x);
        let y = self.y.min(other.y);
        let right = (self.x + self.width).max(other.x + other.width);
        let bottom = (self.y + self.height).max(other.y + other.height);

        DirtyRect {
            x,
            y,
            width: right - x,
            height: bottom - y,
        }
    }

    /// Clips this rect to fit within bounds.
    pub fn clip(&self, max_width: i32, max_height: i32) -> DirtyRect {
        let x = self.x.max(0);
        let y = self.y.max(0);
        let width = (self.x + self.width).min(max_width) - x;
        let height = (self.y + self.height).min(max_height) - y;

        DirtyRect {
            x,
            y,
            width: width.max(0),
            height: height.max(0),
        }
    }
}

/// Screen information for the off-screen browser.
#[cfg(feature = "cef-browser")]
#[derive(Debug, Clone, Copy)]
pub struct ScreenInfo {
    /// Device scale factor (DPI scaling, e.g., 1.0, 1.25, 2.0).
    pub device_scale_factor: f32,
    /// Color depth in bits per pixel.
    pub depth: i32,
    /// Color depth per component.
    pub depth_per_component: i32,
    /// Whether the screen is monochrome.
    pub is_monochrome: bool,
    /// Available screen rectangle (x, y, width, height).
    pub available_rect: (i32, i32, i32, i32),
    /// Full screen rectangle.
    pub rect: (i32, i32, i32, i32),
}

#[cfg(feature = "cef-browser")]
impl Default for ScreenInfo {
    fn default() -> Self {
        Self {
            device_scale_factor: 1.0,
            depth: 32,
            depth_per_component: 8,
            is_monochrome: false,
            available_rect: (0, 0, 1920, 1080),
            rect: (0, 0, 1920, 1080),
        }
    }
}

#[cfg(feature = "cef-browser")]
impl ScreenInfo {
    /// Creates a new ScreenInfo with custom dimensions.
    pub fn new(width: i32, height: i32) -> Self {
        Self {
            available_rect: (0, 0, width, height),
            rect: (0, 0, width, height),
            ..Default::default()
        }
    }

    /// Sets the device scale factor (DPI).
    pub fn with_scale(mut self, scale: f32) -> Self {
        self.device_scale_factor = scale;
        self
    }

    /// Sets the color depth.
    pub fn with_depth(mut self, depth: i32, depth_per_component: i32) -> Self {
        self.depth = depth;
        self.depth_per_component = depth_per_component;
        self
    }
}

/// Frame buffer for storing rendered pixels.
#[cfg(feature = "cef-browser")]
struct FrameBuffer {
    /// BGRA pixel data.
    data: Vec<u8>,
    /// Buffer width in pixels.
    width: u32,
    /// Buffer height in pixels.
    height: u32,
    /// Frame sequence number.
    frame_number: u64,
}

#[cfg(feature = "cef-browser")]
impl FrameBuffer {
    /// Creates a new frame buffer with the specified dimensions.
    fn new(width: u32, height: u32) -> Self {
        let size = (width * height * 4) as usize; // 4 bytes per pixel (BGRA)
        Self {
            data: vec![0; size],
            width,
            height,
            frame_number: 0,
        }
    }

    /// Resizes the buffer to new dimensions.
    fn resize(&mut self, width: u32, height: u32) {
        let size = (width * height * 4) as usize;
        self.data.resize(size, 0);
        self.width = width;
        self.height = height;
    }

    /// Copies pixel data into the buffer.
    fn copy_from(&mut self, data: &[u8], rect: &DirtyRect) {
        // For full buffer updates, copy directly
        if rect.x == 0
            && rect.y == 0
            && rect.width as u32 == self.width
            && rect.height as u32 == self.height
        {
            if data.len() == self.data.len() {
                self.data.copy_from_slice(data);
            }
        } else {
            // Partial update - copy row by row
            let src_stride = rect.width as usize * 4;
            let dst_stride = self.width as usize * 4;

            for row in 0..rect.height as usize {
                let src_offset = row * src_stride;
                let dst_row = (rect.y as usize + row) * dst_stride;
                let dst_offset = dst_row + rect.x as usize * 4;

                if src_offset + src_stride <= data.len()
                    && dst_offset + src_stride <= self.data.len()
                {
                    self.data[dst_offset..dst_offset + src_stride]
                        .copy_from_slice(&data[src_offset..src_offset + src_stride]);
                }
            }
        }

        self.frame_number += 1;
    }

    /// Converts BGRA data to RGBA format.
    fn to_rgba(&self) -> Vec<u8> {
        let mut rgba = Vec::with_capacity(self.data.len());
        for chunk in self.data.chunks_exact(4) {
            // BGRA -> RGBA
            rgba.push(chunk[2]); // R
            rgba.push(chunk[1]); // G
            rgba.push(chunk[0]); // B
            rgba.push(chunk[3]); // A
        }
        rgba
    }
}

/// Off-screen render handler for CEF.
///
/// This handler receives rendered pixels from CEF and stores them in a double-buffered
/// frame buffer for thread-safe access. It provides screenshot capture functionality
/// in various formats.
#[cfg(feature = "cef-browser")]
pub struct OffScreenRenderHandler {
    /// Front buffer (for reading).
    front_buffer: Arc<RwLock<FrameBuffer>>,
    /// Back buffer (for writing).
    back_buffer: Arc<RwLock<FrameBuffer>>,
    /// Current viewport width.
    width: AtomicU64,
    /// Current viewport height.
    height: AtomicU64,
    /// Screen information.
    screen_info: RwLock<ScreenInfo>,
    /// Accumulated dirty rectangles.
    dirty_rects: RwLock<Vec<DirtyRect>>,
    /// Whether a paint is pending.
    paint_pending: AtomicBool,
    /// Total frames rendered.
    frame_count: AtomicU64,
}

#[cfg(feature = "cef-browser")]
impl OffScreenRenderHandler {
    /// Creates a new off-screen render handler.
    ///
    /// # Arguments
    ///
    /// * `width` - Initial viewport width in pixels
    /// * `height` - Initial viewport height in pixels
    /// * `screen_info` - Screen properties (DPI, color depth, etc.)
    pub fn new(width: u32, height: u32, screen_info: ScreenInfo) -> Self {
        Self {
            front_buffer: Arc::new(RwLock::new(FrameBuffer::new(width, height))),
            back_buffer: Arc::new(RwLock::new(FrameBuffer::new(width, height))),
            width: AtomicU64::new(width as u64),
            height: AtomicU64::new(height as u64),
            screen_info: RwLock::new(screen_info),
            dirty_rects: RwLock::new(Vec::new()),
            paint_pending: AtomicBool::new(false),
            frame_count: AtomicU64::new(0),
        }
    }

    /// Creates a handler with default screen info.
    pub fn with_size(width: u32, height: u32) -> Self {
        let screen_info = ScreenInfo::new(width as i32, height as i32);
        Self::new(width, height, screen_info)
    }

    // ========================================================================
    // CEF RenderHandler Interface Methods
    // ========================================================================

    /// Returns the view rectangle for the browser.
    ///
    /// CEF calls this to determine the size of the off-screen surface.
    /// Returns (x, y, width, height) where (x, y) is typically (0, 0).
    pub fn get_view_rect(&self) -> (i32, i32, i32, i32) {
        let width = self.width.load(Ordering::Relaxed) as i32;
        let height = self.height.load(Ordering::Relaxed) as i32;
        (0, 0, width, height)
    }

    /// Returns screen information for the browser.
    ///
    /// CEF uses this to determine DPI scaling, color depth, and screen bounds.
    pub fn get_screen_info(&self) -> ScreenInfo {
        *self.screen_info.read()
    }

    /// Converts view coordinates to screen coordinates.
    ///
    /// For off-screen rendering, this is typically a 1:1 mapping adjusted
    /// for the device scale factor.
    ///
    /// # Arguments
    ///
    /// * `view_x` - X coordinate in view space
    /// * `view_y` - Y coordinate in view space
    ///
    /// # Returns
    ///
    /// (screen_x, screen_y) coordinates
    pub fn get_screen_point(&self, view_x: i32, view_y: i32) -> (i32, i32) {
        let screen_info = self.screen_info.read();
        let scale = screen_info.device_scale_factor;

        let screen_x = (view_x as f32 * scale) as i32;
        let screen_y = (view_y as f32 * scale) as i32;

        (screen_x, screen_y)
    }

    /// Called when CEF has new pixels to render.
    ///
    /// This method receives the rendered pixel data from CEF and updates
    /// the back buffer. The buffers are swapped atomically after the update.
    ///
    /// # Arguments
    ///
    /// * `paint_type` - Type of paint operation (0 = content, 1 = popup)
    /// * `dirty_rects` - List of rectangles that have changed
    /// * `buffer` - Raw BGRA pixel data
    /// * `width` - Width of the rendered area
    /// * `height` - Height of the rendered area
    pub fn on_paint(
        &self,
        _paint_type: i32,
        dirty_rects: &[DirtyRect],
        buffer: &[u8],
        width: i32,
        height: i32,
    ) {
        // Update dimensions if they've changed
        let current_width = self.width.load(Ordering::Relaxed) as i32;
        let current_height = self.height.load(Ordering::Relaxed) as i32;

        if width != current_width || height != current_height {
            self.resize(width as u32, height as u32);
        }

        // Calculate the union of all dirty rects for the update
        let update_rect = if dirty_rects.is_empty() {
            DirtyRect::full(width, height)
        } else {
            dirty_rects
                .iter()
                .fold(dirty_rects[0], |acc, r| acc.union(r))
        };

        // Copy to back buffer
        {
            let mut back = self.back_buffer.write();
            back.copy_from(buffer, &update_rect);
        }

        // Swap buffers
        self.swap_buffers();

        // Track dirty regions
        {
            let mut tracked_rects = self.dirty_rects.write();
            tracked_rects.extend_from_slice(dirty_rects);
        }

        self.frame_count.fetch_add(1, Ordering::Relaxed);
        self.paint_pending.store(false, Ordering::Release);
    }

    /// Called when popup visibility changes.
    pub fn on_popup_show(&self, _show: bool) {
        // Popup handling can be implemented if needed
    }

    /// Called when popup size/position changes.
    pub fn on_popup_size(&self, _rect: DirtyRect) {
        // Popup handling can be implemented if needed
    }

    // ========================================================================
    // Screenshot Capture Methods
    // ========================================================================

    /// Captures a screenshot of the current frame.
    ///
    /// # Arguments
    ///
    /// * `format` - Output image format (PNG, JPEG, WebP)
    /// * `quality` - Quality for lossy formats (0-100, ignored for PNG)
    ///
    /// # Returns
    ///
    /// Base64-encoded image data or an error.
    pub fn capture_screenshot(&self, format: ScreenshotFormat, quality: u8) -> Result<String> {
        let (rgba_data, width, height) = {
            let front = self.front_buffer.read();
            (front.to_rgba(), front.width, front.height)
        };

        self.encode_image(&rgba_data, width, height, format, quality)
    }

    /// Captures a screenshot of a specific region.
    ///
    /// This is useful for capturing specific elements identified by their
    /// bounding box coordinates.
    ///
    /// # Arguments
    ///
    /// * `x` - X coordinate of the region
    /// * `y` - Y coordinate of the region
    /// * `width` - Width of the region
    /// * `height` - Height of the region
    /// * `format` - Output image format
    /// * `quality` - Quality for lossy formats
    ///
    /// # Returns
    ///
    /// Base64-encoded image data or an error.
    pub fn capture_region(
        &self,
        x: u32,
        y: u32,
        width: u32,
        height: u32,
        format: ScreenshotFormat,
        quality: u8,
    ) -> Result<String> {
        let (full_rgba, full_width, full_height) = {
            let front = self.front_buffer.read();
            (front.to_rgba(), front.width, front.height)
        };

        // Validate region bounds
        if x + width > full_width || y + height > full_height {
            return Err(anyhow!(
                "Region ({}, {}, {}, {}) exceeds buffer bounds ({}, {})",
                x,
                y,
                width,
                height,
                full_width,
                full_height
            ));
        }

        // Extract the region
        let mut region_data = Vec::with_capacity((width * height * 4) as usize);
        let src_stride = full_width as usize * 4;

        for row in 0..height as usize {
            let src_row = (y as usize + row) * src_stride;
            let src_start = src_row + x as usize * 4;
            let src_end = src_start + width as usize * 4;
            region_data.extend_from_slice(&full_rgba[src_start..src_end]);
        }

        self.encode_image(&region_data, width, height, format, quality)
    }

    /// Captures a screenshot of a specific element by CSS selector.
    ///
    /// This method requires the element's bounding box to be provided,
    /// typically obtained via JavaScript evaluation.
    ///
    /// # Arguments
    ///
    /// * `selector` - CSS selector (for reference/logging)
    /// * `bounds` - Element bounding box (x, y, width, height)
    /// * `format` - Output image format
    /// * `quality` - Quality for lossy formats
    ///
    /// # Returns
    ///
    /// Base64-encoded image data or an error.
    pub fn capture_element(
        &self,
        _selector: &str,
        bounds: (u32, u32, u32, u32),
        format: ScreenshotFormat,
        quality: u8,
    ) -> Result<String> {
        let (x, y, width, height) = bounds;
        self.capture_region(x, y, width, height, format, quality)
    }

    /// Returns the raw RGBA pixel data of the current frame.
    ///
    /// This can be used for custom image processing or analysis.
    pub fn get_raw_pixels(&self) -> (Vec<u8>, u32, u32) {
        let front = self.front_buffer.read();
        (front.to_rgba(), front.width, front.height)
    }

    /// Returns the raw BGRA pixel data (native CEF format).
    pub fn get_raw_bgra_pixels(&self) -> (Vec<u8>, u32, u32) {
        let front = self.front_buffer.read();
        (front.data.clone(), front.width, front.height)
    }

    // ========================================================================
    // Buffer Management
    // ========================================================================

    /// Resizes the render buffers to new dimensions.
    ///
    /// This should be called when the browser viewport size changes.
    pub fn resize(&self, width: u32, height: u32) {
        self.width.store(width as u64, Ordering::Relaxed);
        self.height.store(height as u64, Ordering::Relaxed);

        {
            let mut front = self.front_buffer.write();
            front.resize(width, height);
        }
        {
            let mut back = self.back_buffer.write();
            back.resize(width, height);
        }

        // Update screen info
        {
            let mut info = self.screen_info.write();
            info.available_rect = (0, 0, width as i32, height as i32);
            info.rect = (0, 0, width as i32, height as i32);
        }

        // Clear dirty rects
        self.dirty_rects.write().clear();
    }

    /// Swaps front and back buffers.
    fn swap_buffers(&self) {
        // We use a simple copy strategy instead of actual pointer swapping
        // for simplicity with parking_lot
        let back_data = {
            let back = self.back_buffer.read();
            (back.data.clone(), back.width, back.height, back.frame_number)
        };

        let mut front = self.front_buffer.write();
        front.data = back_data.0;
        front.width = back_data.1;
        front.height = back_data.2;
        front.frame_number = back_data.3;
    }

    /// Clears the dirty rectangle tracking.
    pub fn clear_dirty_rects(&self) {
        self.dirty_rects.write().clear();
    }

    /// Returns the accumulated dirty rectangles since last clear.
    pub fn get_dirty_rects(&self) -> Vec<DirtyRect> {
        self.dirty_rects.read().clone()
    }

    /// Returns whether there's a paint operation pending.
    pub fn is_paint_pending(&self) -> bool {
        self.paint_pending.load(Ordering::Acquire)
    }

    /// Marks that a paint operation is expected.
    pub fn set_paint_pending(&self) {
        self.paint_pending.store(true, Ordering::Release);
    }

    /// Returns the total number of frames rendered.
    pub fn frame_count(&self) -> u64 {
        self.frame_count.load(Ordering::Relaxed)
    }

    /// Returns the current viewport dimensions.
    pub fn dimensions(&self) -> (u32, u32) {
        let width = self.width.load(Ordering::Relaxed) as u32;
        let height = self.height.load(Ordering::Relaxed) as u32;
        (width, height)
    }

    /// Updates the screen info.
    pub fn set_screen_info(&self, info: ScreenInfo) {
        *self.screen_info.write() = info;
    }

    /// Updates the device scale factor.
    pub fn set_device_scale_factor(&self, scale: f32) {
        self.screen_info.write().device_scale_factor = scale;
    }

    // ========================================================================
    // Private Helper Methods
    // ========================================================================

    /// Encodes RGBA pixel data to the specified image format.
    fn encode_image(
        &self,
        rgba_data: &[u8],
        width: u32,
        height: u32,
        format: ScreenshotFormat,
        quality: u8,
    ) -> Result<String> {
        let img: RgbaImage = ImageBuffer::from_raw(width, height, rgba_data.to_vec())
            .ok_or_else(|| anyhow!("Failed to create image buffer"))?;

        let mut buffer = Vec::new();

        match format {
            ScreenshotFormat::Png => {
                let encoder = image::codecs::png::PngEncoder::new(&mut buffer);
                encoder
                    .write_image(&img, width, height, image::ExtendedColorType::Rgba8)
                    .map_err(|e| anyhow!("PNG encoding failed: {}", e))?;
            }
            ScreenshotFormat::Jpeg => {
                let encoder =
                    image::codecs::jpeg::JpegEncoder::new_with_quality(&mut buffer, quality);
                encoder
                    .write_image(&img, width, height, image::ExtendedColorType::Rgba8)
                    .map_err(|e| anyhow!("JPEG encoding failed: {}", e))?;
            }
            ScreenshotFormat::WebP => {
                // For WebP, we'll use a simple approach - encode as PNG if webp feature isn't available
                // In production, you'd want the webp crate for proper WebP encoding
                #[cfg(feature = "webp")]
                {
                    use webp::Encoder as WebPEncoder;
                    let encoder = WebPEncoder::from_rgba(&img, width, height);
                    let webp_data = encoder.encode(quality as f32);
                    buffer = webp_data.to_vec();
                }
                #[cfg(not(feature = "webp"))]
                {
                    // Fallback to PNG if WebP encoding isn't available
                    let encoder = image::codecs::png::PngEncoder::new(&mut buffer);
                    encoder
                        .write_image(&img, width, height, image::ExtendedColorType::Rgba8)
                        .map_err(|e| anyhow!("PNG encoding (WebP fallback) failed: {}", e))?;
                }
            }
        }

        Ok(BASE64.encode(&buffer))
    }
}

#[cfg(feature = "cef-browser")]
impl Default for OffScreenRenderHandler {
    fn default() -> Self {
        Self::with_size(1920, 1080)
    }
}

// ============================================================================
// CEF Render Handler Trait (for actual CEF integration)
// ============================================================================

/// Trait for CEF render handler integration.
///
/// This trait defines the interface that must be implemented to integrate
/// with the actual CEF library. The `OffScreenRenderHandler` implements
/// this trait's functionality, but the actual CEF bindings will need to
/// call these methods from the CEF callbacks.
#[cfg(feature = "cef-browser")]
pub trait CefRenderHandler: Send + Sync {
    /// Called to get the view rectangle.
    fn get_view_rect(&self) -> (i32, i32, i32, i32);

    /// Called to get screen information.
    fn get_screen_info(&self) -> ScreenInfo;

    /// Called to convert view to screen coordinates.
    fn get_screen_point(&self, view_x: i32, view_y: i32) -> (i32, i32);

    /// Called when the browser has new content to paint.
    fn on_paint(
        &self,
        paint_type: i32,
        dirty_rects: &[DirtyRect],
        buffer: &[u8],
        width: i32,
        height: i32,
    );

    /// Called when popup visibility changes.
    fn on_popup_show(&self, show: bool);

    /// Called when popup size changes.
    fn on_popup_size(&self, rect: DirtyRect);
}

#[cfg(feature = "cef-browser")]
impl CefRenderHandler for OffScreenRenderHandler {
    fn get_view_rect(&self) -> (i32, i32, i32, i32) {
        OffScreenRenderHandler::get_view_rect(self)
    }

    fn get_screen_info(&self) -> ScreenInfo {
        OffScreenRenderHandler::get_screen_info(self)
    }

    fn get_screen_point(&self, view_x: i32, view_y: i32) -> (i32, i32) {
        OffScreenRenderHandler::get_screen_point(self, view_x, view_y)
    }

    fn on_paint(
        &self,
        paint_type: i32,
        dirty_rects: &[DirtyRect],
        buffer: &[u8],
        width: i32,
        height: i32,
    ) {
        OffScreenRenderHandler::on_paint(self, paint_type, dirty_rects, buffer, width, height)
    }

    fn on_popup_show(&self, show: bool) {
        OffScreenRenderHandler::on_popup_show(self, show)
    }

    fn on_popup_size(&self, rect: DirtyRect) {
        OffScreenRenderHandler::on_popup_size(self, rect)
    }
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(all(test, feature = "cef-browser"))]
mod tests {
    use super::*;

    #[test]
    fn test_dirty_rect() {
        let rect1 = DirtyRect::new(10, 20, 100, 50);
        assert_eq!(rect1.area(), 5000);

        let rect2 = DirtyRect::new(50, 30, 80, 40);
        assert!(rect1.intersects(&rect2));

        let union = rect1.union(&rect2);
        assert_eq!(union.x, 10);
        assert_eq!(union.y, 20);
        assert_eq!(union.width, 120);
        assert_eq!(union.height, 50);

        let clipped = DirtyRect::new(-10, -10, 100, 100).clip(50, 50);
        assert_eq!(clipped.x, 0);
        assert_eq!(clipped.y, 0);
        assert_eq!(clipped.width, 50);
        assert_eq!(clipped.height, 50);
    }

    #[test]
    fn test_screen_info() {
        let info = ScreenInfo::default();
        assert_eq!(info.device_scale_factor, 1.0);
        assert_eq!(info.depth, 32);

        let custom = ScreenInfo::new(1280, 720)
            .with_scale(2.0)
            .with_depth(24, 8);
        assert_eq!(custom.device_scale_factor, 2.0);
        assert_eq!(custom.depth, 24);
        assert_eq!(custom.rect, (0, 0, 1280, 720));
    }

    #[test]
    fn test_render_handler_creation() {
        let handler = OffScreenRenderHandler::with_size(800, 600);
        assert_eq!(handler.dimensions(), (800, 600));
        assert_eq!(handler.get_view_rect(), (0, 0, 800, 600));
        assert_eq!(handler.frame_count(), 0);
    }

    #[test]
    fn test_render_handler_resize() {
        let handler = OffScreenRenderHandler::with_size(800, 600);
        handler.resize(1920, 1080);

        assert_eq!(handler.dimensions(), (1920, 1080));
        assert_eq!(handler.get_view_rect(), (0, 0, 1920, 1080));
    }

    #[test]
    fn test_screen_point_conversion() {
        let mut screen_info = ScreenInfo::default();
        screen_info.device_scale_factor = 2.0;

        let handler = OffScreenRenderHandler::new(800, 600, screen_info);
        let (screen_x, screen_y) = handler.get_screen_point(100, 50);

        assert_eq!(screen_x, 200);
        assert_eq!(screen_y, 100);
    }

    #[test]
    fn test_on_paint() {
        let handler = OffScreenRenderHandler::with_size(4, 4);

        // Create a simple 4x4 BGRA buffer (blue pixels)
        let mut buffer = vec![0u8; 4 * 4 * 4];
        for pixel in buffer.chunks_exact_mut(4) {
            pixel[0] = 255; // B
            pixel[1] = 0; // G
            pixel[2] = 0; // R
            pixel[3] = 255; // A
        }

        let dirty_rects = vec![DirtyRect::full(4, 4)];
        handler.on_paint(0, &dirty_rects, &buffer, 4, 4);

        assert_eq!(handler.frame_count(), 1);

        let (rgba, width, height) = handler.get_raw_pixels();
        assert_eq!(width, 4);
        assert_eq!(height, 4);
        assert_eq!(rgba.len(), 4 * 4 * 4);

        // Check first pixel is now RGBA (converted from BGRA)
        assert_eq!(rgba[0], 0); // R
        assert_eq!(rgba[1], 0); // G
        assert_eq!(rgba[2], 255); // B
        assert_eq!(rgba[3], 255); // A
    }

    #[test]
    fn test_capture_screenshot() {
        let handler = OffScreenRenderHandler::with_size(4, 4);

        // Create a simple white buffer
        let buffer = vec![255u8; 4 * 4 * 4];
        let dirty_rects = vec![DirtyRect::full(4, 4)];
        handler.on_paint(0, &dirty_rects, &buffer, 4, 4);

        let screenshot = handler
            .capture_screenshot(ScreenshotFormat::Png, 80)
            .unwrap();
        assert!(!screenshot.is_empty());

        // Verify it's valid base64
        let decoded = BASE64.decode(&screenshot);
        assert!(decoded.is_ok());

        // Verify PNG signature
        let bytes = decoded.unwrap();
        assert!(bytes.len() > 8);
        assert_eq!(&bytes[0..8], &[0x89, 0x50, 0x4E, 0x47, 0x0D, 0x0A, 0x1A, 0x0A]);
    }

    #[test]
    fn test_capture_region() {
        let handler = OffScreenRenderHandler::with_size(8, 8);

        // Create a gradient-like buffer
        let mut buffer = vec![0u8; 8 * 8 * 4];
        for (i, pixel) in buffer.chunks_exact_mut(4).enumerate() {
            let val = (i * 4) as u8;
            pixel[0] = val; // B
            pixel[1] = val; // G
            pixel[2] = val; // R
            pixel[3] = 255; // A
        }

        let dirty_rects = vec![DirtyRect::full(8, 8)];
        handler.on_paint(0, &dirty_rects, &buffer, 8, 8);

        // Capture a 4x4 region from the center
        let screenshot = handler
            .capture_region(2, 2, 4, 4, ScreenshotFormat::Png, 80)
            .unwrap();
        assert!(!screenshot.is_empty());

        // Test out of bounds
        let result = handler.capture_region(6, 6, 4, 4, ScreenshotFormat::Png, 80);
        assert!(result.is_err());
    }

    #[test]
    fn test_dirty_rect_tracking() {
        let handler = OffScreenRenderHandler::with_size(100, 100);

        let dirty_rects = vec![
            DirtyRect::new(0, 0, 50, 50),
            DirtyRect::new(50, 50, 50, 50),
        ];

        let buffer = vec![0u8; 100 * 100 * 4];
        handler.on_paint(0, &dirty_rects, &buffer, 100, 100);

        let tracked = handler.get_dirty_rects();
        assert_eq!(tracked.len(), 2);

        handler.clear_dirty_rects();
        let cleared = handler.get_dirty_rects();
        assert!(cleared.is_empty());
    }

    #[test]
    fn test_paint_pending_flag() {
        let handler = OffScreenRenderHandler::with_size(100, 100);

        assert!(!handler.is_paint_pending());

        handler.set_paint_pending();
        assert!(handler.is_paint_pending());

        let buffer = vec![0u8; 100 * 100 * 4];
        handler.on_paint(0, &[], &buffer, 100, 100);
        assert!(!handler.is_paint_pending());
    }

    #[test]
    fn test_cef_render_handler_trait() {
        let handler: Box<dyn CefRenderHandler> =
            Box::new(OffScreenRenderHandler::with_size(800, 600));

        assert_eq!(handler.get_view_rect(), (0, 0, 800, 600));

        let info = handler.get_screen_info();
        assert_eq!(info.device_scale_factor, 1.0);

        let (sx, sy) = handler.get_screen_point(100, 100);
        assert_eq!(sx, 100);
        assert_eq!(sy, 100);
    }
}
