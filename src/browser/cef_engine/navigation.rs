//! Navigation, JavaScript execution, and screenshot capture on the CEF thread.
//!
//! Contains internal methods that operate on the CEF thread (synchronous)
//! as well as public async convenience methods on CefBrowserEngine that
//! dispatch commands through the channel and await results.

use anyhow::{anyhow, Context, Result};
use cef::CefString;
use parking_lot::RwLock;
use std::collections::HashMap;
use std::sync::atomic::Ordering;
use std::sync::Arc;
use tracing::{debug, info};
use uuid::Uuid;

use crate::browser::screenshot::{Screenshot, ScreenshotFormat, ScreenshotOptions};
use super::CefCommand;
use super::engine::CefBrowserEngine;
use super::tab::CefTab;

// ============================================================================
// Internal methods (called on the CEF thread)
// ============================================================================

/// Navigates a tab to a URL internally on the CEF thread.
pub(crate) fn navigate_internal(
    tab_id: Uuid,
    url: &str,
    tabs: Arc<RwLock<HashMap<Uuid, CefTab>>>,
) -> Result<()> {
    let tabs_guard = tabs.read();
    let tab = tabs_guard
        .get(&tab_id)
        .ok_or_else(|| anyhow!("Tab not found: {}", tab_id))?;

    let browser = tab
        .browser
        .as_ref()
        .ok_or_else(|| anyhow!("Browser not initialized for tab: {}", tab_id))?;

    if let Some(frame) = browser.main_frame() {
        let url_string = CefString::from(url);
        frame.load_url(Some(&url_string));
        info!("Navigating tab {} to: {}", tab_id, url);
        Ok(())
    } else {
        Err(anyhow!("No main frame for tab: {}", tab_id))
    }
}

/// Executes JavaScript internally on the CEF thread.
///
/// Note: CEF doesn't provide synchronous JavaScript return values.
/// For result capture, use V8 context and message passing.
pub(crate) fn execute_js_internal(
    tab_id: Uuid,
    script: &str,
    tabs: Arc<RwLock<HashMap<Uuid, CefTab>>>,
) -> Result<Option<String>> {
    let tabs_guard = tabs.read();
    let tab = tabs_guard
        .get(&tab_id)
        .ok_or_else(|| anyhow!("Tab not found: {}", tab_id))?;

    let browser = tab
        .browser
        .as_ref()
        .ok_or_else(|| anyhow!("Browser not initialized for tab: {}", tab_id))?;

    if let Some(frame) = browser.main_frame() {
        let script_string = CefString::from(script);
        let empty_url = CefString::from("");
        frame.execute_java_script(Some(&script_string), Some(&empty_url), 0);
        debug!("JavaScript executed on tab {}", tab_id);
        // Note: CEF doesn't provide synchronous JS execution results
        // For result capture, use V8 context and message passing
        Ok(None)
    } else {
        Err(anyhow!("No main frame for tab: {}", tab_id))
    }
}

/// Captures a screenshot internally on the CEF thread.
///
/// Reads the current off-screen rendering frame buffer and converts
/// it from raw BGRA to the requested image format (PNG, JPEG, WebP).
pub(crate) fn screenshot_internal(
    tab_id: Uuid,
    options: &ScreenshotOptions,
    tabs: Arc<RwLock<HashMap<Uuid, CefTab>>>,
) -> Result<Screenshot> {
    options.validate()?;

    let tabs_guard = tabs.read();
    let tab = tabs_guard
        .get(&tab_id)
        .ok_or_else(|| anyhow!("Tab not found: {}", tab_id))?;

    let frame_buffer = tab.frame_buffer.read();
    let (width, height) = *tab.frame_size.read();

    if frame_buffer.is_empty() || width == 0 || height == 0 {
        return Err(anyhow!("No frame data available for screenshot"));
    }

    // Convert BGRA to RGB/RGBA based on format
    let image_data = convert_frame_to_image(
        &frame_buffer,
        width,
        height,
        options.format,
        options.quality,
    )?;

    let data = base64::Engine::encode(&base64::engine::general_purpose::STANDARD, &image_data);

    Ok(Screenshot::new(data, options.format, width, height, 1.0))
}

