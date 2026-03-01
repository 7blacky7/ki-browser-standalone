//! Browser command handler for IPC processing
//!
//! This module provides a handler that processes IPC commands and forwards them
//! to the appropriate browser engine (CEF or Mock).

use std::sync::Arc;
use tokio::sync::RwLock;
use tracing::{debug, info, warn};
use uuid::Uuid;

use crate::api::ipc::{IpcCommand, IpcProcessor, IpcResponse};

#[cfg(feature = "cef-browser")]
use crate::browser::CefBrowserEngine;

use crate::browser::{BrowserEngine, MockBrowserEngine, ScreenshotFormat, ScreenshotOptions};


/// Browser engine wrapper that abstracts over different implementations
pub enum BrowserEngineWrapper {
    /// Mock browser for testing and fallback
    Mock(Arc<MockBrowserEngine>),
    /// CEF browser engine (when feature enabled)
    #[cfg(feature = "cef-browser")]
    Cef(Arc<CefBrowserEngine>),
}

impl BrowserEngineWrapper {
    /// Create a mock browser wrapper
    pub async fn mock() -> anyhow::Result<Self> {
        let config = crate::browser::BrowserConfig::default();
        let engine = MockBrowserEngine::new(config).await?;
        Ok(Self::Mock(Arc::new(engine)))
    }

    /// Create a CEF browser wrapper
    #[cfg(feature = "cef-browser")]
    pub fn cef(engine: CefBrowserEngine) -> Self {
        Self::Cef(Arc::new(engine))
    }

    /// Create a CEF browser wrapper from a shared Arc
    #[cfg(feature = "cef-browser")]
    pub fn cef_shared(engine: Arc<CefBrowserEngine>) -> Self {
        Self::Cef(engine)
    }

    /// Check if the browser is running
    pub async fn is_running(&self) -> bool {
        match self {
            Self::Mock(engine) => engine.is_running().await,
            #[cfg(feature = "cef-browser")]
            Self::Cef(engine) => engine.is_running().await,
        }
    }
}

/// Handles IPC commands by forwarding them to the browser engine
pub struct BrowserCommandHandler {
    /// The browser engine to use
    engine: Arc<RwLock<Option<BrowserEngineWrapper>>>,
}

impl BrowserCommandHandler {
    /// Create a new handler with no engine (will use mock responses)
    pub fn new() -> Self {
        Self {
            engine: Arc::new(RwLock::new(None)),
        }
    }

    /// Create a handler with a mock browser engine
    pub async fn with_mock() -> anyhow::Result<Self> {
        let wrapper = BrowserEngineWrapper::mock().await?;
        Ok(Self {
            engine: Arc::new(RwLock::new(Some(wrapper))),
        })
    }

    /// Create a handler with a CEF browser engine
    #[cfg(feature = "cef-browser")]
    pub fn with_cef(engine: CefBrowserEngine) -> Self {
        let wrapper = BrowserEngineWrapper::cef(engine);
        Self {
            engine: Arc::new(RwLock::new(Some(wrapper))),
        }
    }

    /// Create a handler with a shared CEF browser engine (for GUI mode where
    /// the engine is shared between API and GUI).
    #[cfg(feature = "cef-browser")]
    pub fn with_cef_shared(engine: Arc<CefBrowserEngine>) -> Self {
        let wrapper = BrowserEngineWrapper::cef_shared(engine);
        Self {
            engine: Arc::new(RwLock::new(Some(wrapper))),
        }
    }

    /// Set the browser engine
    pub async fn set_engine(&self, wrapper: BrowserEngineWrapper) {
        let mut guard = self.engine.write().await;
        *guard = Some(wrapper);
    }

