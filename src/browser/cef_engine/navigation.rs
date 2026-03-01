//! Navigation, JavaScript execution, and screenshot capture on the CEF thread.
//!
//! Contains internal methods that operate on the CEF thread (synchronous)
//! as well as public async convenience methods on CefBrowserEngine that
//! dispatch commands through the channel and await results.

use anyhow::{anyhow, Context, Result};
use cef::CefString;
use cef::{ImplBrowser, ImplBrowserHost, ImplFrame};
use parking_lot::RwLock;
use std::collections::HashMap;
use std::sync::atomic::Ordering;
use std::sync::Arc;
use tokio::sync::oneshot;
use tracing::{debug, info};
use uuid::Uuid;

use crate::browser::screenshot::{Screenshot, ScreenshotFormat, ScreenshotOptions};
use super::CefCommand;
use super::engine::CefBrowserEngine;
use super::tab::CefTab;
use super::JS_RESULT_STORE;

// ============================================================================
// Internal methods (called on the CEF thread)
// ============================================================================

/// Navigates a tab to a URL internally on the CEF thread.
/// IMPORTANT: Must NOT hold the tabs RwLock while calling CEF methods,
/// because CEF may fire callbacks (e.g. on_loading_state_change) that
/// need a write lock on tabs -- causing a deadlock on the same thread.
pub(crate) fn navigate_internal(
    tab_id: Uuid,
    url: &str,
    tabs: Arc<RwLock<HashMap<Uuid, CefTab>>>,
) -> Result<()> {
    // Clone the browser reference, then release the lock BEFORE calling CEF.
    let browser = {
        let tabs_guard = tabs.read();
        let tab = tabs_guard
            .get(&tab_id)
            .ok_or_else(|| anyhow!("Tab not found: {}", tab_id))?;
        tab.browser.clone()
            .ok_or_else(|| anyhow!("Browser not initialized for tab: {}", tab_id))?
    }; // Read lock released here.

    if let Some(frame) = browser.main_frame() {
        let url_string = CefString::from(url);
        frame.load_url(Some(&url_string));
        info!("Navigating tab {} to: {}", tab_id, url);
        Ok(())
    } else {
        Err(anyhow!("No main frame for tab: {}", tab_id))
    }
}

/// Navigates the browser back in history on the CEF thread.
pub(crate) fn go_back_internal(
    tab_id: Uuid,
    tabs: Arc<RwLock<HashMap<Uuid, CefTab>>>,
) -> Result<()> {
    let browser = {
        let tabs_guard = tabs.read();
        let tab = tabs_guard
            .get(&tab_id)
            .ok_or_else(|| anyhow!("Tab not found: {}", tab_id))?;
        tab.browser.clone()
            .ok_or_else(|| anyhow!("Browser not initialized for tab: {}", tab_id))?
    };

    browser.go_back();
    info!("Go back on tab {}", tab_id);
    Ok(())
}

/// Navigates the browser forward in history on the CEF thread.
pub(crate) fn go_forward_internal(
    tab_id: Uuid,
    tabs: Arc<RwLock<HashMap<Uuid, CefTab>>>,
) -> Result<()> {
    let browser = {
        let tabs_guard = tabs.read();
        let tab = tabs_guard
            .get(&tab_id)
            .ok_or_else(|| anyhow!("Tab not found: {}", tab_id))?;
        tab.browser.clone()
            .ok_or_else(|| anyhow!("Browser not initialized for tab: {}", tab_id))?
    };

    browser.go_forward();
    info!("Go forward on tab {}", tab_id);
    Ok(())
}

