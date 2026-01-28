//! Screenshot capture functionality for browser automation.
//!
//! This module provides structures and functions for capturing screenshots
//! of browser tabs, with options for format, quality, and region selection.
//!
//! # Example
//!
//! ```rust,ignore
//! use ki_browser::browser::screenshot::{ScreenshotOptions, ScreenshotFormat, capture_screenshot};
//!
//! let options = ScreenshotOptions::new()
//!     .format(ScreenshotFormat::Png)
//!     .full_page(true);
//!
//! let screenshot = capture_screenshot(&tab, options).await?;
//! println!("Screenshot size: {} bytes", screenshot.data.len());
//! ```

use anyhow::{anyhow, Result};
use base64::{engine::general_purpose::STANDARD as BASE64, Engine as _};
use serde::{Deserialize, Serialize};

/// Supported image formats for screenshots.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub enum ScreenshotFormat {
    /// PNG format (lossless, larger file size).
    #[default]
    Png,
    /// JPEG format (lossy, smaller file size).
    Jpeg,
    /// WebP format (modern, efficient compression).
    WebP,
}

impl ScreenshotFormat {
    /// Returns the MIME type for this format.
    pub fn mime_type(&self) -> &'static str {
        match self {
            ScreenshotFormat::Png => "image/png",
            ScreenshotFormat::Jpeg => "image/jpeg",
            ScreenshotFormat::WebP => "image/webp",
        }
    }

    /// Returns the file extension for this format.
    pub fn extension(&self) -> &'static str {
        match self {
            ScreenshotFormat::Png => "png",
            ScreenshotFormat::Jpeg => "jpg",
            ScreenshotFormat::WebP => "webp",
        }
    }

    /// Returns whether this format supports transparency.
    pub fn supports_transparency(&self) -> bool {
        match self {
            ScreenshotFormat::Png | ScreenshotFormat::WebP => true,
            ScreenshotFormat::Jpeg => false,
        }
    }
}

impl std::fmt::Display for ScreenshotFormat {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ScreenshotFormat::Png => write!(f, "PNG"),
            ScreenshotFormat::Jpeg => write!(f, "JPEG"),
            ScreenshotFormat::WebP => write!(f, "WebP"),
        }
    }
}

/// Defines a rectangular region for clipping screenshots.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct ClipRegion {
    /// X coordinate of the top-left corner.
    pub x: f64,

    /// Y coordinate of the top-left corner.
    pub y: f64,

    /// Width of the region.
    pub width: f64,

    /// Height of the region.
    pub height: f64,

    /// Scale factor for the screenshot (default: 1.0).
    pub scale: f64,
}

impl ClipRegion {
    /// Creates a new ClipRegion with default scale.
    pub fn new(x: f64, y: f64, width: f64, height: f64) -> Self {
        Self {
            x,
            y,
            width,
            height,
            scale: 1.0,
        }
    }

    /// Creates a ClipRegion with a specific scale factor.
    pub fn with_scale(x: f64, y: f64, width: f64, height: f64, scale: f64) -> Self {
        Self {
            x,
            y,
            width,
            height,
            scale,
        }
    }

    /// Returns the area of the region.
    pub fn area(&self) -> f64 {
        self.width * self.height
    }

    /// Validates the clip region dimensions.
    pub fn is_valid(&self) -> bool {
        self.width > 0.0 && self.height > 0.0 && self.scale > 0.0
    }

    /// Returns the scaled dimensions.
    pub fn scaled_dimensions(&self) -> (f64, f64) {
        (self.width * self.scale, self.height * self.scale)
    }
}

impl Default for ClipRegion {
    fn default() -> Self {
        Self {
            x: 0.0,
            y: 0.0,
            width: 1920.0,
            height: 1080.0,
            scale: 1.0,
        }
    }
}

/// Options for configuring screenshot capture.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ScreenshotOptions {
    /// Image format for the screenshot.
    pub format: ScreenshotFormat,

    /// Quality for JPEG/WebP formats (0-100). Ignored for PNG.
    pub quality: u8,

    /// Whether to capture the full scrollable page.
    pub full_page: bool,

    /// Optional clip region for partial screenshots.
    pub clip_region: Option<ClipRegion>,

    /// Whether to capture from the surface rather than view.
    pub from_surface: bool,

    /// Whether to capture beyond the viewport.
    pub capture_beyond_viewport: bool,

    /// Whether to optimize for speed over quality.
    pub optimize_for_speed: bool,
}