    /// Process a single IPC command
    pub async fn handle_command(&self, command: IpcCommand) -> IpcResponse {
        let engine_guard = self.engine.read().await;

        match command {
            IpcCommand::CreateTab { url, active } => {
                self.handle_create_tab(&engine_guard, &url, active).await
            }
            IpcCommand::CloseTab { tab_id } => {
                self.handle_close_tab(&engine_guard, &tab_id).await
            }
            IpcCommand::Navigate { tab_id, url } => {
                self.handle_navigate(&engine_guard, &tab_id, &url).await
            }
            IpcCommand::ClickCoordinates { tab_id, x, y, button, modifiers: _ } => {
                self.handle_click(&engine_guard, &tab_id, x, y, &button).await
            }
            IpcCommand::Drag { tab_id, from_x, from_y, to_x, to_y, steps, duration_ms } => {
                self.handle_drag(&engine_guard, &tab_id, from_x, from_y, to_x, to_y, steps.unwrap_or(20), duration_ms.unwrap_or(300)).await
            }
            IpcCommand::ClickElement { tab_id, selector, button: _, modifiers: _, frame_id } => {
                self.handle_click_element(&engine_guard, &tab_id, &selector, frame_id.as_deref()).await
            }
            IpcCommand::TypeText { tab_id, text, selector, clear_first: _, frame_id } => {
                self.handle_type_text(&engine_guard, &tab_id, &text, selector.as_deref(), frame_id.as_deref()).await
            }
            IpcCommand::Scroll { tab_id, x, y, delta_x, delta_y, selector: _, behavior: _ } => {
                self.handle_scroll(&engine_guard, &tab_id, x, y, delta_x, delta_y).await
            }
            IpcCommand::CaptureScreenshot { tab_id, format, quality, full_page, selector, clip_x, clip_y, clip_width, clip_height, clip_scale } => {
                let clip = if let (Some(x), Some(y), Some(w), Some(h)) = (clip_x, clip_y, clip_width, clip_height) {
                    Some((x, y, w, h, clip_scale.unwrap_or(1.0)))
                } else {
                    None
                };
                self.handle_screenshot(&engine_guard, &tab_id, &format, quality, full_page, selector.as_deref(), clip).await
            }
            IpcCommand::EvaluateScript { tab_id, script, await_promise: _, frame_id } => {
                self.handle_evaluate(&engine_guard, &tab_id, &script, frame_id.as_deref()).await
            }
            IpcCommand::GetTabs => {
                self.handle_get_tabs(&engine_guard).await
            }
            IpcCommand::DomSnapshot { tab_id, max_nodes, include_text } => {
                self.handle_dom_snapshot(&engine_guard, &tab_id, max_nodes, include_text).await
            }
            IpcCommand::AnnotateElements { tab_id, types, selector, ocr, ocr_lang } => {
                self.handle_annotate(&engine_guard, &tab_id, types, selector, ocr, ocr_lang).await
            }
            IpcCommand::GetFrameTree { tab_id } => {
                self.handle_get_frame_tree(&engine_guard, &tab_id).await
            }
            IpcCommand::EvaluateInFrame { tab_id, frame_id, script, await_promise: _ } => {
                self.handle_evaluate_in_frame(&engine_guard, &tab_id, &frame_id, &script).await
            }
            IpcCommand::FindElement { tab_id, selector, timeout } => {
                self.handle_find_element(&engine_guard, &tab_id, &selector, timeout).await
            }
            IpcCommand::Shutdown => {
                info!("Shutdown command received");
                IpcResponse::success()
            }
            // Handle other commands with mock responses for now
            _ => {
                warn!("Unhandled IPC command: {:?}", command);
                IpcResponse::success()
            }
        }
    }

    async fn handle_create_tab(
        &self,
        engine: &Option<BrowserEngineWrapper>,
        url: &str,
        _active: bool,
    ) -> IpcResponse {
        match engine {
            Some(BrowserEngineWrapper::Mock(e)) => {
                match e.create_tab(url).await {
                    Ok(tab) => IpcResponse::success_with_tab(tab.id.to_string()),
                    Err(e) => IpcResponse::error(e.to_string()),
                }
            }
            #[cfg(feature = "cef-browser")]
            Some(BrowserEngineWrapper::Cef(e)) => {
                match e.create_tab(url).await {
                    Ok(tab) => {
                        let mut resp = IpcResponse::success_with_tab(tab.id.to_string());
                        // Include CEF browser_id for CDP target mapping
                        if let Some(bid) = e.get_browser_id(&tab.id) {
                            resp.data = Some(serde_json::json!({ "browser_id": bid }));
                        }
                        resp
                    }
                    Err(e) => IpcResponse::error(e.to_string()),
                }
            }
            None => {
                let tab_id = format!("mock_tab_{}", Uuid::new_v4());
                IpcResponse::success_with_tab(tab_id)
            }
        }
    }