/// Resizes the CEF viewport for a tab and notifies the browser host.
///
/// Updates the shared viewport dimensions (read by the render handler's
/// `view_rect()` and `screen_info()` callbacks) then calls `was_resized()`
/// on the browser host so CEF re-renders at the new size.
pub(crate) fn resize_viewport_internal(
    tab_id: Uuid,
    width: u32,
    height: u32,
    tabs: Arc<RwLock<HashMap<Uuid, CefTab>>>,
) -> Result<()> {
    let (browser, viewport_size) = {
        let tabs_guard = tabs.read();
        let tab = tabs_guard
            .get(&tab_id)
            .ok_or_else(|| anyhow!("Tab not found: {}", tab_id))?;
        let browser = tab.browser.clone()
            .ok_or_else(|| anyhow!("Browser not initialized for tab: {}", tab_id))?;
        (browser, tab.viewport_size.clone())
    };

    // Update the shared viewport dimensions before notifying CEF.
    // The render handler reads these in view_rect() and screen_info().
    {
        let mut vp = viewport_size.write();
        *vp = (width, height);
    }

    if let Some(host) = browser.host() {
        host.was_resized();
        info!("Viewport resized for tab {}: {}x{}", tab_id, width, height);
        Ok(())
    } else {
        Err(anyhow!("No browser host for tab: {}", tab_id))
    }
}

/// Executes JavaScript internally on the CEF thread.
pub(crate) fn execute_js_internal(
    tab_id: Uuid,
    script: &str,
    tabs: Arc<RwLock<HashMap<Uuid, CefTab>>>,
) -> Result<Option<String>> {
    let browser = {
        let tabs_guard = tabs.read();
        let tab = tabs_guard
            .get(&tab_id)
            .ok_or_else(|| anyhow!("Tab not found: {}", tab_id))?;
        tab.browser.clone()
            .ok_or_else(|| anyhow!("Browser not initialized for tab: {}", tab_id))?
    };

    if let Some(frame) = browser.main_frame() {
        let script_string = CefString::from(script);
        let empty_url = CefString::from("");
        frame.execute_java_script(Some(&script_string), Some(&empty_url), 0);
        debug!("JavaScript executed on tab {}", tab_id);
        Ok(None)
    } else {
        Err(anyhow!("No main frame for tab: {}", tab_id))
    }
}

/// Executes JavaScript and waits for the result via console.log interception.
///
/// This wraps the user script in a console.log call with a special prefix
/// ("KI_RESULT:<id>:<json>") that the DisplayHandler intercepts. This approach
/// works reliably in single-process mode where CEF MessageRouter IPC fails.
pub(crate) fn execute_js_with_result_internal(
    tab_id: Uuid,
    script: &str,
    tabs: Arc<RwLock<HashMap<Uuid, CefTab>>>,
) -> Result<Option<String>> {
    let browser = {
        let tabs_guard = tabs.read();
        let tab = tabs_guard
            .get(&tab_id)
            .ok_or_else(|| anyhow!("Tab not found: {}", tab_id))?;
        tab.browser.clone()
            .ok_or_else(|| anyhow!("Browser not initialized for tab: {}", tab_id))?
    };

    if let Some(frame) = browser.main_frame() {
        // Use a random i64 as query ID to correlate the console.log response.
        let query_id = rand::random::<u32>() as i64;

        // Wrap the user script: evaluate it, then send the JSON-serialised
        // result back via console.log with KI_RESULT prefix so the
        // DisplayHandler can capture it.
        // Strategy: try as expression first (return (SCRIPT)), fall back to
        // statement body (SCRIPT) for multi-statement scripts with own return.
        let wrapped = format!(
            r#"(function(){{var __r;try{{__r=(new Function('return ('+{script_escaped}+')'))()}}catch(_e1){{try{{__r=(new Function({script_escaped}))()}}catch(e){{__r={{"__error":e.message}}}}}};console.log('KI_RESULT:{qid}:'+JSON.stringify(__r))}})()"#,
            script_escaped = serde_json::to_string(script).unwrap_or_else(|_| format!("\"{}\"", script)),
            qid = query_id,
        );

        let (tx, rx) = std::sync::mpsc::channel::<Result<String, String>>();
        JS_RESULT_STORE.lock().insert(query_id, tx);

        let script_cef = CefString::from(wrapped.as_str());
        let empty_url = CefString::from("");
        frame.execute_java_script(Some(&script_cef), Some(&empty_url), 0);

        // Pump the CEF message loop while waiting for the cefQuery callback.
        // Without pumping we would deadlock because the JS response is
        // delivered on this same CEF thread.
        let start = std::time::Instant::now();
        loop {
            match rx.try_recv() {
                Ok(Ok(result)) => {
                    if result == "null" || result == "undefined" {
                        return Ok(None);
                    }
                    return Ok(Some(result));
                }
                Ok(Err(e)) => {
                    return Err(anyhow!("JS error: {}", e));
                }
                Err(std::sync::mpsc::TryRecvError::Empty) => {
                    if start.elapsed() > std::time::Duration::from_secs(10) {
                        JS_RESULT_STORE.lock().remove(&query_id);
                        return Err(anyhow!("JS execution timeout (10s) for tab {}", tab_id));
                    }
                    cef::do_message_loop_work();
                    std::thread::sleep(std::time::Duration::from_millis(10));
                }
                Err(std::sync::mpsc::TryRecvError::Disconnected) => {
                    JS_RESULT_STORE.lock().remove(&query_id);
                    return Err(anyhow!("JS result channel disconnected for tab {}", tab_id));
                }
            }
        }
    } else {
        Err(anyhow!("No main frame for tab: {}", tab_id))
    }
}