impl Default for ScreenshotOptions {
    fn default() -> Self {
        Self {
            format: ScreenshotFormat::Png,
            quality: 80,
            full_page: false,
            clip_region: None,
            from_surface: true,
            capture_beyond_viewport: false,
            optimize_for_speed: false,
        }
    }
}

impl ScreenshotOptions {
    /// Creates new ScreenshotOptions with defaults.
    pub fn new() -> Self {
        Self::default()
    }

    /// Sets the image format.
    pub fn format(mut self, format: ScreenshotFormat) -> Self {
        self.format = format;
        self
    }

    /// Sets the quality (for JPEG/WebP).
    pub fn quality(mut self, quality: u8) -> Self {
        self.quality = quality.min(100);
        self
    }

    /// Sets whether to capture the full page.
    pub fn full_page(mut self, full_page: bool) -> Self {
        self.full_page = full_page;
        self
    }

    /// Sets a clip region for the screenshot.
    pub fn clip(mut self, region: ClipRegion) -> Self {
        self.clip_region = Some(region);
        self
    }

    /// Sets clipping coordinates directly.
    pub fn clip_rect(mut self, x: f64, y: f64, width: f64, height: f64) -> Self {
        self.clip_region = Some(ClipRegion::new(x, y, width, height));
        self
    }

    /// Enables optimization for speed.
    pub fn optimize_for_speed(mut self) -> Self {
        self.optimize_for_speed = true;
        self
    }

    /// Validates the options.
    pub fn validate(&self) -> Result<()> {
        if let Some(ref clip) = self.clip_region {
            if !clip.is_valid() {
                return Err(anyhow!("Invalid clip region dimensions"));
            }
        }

        if self.quality > 100 {
            return Err(anyhow!("Quality must be between 0 and 100"));
        }

        Ok(())
    }
}

/// Result of a screenshot capture operation.
#[derive(Debug, Clone)]
pub struct Screenshot {
    /// Raw image data (base64 encoded).
    pub data: String,

    /// Image format.
    pub format: ScreenshotFormat,

    /// Width of the captured image in pixels.
    pub width: u32,

    /// Height of the captured image in pixels.
    pub height: u32,

    /// Device scale factor used during capture.
    pub device_scale_factor: f64,
}

impl Screenshot {
    /// Creates a new Screenshot from base64 encoded data.
    pub fn new(
        data: String,
        format: ScreenshotFormat,
        width: u32,
        height: u32,
        device_scale_factor: f64,
    ) -> Self {
        Self {
            data,
            format,
            width,
            height,
            device_scale_factor,
        }
    }

    /// Decodes the base64 data to raw bytes.
    pub fn decode(&self) -> Result<Vec<u8>> {
        BASE64.decode(&self.data).map_err(|e| anyhow!("Failed to decode screenshot: {}", e))
    }

    /// Returns the data as a data URL.
    pub fn to_data_url(&self) -> String {
        format!("data:{};base64,{}", self.format.mime_type(), self.data)
    }

    /// Returns the approximate size in bytes.
    pub fn size_bytes(&self) -> usize {
        // Base64 encodes 3 bytes into 4 characters
        self.data.len() * 3 / 4
    }

    /// Returns the image dimensions as a tuple.
    pub fn dimensions(&self) -> (u32, u32) {
        (self.width, self.height)
    }
}

/// Captures a mock screenshot for testing purposes.
///
/// This function generates a simple placeholder image encoded in base64.
/// In a real implementation, this would interact with the browser to capture
/// the actual page content.
///
/// # Arguments
///
/// * `options` - Screenshot configuration options
///
/// # Returns
///
/// A Screenshot with mock data suitable for testing.
pub fn capture_mock_screenshot(options: &ScreenshotOptions) -> Result<Screenshot> {
    options.validate()?;

    // Determine dimensions
    let (width, height) = if let Some(ref clip) = options.clip_region {
        let (w, h) = clip.scaled_dimensions();
        (w as u32, h as u32)
    } else if options.full_page {
        (1920, 3000) // Simulated full page height
    } else {
        (1920, 1080) // Default viewport
    };

    // Generate a minimal placeholder PNG (1x1 transparent pixel)
    // In a real implementation, this would be the actual screenshot data
    let placeholder_png = create_placeholder_image(options.format)?;
    let data = BASE64.encode(&placeholder_png);

    Ok(Screenshot::new(data, options.format, width, height, 1.0))
}