    async fn handle_close_tab(
        &self,
        engine: &Option<BrowserEngineWrapper>,
        tab_id: &str,
    ) -> IpcResponse {
        let uuid = match Uuid::parse_str(tab_id) {
            Ok(u) => u,
            Err(_) => return IpcResponse::error("Invalid tab ID"),
        };

        match engine {
            Some(BrowserEngineWrapper::Mock(e)) => {
                match e.close_tab(uuid).await {
                    Ok(_) => IpcResponse::success(),
                    Err(e) => IpcResponse::error(e.to_string()),
                }
            }
            #[cfg(feature = "cef-browser")]
            Some(BrowserEngineWrapper::Cef(e)) => {
                match e.close_tab(uuid).await {
                    Ok(_) => IpcResponse::success(),
                    Err(e) => IpcResponse::error(e.to_string()),
                }
            }
            None => IpcResponse::success(),
        }
    }

    async fn handle_navigate(
        &self,
        engine: &Option<BrowserEngineWrapper>,
        tab_id: &str,
        url: &str,
    ) -> IpcResponse {
        let uuid = match Uuid::parse_str(tab_id) {
            Ok(u) => u,
            Err(_) => return IpcResponse::error("Invalid tab ID"),
        };

        match engine {
            #[cfg(feature = "cef-browser")]
            Some(BrowserEngineWrapper::Cef(e)) => {
                match e.navigate(uuid, url).await {
                    Ok(_) => IpcResponse::success(),
                    Err(e) => IpcResponse::error(e.to_string()),
                }
            }
            _ => {
                debug!("Navigate (mock): {} -> {}", tab_id, url);
                IpcResponse::success()
            }
        }
    }

    async fn handle_click(
        &self,
        engine: &Option<BrowserEngineWrapper>,
        tab_id: &str,
        x: i32,
        y: i32,
        button: &str,
    ) -> IpcResponse {
        let uuid = match Uuid::parse_str(tab_id) {
            Ok(u) => u,
            Err(_) => return IpcResponse::error("Invalid tab ID"),
        };

        let _button_code = match button {
            "left" => 0,
            "middle" => 1,
            "right" => 2,
            _ => 0,
        };

        match engine {
            #[cfg(feature = "cef-browser")]
            Some(BrowserEngineWrapper::Cef(e)) => {
                match e.click(uuid, x, y, _button_code).await {
                    Ok(_) => IpcResponse::success(),
                    Err(e) => IpcResponse::error(e.to_string()),
                }
            }
            _ => {
                debug!("Click (mock): {} at ({}, {})", tab_id, x, y);
                IpcResponse::success()
            }
        }
    }

    async fn handle_drag(
        &self,
        engine: &Option<BrowserEngineWrapper>,
        tab_id: &str,
        from_x: i32,
        from_y: i32,
        to_x: i32,
        to_y: i32,
        steps: u32,
        duration_ms: u64,
    ) -> IpcResponse {
        let uuid = match Uuid::parse_str(tab_id) {
            Ok(u) => u,
            Err(_) => return IpcResponse::error("Invalid tab ID"),
        };

        match engine {
            #[cfg(feature = "cef-browser")]
            Some(BrowserEngineWrapper::Cef(e)) => {
                match e.drag(uuid, from_x, from_y, to_x, to_y, steps, duration_ms).await {
                    Ok(_) => IpcResponse::success(),
                    Err(e) => IpcResponse::error(e.to_string()),
                }
            }
            _ => {
                debug!("Drag (mock): {} from ({},{}) to ({},{})", tab_id, from_x, from_y, to_x, to_y);
                IpcResponse::success()
            }
        }
    }

    async fn handle_click_element(
        &self,
        engine: &Option<BrowserEngineWrapper>,
        tab_id: &str,
        selector: &str,
        frame_id: Option<&str>,
    ) -> IpcResponse {
        let _uuid = match Uuid::parse_str(tab_id) {
            Ok(u) => u,
            Err(_) => return IpcResponse::error("Invalid tab ID"),
        };

        match engine {
            _ => {
                debug!("ClickElement (mock): {} -> {} (frame: {:?})", tab_id, selector, frame_id);
                IpcResponse::success()
            }
        }
    }