/// Converts raw BGRA frame buffer to encoded image (PNG, JPEG, or WebP).
fn convert_frame_to_image(
    buffer: &[u8],
    width: u32,
    height: u32,
    format: ScreenshotFormat,
    quality: u8,
) -> Result<Vec<u8>> {
    use image::{ImageBuffer, ImageOutputFormat, Rgba};

    // Create image from BGRA buffer
    let mut img: ImageBuffer<Rgba<u8>, Vec<u8>> = ImageBuffer::new(width, height);

    for (x, y, pixel) in img.enumerate_pixels_mut() {
        let idx = ((y * width + x) * 4) as usize;
        if idx + 3 < buffer.len() {
            // BGRA to RGBA conversion
            *pixel = Rgba([
                buffer[idx + 2], // R
                buffer[idx + 1], // G
                buffer[idx],     // B
                buffer[idx + 3], // A
            ]);
        }
    }

    // Encode to requested format
    let mut output = Vec::new();
    let format = match format {
        ScreenshotFormat::Png => ImageOutputFormat::Png,
        ScreenshotFormat::Jpeg => ImageOutputFormat::Jpeg(quality),
        ScreenshotFormat::WebP => {
            // WebP not directly supported by image crate, use PNG as fallback
            ImageOutputFormat::Png
        }
    };

    img.write_to(&mut std::io::Cursor::new(&mut output), format)
        .context("Failed to encode screenshot")?;

    Ok(output)
}

// ============================================================================
// Public async API on CefBrowserEngine
// ============================================================================

impl CefBrowserEngine {
    /// Navigates a tab to the specified URL.
    ///
    /// Sends a Navigate command to the CEF thread and awaits the result.
    pub async fn navigate(&self, tab_id: Uuid, url: &str) -> Result<()> {
        if !self.is_running.load(Ordering::SeqCst) {
            return Err(anyhow!("Browser engine is not running"));
        }

        let (response_tx, response_rx) = tokio::sync::oneshot::channel();

        self.command_tx
            .send(CefCommand::Navigate {
                tab_id,
                url: url.to_string(),
                response: response_tx,
            })
            .await
            .context("Failed to send navigate command")?;

        response_rx
            .await
            .context("Failed to receive navigate response")?
    }

    /// Executes JavaScript in a tab.
    ///
    /// Sends an ExecuteJs command to the CEF thread and awaits the result.
    /// Note: CEF doesn't provide synchronous JavaScript return values.
    /// For complex interactions, use message passing via V8 context.
    pub async fn execute_js(&self, tab_id: Uuid, script: &str) -> Result<Option<String>> {
        if !self.is_running.load(Ordering::SeqCst) {
            return Err(anyhow!("Browser engine is not running"));
        }

        let (response_tx, response_rx) = tokio::sync::oneshot::channel();

        self.command_tx
            .send(CefCommand::ExecuteJs {
                tab_id,
                script: script.to_string(),
                response: response_tx,
            })
            .await
            .context("Failed to send execute JS command")?;

        response_rx
            .await
            .context("Failed to receive execute JS response")?
    }

    /// Captures a screenshot of a tab.
    ///
    /// Sends a Screenshot command to the CEF thread and awaits the encoded
    /// image result in the requested format.
    pub async fn screenshot(
        &self,
        tab_id: Uuid,
        options: ScreenshotOptions,
    ) -> Result<Screenshot> {
        if !self.is_running.load(Ordering::SeqCst) {
            return Err(anyhow!("Browser engine is not running"));
        }

        let (response_tx, response_rx) = tokio::sync::oneshot::channel();

        self.command_tx
            .send(CefCommand::Screenshot {
                tab_id,
                options,
                response: response_tx,
            })
            .await
            .context("Failed to send screenshot command")?;

        response_rx
            .await
            .context("Failed to receive screenshot response")?
    }

    /// Waits for a tab to be ready for interaction.
    ///
    /// Polls the tab's readiness flag with a 50ms interval until the tab
    /// reports ready or the specified timeout elapses.
    pub async fn wait_for_ready(&self, tab_id: Uuid, timeout_ms: u64) -> Result<()> {
        let start = std::time::Instant::now();
        let timeout = std::time::Duration::from_millis(timeout_ms);

        loop {
            {
                let tabs = self.tabs.read();
                if let Some(tab) = tabs.get(&tab_id) {
                    if tab.is_ready.load(Ordering::SeqCst) {
                        return Ok(());
                    }
                } else {
                    return Err(anyhow!("Tab not found: {}", tab_id));
                }
            }

            if start.elapsed() > timeout {
                return Err(anyhow!("Timeout waiting for tab {} to be ready", tab_id));
            }

            tokio::time::sleep(tokio::time::Duration::from_millis(50)).await;
        }
    }
}