/// Creates a minimal placeholder image for the specified format.
fn create_placeholder_image(format: ScreenshotFormat) -> Result<Vec<u8>> {
    match format {
        ScreenshotFormat::Png => {
            // Minimal valid PNG (1x1 transparent pixel)
            Ok(vec![
                0x89, 0x50, 0x4E, 0x47, 0x0D, 0x0A, 0x1A, 0x0A, // PNG signature
                0x00, 0x00, 0x00, 0x0D, // IHDR length
                0x49, 0x48, 0x44, 0x52, // IHDR
                0x00, 0x00, 0x00, 0x01, // width: 1
                0x00, 0x00, 0x00, 0x01, // height: 1
                0x08, 0x06, // bit depth: 8, color type: RGBA
                0x00, 0x00, 0x00, // compression, filter, interlace
                0x1F, 0x15, 0xC4, 0x89, // IHDR CRC
                0x00, 0x00, 0x00, 0x0A, // IDAT length
                0x49, 0x44, 0x41, 0x54, // IDAT
                0x78, 0x9C, 0x63, 0x00, 0x01, 0x00, 0x00, 0x05, 0x00, 0x01, // compressed data
                0x0D, 0x0A, 0x2D, 0xB4, // IDAT CRC
                0x00, 0x00, 0x00, 0x00, // IEND length
                0x49, 0x45, 0x4E, 0x44, // IEND
                0xAE, 0x42, 0x60, 0x82, // IEND CRC
            ])
        }
        ScreenshotFormat::Jpeg => {
            // Minimal valid JPEG (1x1 white pixel)
            Ok(vec![
                0xFF, 0xD8, 0xFF, 0xE0, 0x00, 0x10, 0x4A, 0x46, 0x49, 0x46, 0x00, 0x01, 0x01, 0x00,
                0x00, 0x01, 0x00, 0x01, 0x00, 0x00, 0xFF, 0xDB, 0x00, 0x43, 0x00, 0x08, 0x06, 0x06,
                0x07, 0x06, 0x05, 0x08, 0x07, 0x07, 0x07, 0x09, 0x09, 0x08, 0x0A, 0x0C, 0x14, 0x0D,
                0x0C, 0x0B, 0x0B, 0x0C, 0x19, 0x12, 0x13, 0x0F, 0x14, 0x1D, 0x1A, 0x1F, 0x1E, 0x1D,
                0x1A, 0x1C, 0x1C, 0x20, 0x24, 0x2E, 0x27, 0x20, 0x22, 0x2C, 0x23, 0x1C, 0x1C, 0x28,
                0x37, 0x29, 0x2C, 0x30, 0x31, 0x34, 0x34, 0x34, 0x1F, 0x27, 0x39, 0x3D, 0x38, 0x32,
                0x3C, 0x2E, 0x33, 0x34, 0x32, 0xFF, 0xC0, 0x00, 0x0B, 0x08, 0x00, 0x01, 0x00, 0x01,
                0x01, 0x01, 0x11, 0x00, 0xFF, 0xC4, 0x00, 0x1F, 0x00, 0x00, 0x01, 0x05, 0x01, 0x01,
                0x01, 0x01, 0x01, 0x01, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x01, 0x02,
                0x03, 0x04, 0x05, 0x06, 0x07, 0x08, 0x09, 0x0A, 0x0B, 0xFF, 0xC4, 0x00, 0xB5, 0x10,
                0x00, 0x02, 0x01, 0x03, 0x03, 0x02, 0x04, 0x03, 0x05, 0x05, 0x04, 0x04, 0x00, 0x00,
                0x01, 0x7D, 0x01, 0x02, 0x03, 0x00, 0x04, 0x11, 0x05, 0x12, 0x21, 0x31, 0x41, 0x06,
                0x13, 0x51, 0x61, 0x07, 0x22, 0x71, 0x14, 0x32, 0x81, 0x91, 0xA1, 0x08, 0x23, 0x42,
                0xB1, 0xC1, 0x15, 0x52, 0xD1, 0xF0, 0x24, 0x33, 0x62, 0x72, 0x82, 0x09, 0x0A, 0x16,
                0x17, 0x18, 0x19, 0x1A, 0x25, 0x26, 0x27, 0x28, 0x29, 0x2A, 0x34, 0x35, 0x36, 0x37,
                0x38, 0x39, 0x3A, 0x43, 0x44, 0x45, 0x46, 0x47, 0x48, 0x49, 0x4A, 0x53, 0x54, 0x55,
                0x56, 0x57, 0x58, 0x59, 0x5A, 0x63, 0x64, 0x65, 0x66, 0x67, 0x68, 0x69, 0x6A, 0x73,
                0x74, 0x75, 0x76, 0x77, 0x78, 0x79, 0x7A, 0x83, 0x84, 0x85, 0x86, 0x87, 0x88, 0x89,
                0x8A, 0x92, 0x93, 0x94, 0x95, 0x96, 0x97, 0x98, 0x99, 0x9A, 0xA2, 0xA3, 0xA4, 0xA5,
                0xA6, 0xA7, 0xA8, 0xA9, 0xAA, 0xB2, 0xB3, 0xB4, 0xB5, 0xB6, 0xB7, 0xB8, 0xB9, 0xBA,
                0xC2, 0xC3, 0xC4, 0xC5, 0xC6, 0xC7, 0xC8, 0xC9, 0xCA, 0xD2, 0xD3, 0xD4, 0xD5, 0xD6,
                0xD7, 0xD8, 0xD9, 0xDA, 0xE1, 0xE2, 0xE3, 0xE4, 0xE5, 0xE6, 0xE7, 0xE8, 0xE9, 0xEA,
                0xF1, 0xF2, 0xF3, 0xF4, 0xF5, 0xF6, 0xF7, 0xF8, 0xF9, 0xFA, 0xFF, 0xDA, 0x00, 0x08,
                0x01, 0x01, 0x00, 0x00, 0x3F, 0x00, 0xFB, 0xD5, 0xDB, 0x20, 0xA8, 0xF1, 0x7E, 0xA8,
                0xA0, 0x02, 0x80, 0x0A, 0x00, 0x28, 0x00, 0xA0, 0x02, 0x80, 0xFF, 0xD9,
            ])
        }
        ScreenshotFormat::WebP => {
            // Minimal valid WebP (1x1 white pixel)
            Ok(vec![
                0x52, 0x49, 0x46, 0x46, // RIFF
                0x1A, 0x00, 0x00, 0x00, // File size - 8
                0x57, 0x45, 0x42, 0x50, // WEBP
                0x56, 0x50, 0x38, 0x4C, // VP8L
                0x0D, 0x00, 0x00, 0x00, // Chunk size
                0x2F, 0x00, 0x00, 0x00, // Signature
                0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, // Minimal VP8L data
            ])
        }
    }
}