    async fn handle_type_text(
        &self,
        engine: &Option<BrowserEngineWrapper>,
        tab_id: &str,
        text: &str,
        _selector: Option<&str>,
        frame_id: Option<&str>,
    ) -> IpcResponse {
        let uuid = match Uuid::parse_str(tab_id) {
            Ok(u) => u,
            Err(_) => return IpcResponse::error("Invalid tab ID"),
        };

        match engine {
            #[cfg(feature = "cef-browser")]
            Some(BrowserEngineWrapper::Cef(e)) => {
                if frame_id.is_some() {
                    warn!("Frame-specific type_text not fully implemented for CEF");
                }
                match e.type_text(uuid, text).await {
                    Ok(_) => IpcResponse::success(),
                    Err(e) => IpcResponse::error(e.to_string()),
                }
            }
            _ => {
                debug!("Type (mock): {} chars to {} (frame: {:?})", text.len(), tab_id, frame_id);
                IpcResponse::success()
            }
        }
    }

    async fn handle_scroll(
        &self,
        engine: &Option<BrowserEngineWrapper>,
        tab_id: &str,
        _x: Option<i32>,
        _y: Option<i32>,
        delta_x: Option<i32>,
        delta_y: Option<i32>,
    ) -> IpcResponse {
        let uuid = match Uuid::parse_str(tab_id) {
            Ok(u) => u,
            Err(_) => return IpcResponse::error("Invalid tab ID"),
        };

        let dx = delta_x.unwrap_or(0);
        let dy = delta_y.unwrap_or(100);

        match engine {
            #[cfg(feature = "cef-browser")]
            Some(BrowserEngineWrapper::Cef(e)) => {
                let scroll_x = _x.unwrap_or(0);
                let scroll_y = _y.unwrap_or(0);
                match e.scroll(uuid, scroll_x, scroll_y, dx, dy).await {
                    Ok(_) => IpcResponse::success(),
                    Err(e) => IpcResponse::error(e.to_string()),
                }
            }
            _ => {
                debug!("Scroll (mock): {} by ({}, {})", tab_id, dx, dy);
                IpcResponse::success()
            }
        }
    }

    async fn handle_screenshot(
        &self,
        engine: &Option<BrowserEngineWrapper>,
        tab_id: &str,
        format: &str,
        quality: Option<u8>,
        full_page: bool,
        _selector: Option<&str>,
        _clip: Option<(f64, f64, f64, f64, f64)>,
    ) -> IpcResponse {
        let uuid = match Uuid::parse_str(tab_id) {
            Ok(u) => u,
            Err(_) => return IpcResponse::error("Invalid tab ID"),
        };

        let screenshot_format = match format.to_lowercase().as_str() {
            "jpeg" | "jpg" => ScreenshotFormat::Jpeg,
            "webp" => ScreenshotFormat::WebP,
            _ => ScreenshotFormat::Png,
        };

        let options = ScreenshotOptions {
            format: screenshot_format,
            quality: quality.unwrap_or(90),
            full_page,
            ..Default::default()
        };

        match engine {
            #[cfg(feature = "cef-browser")]
            Some(BrowserEngineWrapper::Cef(e)) => {
                match e.screenshot(uuid, options).await {
                    Ok(screenshot) => {
                        IpcResponse::success_with_data(serde_json::json!({
                            "screenshot": screenshot.data,
                            "width": screenshot.width,
                            "height": screenshot.height,
                            "format": format
                        }))
                    }
                    Err(e) => IpcResponse::error(e.to_string()),
                }
            }
            _ => {
                debug!("Screenshot (mock): {}", tab_id);
                IpcResponse::success_with_data(serde_json::json!({
                    "screenshot": "",
                    "width": 1920,
                    "height": 1080,
                    "format": format
                }))
            }
        }
    }

    async fn handle_evaluate(
        &self,
        engine: &Option<BrowserEngineWrapper>,
        tab_id: &str,
        script: &str,
        frame_id: Option<&str>,
    ) -> IpcResponse {
        let uuid = match Uuid::parse_str(tab_id) {
            Ok(u) => u,
            Err(_) => return IpcResponse::error("Invalid tab ID"),
        };

        match engine {
            #[cfg(feature = "cef-browser")]
            Some(BrowserEngineWrapper::Cef(e)) => {
                if frame_id.is_some() {
                    warn!("Frame-specific evaluate not implemented for CEF, using main frame");
                }
                // CEF JS execution with return values via console.log interception
                match e.execute_js_with_result(uuid, script).await {
                    Ok(Some(result)) => {
                        // Parse the JSON string back to a Value
                        let value: serde_json::Value = serde_json::from_str(&result)
                            .unwrap_or(serde_json::Value::String(result));
                        IpcResponse::success_with_data(serde_json::json!({
                            "result": value
                        }))
                    }
                    Ok(None) => {
                        IpcResponse::success_with_data(serde_json::json!({
                            "result": null
                        }))
                    }
                    Err(e) => IpcResponse::error(e.to_string()),
                }
            }
            _ => {
                debug!("Evaluate (mock): {} chars to {} (frame: {:?})", script.len(), tab_id, frame_id);
                IpcResponse::success_with_data(serde_json::json!({
                    "result": null
                }))
            }
        }
    }