/// Captures a screenshot internally on the CEF thread.
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
    pub async fn navigate(&self, tab_id: Uuid, url: &str) -> Result<()> {
        if !self.is_running.load(Ordering::SeqCst) {
            return Err(anyhow!("Browser engine is not running"));
        }

        let (response_tx, response_rx) = oneshot::channel();

        self.command_tx
            .send(CefCommand::Navigate {
                tab_id,
                url: url.to_string(),
                response: response_tx,
            })
            .map_err(|_| anyhow!("Failed to send navigate command"))?;

        response_rx.await.context("Failed to receive navigate response")?
    }

    /// Executes JavaScript in a tab.
    ///
    /// Note: CEF doesn't provide synchronous JavaScript return values.
    /// For complex interactions, use message passing via V8 context.
    pub async fn execute_js(&self, tab_id: Uuid, script: &str) -> Result<Option<String>> {
        if !self.is_running.load(Ordering::SeqCst) {
            return Err(anyhow!("Browser engine is not running"));
        }

        let (response_tx, response_rx) = oneshot::channel();

        self.command_tx
            .send(CefCommand::ExecuteJs {
                tab_id,
                script: script.to_string(),
                response: response_tx,
            })
            .map_err(|_| anyhow!("Failed to send execute JS command"))?;

        response_rx.await.context("Failed to receive execute JS response")?
    }

    /// Executes JavaScript in a tab and waits for the return value via CEF MessageRouter.
    ///
    /// Unlike `execute_js`, this method actually captures and returns the JS
    /// return value by routing it through `window.cefQuery`. The CEF message
    /// loop is pumped on the command thread while waiting so no deadlock occurs.
    pub async fn execute_js_with_result(&self, tab_id: Uuid, script: &str) -> Result<Option<String>> {
        if !self.is_running.load(Ordering::SeqCst) {
            return Err(anyhow!("Browser engine is not running"));
        }

        let (response_tx, response_rx) = oneshot::channel();

        self.command_tx
            .send(CefCommand::ExecuteJsWithResult {
                tab_id,
                script: script.to_string(),
                response: response_tx,
            })
            .map_err(|_| anyhow!("Failed to send execute JS with result command"))?;

        response_rx.await.context("Failed to receive JS with result response")?
    }

    /// Captures a screenshot of a tab.
    pub async fn screenshot(
        &self,
        tab_id: Uuid,
        options: ScreenshotOptions,
    ) -> Result<Screenshot> {
        if !self.is_running.load(Ordering::SeqCst) {
            return Err(anyhow!("Browser engine is not running"));
        }

        let (response_tx, response_rx) = oneshot::channel();

        self.command_tx
            .send(CefCommand::Screenshot {
                tab_id,
                options,
                response: response_tx,
            })
            .map_err(|_| anyhow!("Failed to send screenshot command"))?;

        response_rx.await.context("Failed to receive screenshot response")?
    }

    /// Waits for a tab to be ready for interaction.
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