/// Trait for screenshot capture implementations.
///
/// This trait allows different browser engines to provide their own
/// screenshot capture logic while maintaining a consistent interface.
#[async_trait::async_trait]
pub trait ScreenshotCapture: Send + Sync {
    /// Captures a screenshot with the given options.
    ///
    /// # Arguments
    ///
    /// * `options` - Screenshot configuration
    ///
    /// # Returns
    ///
    /// A Screenshot containing the captured image data.
    async fn capture(&self, options: &ScreenshotOptions) -> Result<Screenshot>;

    /// Captures a screenshot of a specific element.
    ///
    /// # Arguments
    ///
    /// * `selector` - CSS selector for the element to capture
    /// * `options` - Screenshot configuration
    ///
    /// # Returns
    ///
    /// A Screenshot of the element, or an error if not found.
    async fn capture_element(
        &self,
        selector: &str,
        options: &ScreenshotOptions,
    ) -> Result<Screenshot>;
}

/// Mock implementation of ScreenshotCapture for testing.
pub struct MockScreenshotCapture;

impl Default for MockScreenshotCapture {
    fn default() -> Self {
        Self::new()
    }
}

impl MockScreenshotCapture {
    /// Creates a new MockScreenshotCapture.
    pub fn new() -> Self {
        Self
    }
}

#[async_trait::async_trait]
impl ScreenshotCapture for MockScreenshotCapture {
    async fn capture(&self, options: &ScreenshotOptions) -> Result<Screenshot> {
        capture_mock_screenshot(options)
    }