    async fn handle_annotate(
        &self,
        engine: &Option<BrowserEngineWrapper>,
        tab_id: &str,
        types: Vec<String>,
        selector: Option<String>,
        ocr: bool,
        ocr_lang: String,
    ) -> IpcResponse {
        let uuid = match Uuid::parse_str(tab_id) {
            Ok(u) => u,
            Err(_) => return IpcResponse::error("Invalid tab ID"),
        };

        let js = crate::browser::annotate::generate_find_elements_js(&types, selector.as_deref());

        match engine {
            #[cfg(feature = "cef-browser")]
            Some(BrowserEngineWrapper::Cef(e)) => {
                // Step 1: Evaluate JS via MessageRouter to get actual return value
                let elements_json_str = match e.execute_js_with_result(uuid, &js).await {
                    Ok(val) => val,
                    Err(e) => return IpcResponse::error(format!("JS evaluation failed: {}", e)),
                };

                // CEF execute_js returns Option<String>, parse to Value first
                let elements_value: serde_json::Value = match elements_json_str {
                    Some(s) => serde_json::from_str(&s).unwrap_or(serde_json::Value::Null),
                    None => serde_json::Value::Null,
                };

                let elements: Vec<crate::browser::annotate::AnnotatedElement> =
                    match serde_json::from_value(elements_value) {
                        Ok(elems) => elems,
                        Err(e) => {
                            return IpcResponse::error(format!("Failed to parse elements: {}", e))
                        }
                    };

                // Step 2: Screenshot (CEF returns struct, extract PNG bytes)
                let options = ScreenshotOptions {
                    format: ScreenshotFormat::Png,
                    quality: 90,
                    ..Default::default()
                };
                let screenshot = match e.screenshot(uuid, options).await {
                    Ok(s) => s,
                    Err(e) => return IpcResponse::error(format!("Screenshot failed: {}", e)),
                };

                // Decode base64 screenshot data to raw bytes
                let png_bytes = match base64::Engine::decode(
                    &base64::engine::general_purpose::STANDARD,
                    &screenshot.data,
                ) {
                    Ok(data) => data,
                    Err(e) => {
                        return IpcResponse::error(format!("Screenshot decode failed: {}", e))
                    }
                };

                // Step 3: Annotate
                let annotated =
                    match crate::browser::annotate::annotate_screenshot(&png_bytes, &elements) {
                        Ok(data) => data,
                        Err(e) => {
                            return IpcResponse::error(format!("Annotation failed: {}", e))
                        }
                    };

                // Step 4: Optional OCR
                let ocr_text = if ocr {
                    match crate::browser::annotate::ocr_screenshot(&png_bytes, &ocr_lang) {
                        Ok(result) => Some(result.text),
                        Err(e) => {
                            warn!("OCR failed: {}", e);
                            None
                        }
                    }
                } else {
                    None
                };

                let b64 = base64::Engine::encode(
                    &base64::engine::general_purpose::STANDARD,
                    &annotated,
                );

                IpcResponse::success_with_data(serde_json::json!({
                    "screenshot": b64,
                    "elements": elements,
                    "ocr_text": ocr_text,
                }))
            }
            _ => {
                debug!("Annotate (mock): {}", tab_id);
                IpcResponse::success_with_data(serde_json::json!({
                    "screenshot": "",
                    "elements": [],
                    "ocr_text": null,
                }))
            }
        }
    }

    async fn handle_find_element(
        &self,
        engine: &Option<BrowserEngineWrapper>,
        tab_id: &str,
        selector: &str,
        _timeout: Option<u64>,
    ) -> IpcResponse {
        let uuid = match Uuid::parse_str(tab_id) {
            Ok(u) => u,
            Err(_) => return IpcResponse::error("Invalid tab ID"),
        };

        // Escape selector for safe embedding in a JS string literal.
        let escaped_selector = selector.replace('\\', "\\\\").replace('\'', "\\'");
        let js = format!(
            r#"(function(){{var el=document.querySelector('{}');if(!el)return null;var r=el.getBoundingClientRect();var a={{}};for(var i=0;i<el.attributes.length;i++){{a[el.attributes[i].name]=el.attributes[i].value}}return {{found:true,tagName:el.tagName.toLowerCase(),textContent:(el.textContent||'').trim().substring(0,500),attributes:a,boundingBox:{{x:r.x,y:r.y,width:r.width,height:r.height}},isVisible:r.width>0&&r.height>0&&getComputedStyle(el).display!=='none'}}}})()"#,
            escaped_selector
        );

        match engine {
            #[cfg(feature = "cef-browser")]
            Some(BrowserEngineWrapper::Cef(e)) => {
                match e.execute_js_with_result(uuid, &js).await {
                    Ok(Some(json_str)) => {
                        match serde_json::from_str::<serde_json::Value>(&json_str) {
                            Ok(data) if !data.is_null() => IpcResponse::success_with_data(data),
                            _ => IpcResponse::success_with_data(serde_json::json!({"found": false})),
                        }
                    }
                    Ok(None) => IpcResponse::success_with_data(serde_json::json!({"found": false})),
                    Err(e) => IpcResponse::error(e.to_string()),
                }
            }
            _ => IpcResponse::error("No browser engine available"),
        }
    }

    async fn handle_dom_snapshot(
        &self,
        engine: &Option<BrowserEngineWrapper>,
        tab_id: &str,
        max_nodes: u32,
        include_text: bool,
    ) -> IpcResponse {
        let uuid = match Uuid::parse_str(tab_id) {
            Ok(u) => u,
            Err(_) => return IpcResponse::error("Invalid tab ID"),
        };

        let config = crate::browser::dom_snapshot::SnapshotConfig {
            max_nodes,
            include_text,
        };
        let script = crate::browser::dom_snapshot::build_snapshot_script(&config);

        match engine {
            #[cfg(feature = "cef-browser")]
            Some(BrowserEngineWrapper::Cef(e)) => {
                match e.execute_js_with_result(uuid, &script).await {
                    Ok(Some(json_str)) => {
                        match crate::browser::dom_snapshot::parse_snapshot_json(&json_str) {
                            Ok(snapshot) => {
                                match serde_json::to_value(&snapshot) {
                                    Ok(val) => IpcResponse::success_with_data(val),
                                    Err(e) => IpcResponse::error(format!("Serialization failed: {}", e)),
                                }
                            }
                            Err(e) => IpcResponse::error(format!("Snapshot parse failed: {}", e)),
                        }
                    }
                    Ok(None) => IpcResponse::error("DOM snapshot returned no data"),
                    Err(e) => IpcResponse::error(format!("JS evaluation failed: {}", e)),
                }
            }
            _ => {
                debug!("DomSnapshot (mock): {}", tab_id);
                IpcResponse::success_with_data(serde_json::json!({
                    "nodes": [],
                    "viewport": { "width": 1920, "height": 1080, "scroll_x": 0.0, "scroll_y": 0.0 },
                    "device_pixel_ratio": 1.0,
                    "url": "about:blank",
                    "timestamp": chrono::Utc::now().to_rfc3339()
                }))
            }
        }
    }

    async fn handle_get_tabs(&self, engine: &Option<BrowserEngineWrapper>) -> IpcResponse {
        match engine {
            Some(BrowserEngineWrapper::Mock(e)) => {
                match e.get_tabs().await {
                    Ok(tabs) => {
                        let tabs_data: Vec<_> = tabs.iter().map(|t| {
                            serde_json::json!({
                                "id": t.id.to_string(),
                                "url": t.url,
                                "title": t.title,
                            })
                        }).collect();
                        IpcResponse::success_with_data(serde_json::json!({ "tabs": tabs_data }))
                    }
                    Err(e) => IpcResponse::error(e.to_string()),
                }
            }
            #[cfg(feature = "cef-browser")]
            Some(BrowserEngineWrapper::Cef(e)) => {
                match e.get_tabs().await {
                    Ok(tabs) => {
                        let tabs_data: Vec<_> = tabs.iter().map(|t| {
                            serde_json::json!({
                                "id": t.id.to_string(),
                                "url": t.url,
                                "title": t.title,
                            })
                        }).collect();
                        IpcResponse::success_with_data(serde_json::json!({ "tabs": tabs_data }))
                    }
                    Err(e) => IpcResponse::error(e.to_string()),
                }
            }
            None => {
                IpcResponse::success_with_data(serde_json::json!({ "tabs": [] }))
            }
        }
    }