    async fn capture_element(
        &self,
        _selector: &str,
        options: &ScreenshotOptions,
    ) -> Result<Screenshot> {
        // For mock, just return a smaller screenshot simulating an element
        let mut element_options = options.clone();
        element_options.clip_region = Some(ClipRegion::new(0.0, 0.0, 200.0, 100.0));
        capture_mock_screenshot(&element_options)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_screenshot_format() {
        assert_eq!(ScreenshotFormat::Png.mime_type(), "image/png");
        assert_eq!(ScreenshotFormat::Jpeg.extension(), "jpg");
        assert!(ScreenshotFormat::Png.supports_transparency());
        assert!(!ScreenshotFormat::Jpeg.supports_transparency());
    }

    #[test]
    fn test_clip_region() {
        let region = ClipRegion::new(10.0, 20.0, 100.0, 50.0);
        assert!(region.is_valid());
        assert_eq!(region.area(), 5000.0);
        assert_eq!(region.scaled_dimensions(), (100.0, 50.0));

        let scaled = ClipRegion::with_scale(0.0, 0.0, 100.0, 100.0, 2.0);
        assert_eq!(scaled.scaled_dimensions(), (200.0, 200.0));

        let invalid = ClipRegion::new(0.0, 0.0, 0.0, 100.0);
        assert!(!invalid.is_valid());
    }

    #[test]
    fn test_screenshot_options_builder() {
        let options = ScreenshotOptions::new()
            .format(ScreenshotFormat::Jpeg)
            .quality(90)
            .full_page(true)
            .clip_rect(0.0, 0.0, 800.0, 600.0);

        assert_eq!(options.format, ScreenshotFormat::Jpeg);
        assert_eq!(options.quality, 90);
        assert!(options.full_page);
        assert!(options.clip_region.is_some());
    }

    #[test]
    fn test_screenshot_options_validation() {
        let valid = ScreenshotOptions::new();
        assert!(valid.validate().is_ok());

        let mut invalid = ScreenshotOptions::new();
        invalid.clip_region = Some(ClipRegion::new(0.0, 0.0, -100.0, 100.0));
        assert!(invalid.validate().is_err());
    }

    #[test]
    fn test_capture_mock_screenshot() {
        let options = ScreenshotOptions::new();
        let screenshot = capture_mock_screenshot(&options).unwrap();

        assert_eq!(screenshot.format, ScreenshotFormat::Png);
        assert!(!screenshot.data.is_empty());
        assert!(screenshot.decode().is_ok());

        let data_url = screenshot.to_data_url();
        assert!(data_url.starts_with("data:image/png;base64,"));
    }

    #[test]
    fn test_screenshot_with_clip() {
        let options = ScreenshotOptions::new()
            .clip_rect(100.0, 100.0, 400.0, 300.0);

        let screenshot = capture_mock_screenshot(&options).unwrap();
        assert_eq!(screenshot.width, 400);
        assert_eq!(screenshot.height, 300);
    }

    #[tokio::test]
    async fn test_mock_screenshot_capture() {
        let capture = MockScreenshotCapture::new();

        let options = ScreenshotOptions::new()
            .format(ScreenshotFormat::Jpeg)
            .quality(85);

        let screenshot = capture.capture(&options).await.unwrap();
        assert_eq!(screenshot.format, ScreenshotFormat::Jpeg);

        let element_screenshot = capture
            .capture_element("#test", &ScreenshotOptions::new())
            .await
            .unwrap();
        assert_eq!(element_screenshot.width, 200);
        assert_eq!(element_screenshot.height, 100);
    }

    #[test]
    fn test_placeholder_images() {
        // Verify all format placeholders are valid
        let png = create_placeholder_image(ScreenshotFormat::Png).unwrap();
        assert!(!png.is_empty());
        assert_eq!(&png[0..8], &[0x89, 0x50, 0x4E, 0x47, 0x0D, 0x0A, 0x1A, 0x0A]);

        let jpeg = create_placeholder_image(ScreenshotFormat::Jpeg).unwrap();
        assert!(!jpeg.is_empty());
        assert_eq!(&jpeg[0..2], &[0xFF, 0xD8]);

        let webp = create_placeholder_image(ScreenshotFormat::WebP).unwrap();
        assert!(!webp.is_empty());
        assert_eq!(&webp[0..4], b"RIFF");
    }
}