    async fn handle_get_frame_tree(
        &self,
        engine: &Option<BrowserEngineWrapper>,
        tab_id: &str,
    ) -> IpcResponse {
        let _uuid = match Uuid::parse_str(tab_id) {
            Ok(u) => u,
            Err(_) => return IpcResponse::error("Invalid tab ID"),
        };

        match engine {
            _ => {
                debug!("GetFrameTree (mock): {}", tab_id);
                // Mock: return single main frame
                IpcResponse::success_with_data(serde_json::json!({
                    "frames": [{
                        "frame_id": "main",
                        "parent_frame_id": null,
                        "url": "about:blank",
                        "name": "",
                        "security_origin": "null",
                    }]
                }))
            }
        }
    }

    async fn handle_evaluate_in_frame(
        &self,
        engine: &Option<BrowserEngineWrapper>,
        tab_id: &str,
        frame_id: &str,
        script: &str,
    ) -> IpcResponse {
        let _uuid = match Uuid::parse_str(tab_id) {
            Ok(u) => u,
            Err(_) => return IpcResponse::error("Invalid tab ID"),
        };

        match engine {
            _ => {
                debug!("EvaluateInFrame (mock): {} in frame {} ({} chars)", tab_id, frame_id, script.len());
                IpcResponse::success_with_data(serde_json::json!({
                    "result": null
                }))
            }
        }
    }

    /// Run the command processing loop
    pub async fn run(&self, processor: &mut IpcProcessor) {
        processor.process(|cmd| self.handle_command(cmd)).await;
    }
}

impl Default for BrowserCommandHandler {
    fn default() -> Self {
        Self::new()
    }
}

/// Detect image dimensions from raw PNG/JPEG/WebP bytes
#[allow(dead_code)]
fn detect_image_dimensions(data: &[u8]) -> (u32, u32) {
    // PNG: width/height at bytes 16-23 (big-endian u32)
    if data.len() >= 24 && &data[0..8] == b"\x89PNG\r\n\x1a\n" {
        let width = u32::from_be_bytes([data[16], data[17], data[18], data[19]]);
        let height = u32::from_be_bytes([data[20], data[21], data[22], data[23]]);
        return (width, height);
    }
    // JPEG: scan for SOF0 marker (0xFF 0xC0) - height then width as big-endian u16
    if data.len() >= 2 && data[0] == 0xFF && data[1] == 0xD8 {
        let mut i = 2;
        while i + 9 < data.len() {
            if data[i] == 0xFF && (data[i + 1] == 0xC0 || data[i + 1] == 0xC2) {
                let height = u16::from_be_bytes([data[i + 5], data[i + 6]]) as u32;
                let width = u16::from_be_bytes([data[i + 7], data[i + 8]]) as u32;
                return (width, height);
            }
            if data[i] == 0xFF && data[i + 1] != 0x00 {
                let seg_len = u16::from_be_bytes([data[i + 2], data[i + 3]]) as usize;
                i += 2 + seg_len;
            } else {
                i += 1;
            }
        }
    }
    // WebP VP8: width/height in VP8 bitstream
    if data.len() >= 30 && &data[0..4] == b"RIFF" && &data[8..12] == b"WEBP" {
        if &data[12..16] == b"VP8 " && data.len() >= 30 {
            let width = (u16::from_le_bytes([data[26], data[27]]) & 0x3FFF) as u32;
            let height = (u16::from_le_bytes([data[28], data[29]]) & 0x3FFF) as u32;
            return (width, height);
        }
        if &data[12..16] == b"VP8L" && data.len() >= 25 {
            let bits = u32::from_le_bytes([data[21], data[22], data[23], data[24]]);
            let width = (bits & 0x3FFF) + 1;
            let height = ((bits >> 14) & 0x3FFF) + 1;
            return (width, height);
        }
    }
    (0, 0)
}
