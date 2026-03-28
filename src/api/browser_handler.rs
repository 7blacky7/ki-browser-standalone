//! Browser command handler for IPC processing
//!
//! This module provides a handler that processes IPC commands and forwards them
//! to the appropriate browser engine. Returns errors when no engine is available.

use std::sync::Arc;
use tokio::sync::RwLock;
use tracing::{debug, info, warn};
use uuid::Uuid;

use crate::api::ipc::{IpcCommand, IpcProcessor, IpcResponse};

#[cfg(feature = "cef-browser")]
use crate::browser::CefBrowserEngine;

use crate::browser::{BrowserEngine, MockBrowserEngine, ScreenshotFormat, ScreenshotOptions};

/// Parameters for drag operations between two screen coordinates
struct DragParams {
    from_x: i32,
    from_y: i32,
    to_x: i32,
    to_y: i32,
    steps: u32,
    duration_ms: u64,
}

/// Parameters for screenshot capture configuration
struct ScreenshotParams<'a> {
    format: &'a str,
    quality: Option<u8>,
    full_page: bool,
    selector: Option<&'a str>,
    clip: Option<(f64, f64, f64, f64, f64)>,
}

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
    /// CDP client for privileged operations (bypasses CSP/Trusted Types)
    cdp_client: Option<Arc<crate::api::cdp_client::CdpClient>>,
    /// Complete stealth override script for CDP injection (pre-document)
    stealth_init_script: Option<String>,
}

impl BrowserCommandHandler {
    /// Create a new handler with no engine (all commands will return errors)
    pub fn new() -> Self {
        Self {
            engine: Arc::new(RwLock::new(None)),
            cdp_client: None,
            stealth_init_script: None,
        }
    }

    /// Set the CDP client for privileged JS evaluation.
    pub fn set_cdp_client(&mut self, client: Arc<crate::api::cdp_client::CdpClient>) {
        self.cdp_client = Some(client);
    }

    /// Set the complete stealth init script for CDP pre-document injection.
    pub fn set_stealth_init_script(&mut self, script: String) {
        self.stealth_init_script = Some(script);
    }

    /// Create a handler with a mock browser engine
    pub async fn with_mock() -> anyhow::Result<Self> {
        let wrapper = BrowserEngineWrapper::mock().await?;
        Ok(Self {
            engine: Arc::new(RwLock::new(Some(wrapper))),
            cdp_client: None,
            stealth_init_script: None,
        })
    }

    /// Create a handler with a CEF browser engine
    #[cfg(feature = "cef-browser")]
    pub fn with_cef(engine: CefBrowserEngine) -> Self {
        let wrapper = BrowserEngineWrapper::cef(engine);
        Self {
            engine: Arc::new(RwLock::new(Some(wrapper))),
            cdp_client: None,
            stealth_init_script: None,
        }
    }

    /// Create a handler with a shared CEF browser engine (for GUI mode where
    /// the engine is shared between API and GUI).
    #[cfg(feature = "cef-browser")]
    pub fn with_cef_shared(engine: Arc<CefBrowserEngine>) -> Self {
        let wrapper = BrowserEngineWrapper::cef_shared(engine);
        Self {
            engine: Arc::new(RwLock::new(Some(wrapper))),
            cdp_client: None,
            stealth_init_script: None,
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
                self.handle_drag(&engine_guard, &tab_id, DragParams {
                    from_x, from_y, to_x, to_y,
                    steps: steps.unwrap_or(20),
                    duration_ms: duration_ms.unwrap_or(300),
                }).await
            }
            IpcCommand::ClickElement { tab_id, selector, button: _, modifiers: _, frame_id } => {
                self.handle_click_element(&engine_guard, &tab_id, &selector, frame_id.as_deref()).await
            }
            IpcCommand::TypeText { tab_id, text, selector, clear_first: _, frame_id } => {
                self.handle_type_text(&engine_guard, &tab_id, &text, selector.as_deref(), frame_id.as_deref()).await
            }
            IpcCommand::Scroll { tab_id, x, y, delta_x, delta_y, selector, behavior, frame_id } => {
                self.handle_scroll(&engine_guard, &tab_id, x, y, delta_x, delta_y, selector, behavior, frame_id.as_deref()).await
            }
            IpcCommand::CaptureScreenshot { tab_id, format, quality, full_page, selector, clip_x, clip_y, clip_width, clip_height, clip_scale } => {
                let clip = if let (Some(x), Some(y), Some(w), Some(h)) = (clip_x, clip_y, clip_width, clip_height) {
                    Some((x, y, w, h, clip_scale.unwrap_or(1.0)))
                } else {
                    None
                };
                self.handle_screenshot(&engine_guard, &tab_id, ScreenshotParams {
                    format: &format, quality, full_page,
                    selector: selector.as_deref(), clip,
                }).await
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
            IpcCommand::VisionLabels { tab_id } => {
                // Delegate to annotate with default element types (all interactive)
                let types = vec![
                    "button".to_string(), "input".to_string(), "link".to_string(),
                    "select".to_string(), "textarea".to_string(),
                ];
                self.handle_annotate(&engine_guard, &tab_id, types, None, false, String::new()).await
            }
            IpcCommand::Shutdown => {
                info!("Shutdown command received");
                IpcResponse::success()
            }
            _ => {
                warn!("Unhandled IPC command: {:?}", command);
                IpcResponse::error(format!("Not implemented: {:?}", command))
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

                        // Phase 3: Inject stealth scripts via CDP before any page JS runs.
                        // This uses Page.addScriptToEvaluateOnNewDocument which persists
                        // across navigations and bypasses CSP/Trusted Types.
                        if let Some(ref cdp) = self.cdp_client {
                            let tab_url = tab.url.clone();
                            let cdp = cdp.clone();
                            let stealth_script = self.stealth_init_script.clone();
                            tokio::spawn(async move {
                                // Wait for CDP target to become available
                                tokio::time::sleep(tokio::time::Duration::from_millis(500)).await;
                                if let Ok(ws_url) = cdp.find_target_ws_url(&tab_url).await {
                                    // Inject complete stealth script via CDP (runs before any page JS)
                                    let stealth_js = stealth_script.unwrap_or_else(|| r#"
                                        Object.defineProperty(navigator, 'webdriver', {get: () => undefined});
                                        Object.defineProperty(Navigator.prototype, 'webdriver', {get: () => undefined});
                                    "#.to_string());
                                    match cdp.add_init_script(&ws_url, &stealth_js).await {
                                        Ok(_) => debug!("CDP stealth init-script injected for new tab"),
                                        Err(e) => debug!("CDP stealth injection failed: {}", e),
                                    }
                                }
                            });
                        }

                        resp
                    }
                    Err(e) => IpcResponse::error(e.to_string()),
                }
            }
            None => {
                IpcResponse::error("No browser engine available for CreateTab")
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
            None => IpcResponse::error("No browser engine available for CloseTab"),
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

        // Validate URL scheme before navigating
        if !url.starts_with("http://") && !url.starts_with("https://") && !url.starts_with("about:") && !url.starts_with("data:") {
            return IpcResponse::error(format!("Invalid URL scheme: URL must start with http://, https://, about:, or data: — got: {}", url));
        }

        // Before navigating: inject stealth init-script via CDP so it runs
        // before any page JS on the new document.
        if let Some(ref cdp) = self.cdp_client {
            if let Some(tab_url) = match engine {
                #[cfg(feature = "cef-browser")]
                Some(BrowserEngineWrapper::Cef(e)) => {
                    e.get_tabs_sync().into_iter()
                        .find(|t| t.id == uuid)
                        .map(|t| t.url.clone())
                }
                _ => None,
            } {
                if let Ok(ws_url) = cdp.find_target_ws_url(&tab_url).await {
                    // Inject complete stealth script via CDP (includes WebGL, navigator, etc.)
                    let stealth_js = self.stealth_init_script.clone().unwrap_or_else(|| r#"
                        Object.defineProperty(navigator, 'webdriver', {get: () => undefined});
                        Object.defineProperty(Navigator.prototype, 'webdriver', {get: () => undefined});
                    "#.to_string());
                    let _ = cdp.add_init_script(&ws_url, &stealth_js).await;
                    debug!("CDP stealth init-script set before navigation to {}", url);
                }
            }
        }

        match engine {
            #[cfg(feature = "cef-browser")]
            Some(BrowserEngineWrapper::Cef(e)) => {
                match e.navigate(uuid, url).await {
                    Ok(_) => IpcResponse::success(),
                    Err(e) => IpcResponse::error(e.to_string()),
                }
            }
            _ => {
                IpcResponse::error("No browser engine available for Navigate")
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
                IpcResponse::error("No browser engine available for Click")
            }
        }
    }

    async fn handle_drag(
        &self,
        engine: &Option<BrowserEngineWrapper>,
        tab_id: &str,
        params: DragParams,
    ) -> IpcResponse {
        let DragParams { from_x, from_y, to_x, to_y, steps, duration_ms } = params;
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
                IpcResponse::error("No browser engine available for Drag")
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

        // Strategy 1: If frame_id is set, resolve element position via CDP frame context
        if let Some(fid) = frame_id {
            if let Some(ref cdp) = self.cdp_client {
                let tab_url = match engine {
                    #[cfg(feature = "cef-browser")]
                    Some(BrowserEngineWrapper::Cef(e)) => {
                        e.get_tabs_sync().into_iter()
                            .find(|t| t.id == _uuid)
                            .map(|t| t.url.clone())
                    }
                    _ => None,
                };

                if let Some(url) = tab_url {
                    if let Ok(ws_url) = cdp.find_target_ws_url(&url).await {
                        match crate::api::cdp_frames::get_element_center_in_frame(cdp, &ws_url, fid, selector).await {
                            Ok((cx, cy)) => {
                                match engine {
                                    #[cfg(feature = "cef-browser")]
                                    Some(BrowserEngineWrapper::Cef(e)) => {
                                        match e.click(_uuid, cx, cy, 0).await {
                                            Ok(_) => return IpcResponse::success(),
                                            Err(e) => return IpcResponse::error(e.to_string()),
                                        }
                                    }
                                    _ => {}
                                }
                            }
                            Err(e) => {
                                debug!("CDP frame click failed ({}), falling back", e);
                            }
                        }
                    }
                }
            }
        }

        // Strategy 2: Main frame click via CEF JS
        match engine {
            #[cfg(feature = "cef-browser")]
            Some(BrowserEngineWrapper::Cef(e)) => {
                let escaped = selector.replace('\\', "\\\\").replace('\'', "\\'");
                let js = format!(
                    r#"(function(){{var el=document.querySelector('{}');if(!el)return null;var r=el.getBoundingClientRect();return {{x:r.x+r.width/2,y:r.y+r.height/2}}}})()"#,
                    escaped
                );
                match e.execute_js_with_result(_uuid, &js).await {
                    Ok(Some(json_str)) => {
                        match serde_json::from_str::<serde_json::Value>(&json_str) {
                            Ok(coords) if !coords.is_null() => {
                                let cx = coords["x"].as_f64().unwrap_or(0.0) as i32;
                                let cy = coords["y"].as_f64().unwrap_or(0.0) as i32;
                                match e.click(_uuid, cx, cy, 0).await {
                                    Ok(_) => IpcResponse::success(),
                                    Err(e) => IpcResponse::error(e.to_string()),
                                }
                            }
                            _ => IpcResponse::error(format!("Element not found: {}", selector)),
                        }
                    }
                    Ok(None) => IpcResponse::error(format!("Element not found: {}", selector)),
                    Err(e) => IpcResponse::error(e.to_string()),
                }
            }
            _ => {
                IpcResponse::error("No browser engine available for ClickElement")
            }
        }
    }

    async fn handle_type_text(
        &self,
        engine: &Option<BrowserEngineWrapper>,
        tab_id: &str,
        text: &str,
        selector: Option<&str>,
        frame_id: Option<&str>,
    ) -> IpcResponse {
        let uuid = match Uuid::parse_str(tab_id) {
            Ok(u) => u,
            Err(_) => return IpcResponse::error("Invalid tab ID"),
        };

        // Strategy 1: Try CDP (with frame isolation if frame_id is set)
        if let Some(ref cdp) = self.cdp_client {
            let tab_url = match engine {
                #[cfg(feature = "cef-browser")]
                Some(BrowserEngineWrapper::Cef(e)) => {
                    e.get_tabs_sync().into_iter()
                        .find(|t| t.id == uuid)
                        .map(|t| t.url.clone())
                }
                _ => None,
            };

            if let Some(url) = tab_url {
                if let Ok(ws_url) = cdp.find_target_ws_url(&url).await {
                    if let Some(fid) = frame_id {
                        // Frame-specific: focus element in frame context, then insertText
                        if let Some(sel) = selector {
                            let escaped = sel.replace('\'', "\\'");
                            let focus_js = format!(
                                r#"(()=>{{var el=document.querySelector('{}');if(!el)return 'not_found';el.focus();return 'focused'}})()"#,
                                escaped
                            );
                            match crate::api::cdp_frames::evaluate_in_frame(cdp, &ws_url, fid, &focus_js, false).await {
                                Ok(result) if !result.contains("not_found") => {
                                    tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;
                                    match cdp.insert_text(&ws_url, text).await {
                                        Ok(_) => {
                                            debug!("CDP frame type succeeded for frame '{}' selector '{}'", fid, sel);
                                            return IpcResponse::success();
                                        }
                                        Err(e) => {
                                            debug!("CDP frame insert_text failed ({}), falling back", e);
                                        }
                                    }
                                }
                                Ok(_) => {
                                    debug!("Element '{}' not found in frame '{}'", sel, fid);
                                }
                                Err(e) => {
                                    debug!("CDP frame focus failed ({}), falling back", e);
                                }
                            }
                        }
                    } else {
                        // No frame_id — original main-context logic
                        if let Some(sel) = selector {
                            match cdp.focus_and_type(&ws_url, sel, text).await {
                                Ok(_) => {
                                    debug!("CDP focus_and_type succeeded for selector '{}'", sel);
                                    return IpcResponse::success();
                                }
                                Err(e) => {
                                    debug!("CDP focus_and_type failed ({}), trying insertText", e);
                                }
                            }
                        }

                        // No selector or selector-focus failed: try plain insertText
                        match cdp.insert_text(&ws_url, text).await {
                            Ok(_) => {
                                debug!("CDP insert_text succeeded");
                                return IpcResponse::success();
                            }
                            Err(e) => {
                                debug!("CDP insert_text failed ({}), falling back to CEF", e);
                            }
                        }
                    }
                }
            }
        }

        // Strategy 2: Fallback to CEF key events
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
                IpcResponse::error("No browser engine available for TypeText")
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
        selector: Option<String>,
        behavior: Option<String>,
        frame_id: Option<&str>,
    ) -> IpcResponse {
        let uuid = match Uuid::parse_str(tab_id) {
            Ok(u) => u,
            Err(_) => return IpcResponse::error("Invalid tab ID"),
        };

        let dx = delta_x.unwrap_or(0);
        let dy = delta_y.unwrap_or(100);
        let behavior_str = behavior.unwrap_or_else(|| "instant".to_string());

        // Use JS-based scrolling for reliability (CEF MouseWheel at 0,0 is unreliable)
        let js = if let Some(ref sel) = selector {
            format!(
                "(() => {{ var el = document.querySelector('{}'); if (!el) return JSON.stringify({{error: 'Element not found'}}); el.scrollIntoView({{behavior: '{}', block: 'start'}}); return JSON.stringify({{scrollY: window.scrollY}}); }})()",
                sel.replace('\'', "\\'"), behavior_str
            )
        } else {
            format!(
                "(() => {{ window.scrollBy({{left: {}, top: {}, behavior: '{}'}}); return JSON.stringify({{scrollY: window.scrollY}}); }})()",
                dx, dy, behavior_str
            )
        };

        // If frame_id is set, try CDP frame-isolated evaluation first
        if let Some(fid) = frame_id {
            if let Some(ref cdp) = self.cdp_client {
                let tab_url = match engine {
                    #[cfg(feature = "cef-browser")]
                    Some(BrowserEngineWrapper::Cef(e)) => {
                        e.get_tabs_sync().into_iter()
                            .find(|t| t.id == uuid)
                            .map(|t| t.url.clone())
                    }
                    _ => None,
                };

                if let Some(url) = tab_url {
                    match cdp.find_target_ws_url(&url).await {
                        Ok(ws_url) => {
                            match crate::api::cdp_frames::evaluate_in_frame(cdp, &ws_url, fid, &js, true).await {
                                Ok(result) => {
                                    debug!("CDP frame scroll succeeded for tab {} frame '{}'", tab_id, fid);
                                    let value: serde_json::Value = serde_json::from_str(&result)
                                        .unwrap_or(serde_json::Value::Null);
                                    if let Some(err) = value.get("error").and_then(|e| e.as_str()) {
                                        return IpcResponse::error(format!("Scroll failed: {}", err));
                                    }
                                    return IpcResponse::success_with_data(value);
                                }
                                Err(e) => {
                                    debug!("CDP frame scroll failed ({}), falling back to CEF", e);
                                }
                            }
                        }
                        Err(e) => {
                            debug!("CDP target discovery failed for scroll ({}), falling back to CEF", e);
                        }
                    }
                }
            }
            // No CDP client available for frame-isolated scroll
            warn!("Frame-specific scroll not possible without CDP, using main frame");
        }

        match engine {
            #[cfg(feature = "cef-browser")]
            Some(BrowserEngineWrapper::Cef(e)) => {
                match e.execute_js_with_result(uuid, &js).await {
                    Ok(Some(result)) => {
                        let value: serde_json::Value = serde_json::from_str(&result)
                            .unwrap_or(serde_json::Value::Null);
                        if let Some(err) = value.get("error").and_then(|e| e.as_str()) {
                            return IpcResponse::error(format!("Scroll failed: {}", err));
                        }
                        IpcResponse::success_with_data(value)
                    }
                    Ok(None) => IpcResponse::success(),
                    Err(e) => IpcResponse::error(e.to_string()),
                }
            }
            _ => {
                IpcResponse::error("No browser engine available for Scroll")
            }
        }
    }

    async fn handle_screenshot(
        &self,
        engine: &Option<BrowserEngineWrapper>,
        tab_id: &str,
        params: ScreenshotParams<'_>,
    ) -> IpcResponse {
        let ScreenshotParams { format, quality, full_page, selector: _selector, clip: _clip } = params;
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
                IpcResponse::error("No browser engine available for Screenshot")
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

        // Strategy 1: Try CDP Runtime.evaluate (bypasses CSP/Trusted Types)
        if let Some(ref cdp) = self.cdp_client {
            // Get the tab's current URL for target discovery
            let tab_url = match engine {
                #[cfg(feature = "cef-browser")]
                Some(BrowserEngineWrapper::Cef(e)) => {
                    e.get_tabs_sync().into_iter()
                        .find(|t| t.id == uuid)
                        .map(|t| t.url.clone())
                }
                _ => None,
            };

            if let Some(url) = tab_url {
                match cdp.find_target_ws_url(&url).await {
                    Ok(ws_url) => {
                        // If frame_id is set, use frame-isolated evaluation
                        if let Some(fid) = frame_id {
                            match crate::api::cdp_frames::evaluate_in_frame(cdp, &ws_url, fid, script, true).await {
                                Ok(result) => {
                                    debug!("CDP frame evaluate succeeded for tab {} frame '{}'", tab_id, fid);
                                    let value: serde_json::Value = serde_json::from_str(&result)
                                        .unwrap_or(serde_json::Value::String(result));
                                    return IpcResponse::success_with_data(serde_json::json!({
                                        "result": value
                                    }));
                                }
                                Err(e) => {
                                    debug!("CDP frame evaluate failed ({}), falling back to CEF", e);
                                }
                            }
                        } else {
                            // No frame_id — evaluate in main context
                            match cdp.evaluate(&ws_url, script).await {
                                Ok(result) => {
                                    debug!("CDP evaluate succeeded for tab {}", tab_id);
                                    let value: serde_json::Value = serde_json::from_str(&result)
                                        .unwrap_or(serde_json::Value::String(result));
                                    // Check for JS errors in the result
                                    if let Some(err) = value.get("__error").and_then(|e| e.as_str()) {
                                        return IpcResponse::error(format!("JavaScript error: {}", err));
                                    }
                                    if let Some(result_val) = value.get("result") {
                                        if let Some(err) = result_val.get("__error").and_then(|e| e.as_str()) {
                                            return IpcResponse::error(format!("JavaScript error: {}", err));
                                        }
                                    }
                                    return IpcResponse::success_with_data(serde_json::json!({
                                        "result": value
                                    }));
                                }
                                Err(e) => {
                                    debug!("CDP evaluate failed ({}), falling back to CEF", e);
                                }
                            }
                        }
                    }
                    Err(e) => {
                        debug!("CDP target discovery failed ({}), falling back to CEF", e);
                    }
                }
            }
        }

        // Strategy 2: Fallback to CEF execute_java_script (page context, subject to CSP)
        match engine {
            #[cfg(feature = "cef-browser")]
            Some(BrowserEngineWrapper::Cef(e)) => {
                if frame_id.is_some() {
                    warn!("Frame-specific evaluate not implemented for CEF, using main frame");
                }
                match e.execute_js_with_result(uuid, script).await {
                    Ok(Some(result)) => {
                        let value: serde_json::Value = serde_json::from_str(&result)
                            .unwrap_or(serde_json::Value::String(result));
                        // Check for JS errors in the result (CEF path)
                        if let Some(err) = value.get("__error").and_then(|e| e.as_str()) {
                            return IpcResponse::error(format!("JavaScript error: {}", err));
                        }
                        if let Some(result_val) = value.get("result") {
                            if let Some(err) = result_val.get("__error").and_then(|e| e.as_str()) {
                                return IpcResponse::error(format!("JavaScript error: {}", err));
                            }
                        }
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
                IpcResponse::error("No browser engine available for EvaluateScript")
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

                // CEF execute_js returns Option<String>, parse to Value first.
                // Handle double-encoded JSON (string containing JSON array).
                let elements_value: serde_json::Value = match elements_json_str {
                    Some(s) => {
                        let parsed: serde_json::Value = serde_json::from_str(&s).unwrap_or(serde_json::Value::Null);
                        // If the result is a string, it may be double-encoded JSON
                        if let serde_json::Value::String(ref inner) = parsed {
                            serde_json::from_str(inner).unwrap_or(parsed)
                        } else {
                            parsed
                        }
                    }
                    None => serde_json::Value::Null,
                };

                let elements: Vec<crate::browser::annotate::AnnotatedElement> =
                    match serde_json::from_value(elements_value.clone()) {
                        Ok(elems) => elems,
                        Err(e) => {
                            // If it's an array inside a wrapper object, try extracting it
                            if let Some(arr) = elements_value.as_object()
                                .and_then(|obj| obj.values().next())
                                .filter(|v| v.is_array()) {
                                serde_json::from_value(arr.clone()).unwrap_or_else(|_| {
                                    warn!("Failed to parse annotated elements from wrapper: {}", e);
                                    vec![]
                                })
                            } else {
                                return IpcResponse::error(format!("Failed to parse elements: {}", e))
                            }
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
                IpcResponse::error("No browser engine available for AnnotateElements")
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
                IpcResponse::error("No browser engine available for DomSnapshot")
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
                IpcResponse::error("No browser engine available for GetTabs")
            }
        }
    }

    async fn handle_get_frame_tree(
        &self,
        engine: &Option<BrowserEngineWrapper>,
        tab_id: &str,
    ) -> IpcResponse {
        let uuid = match Uuid::parse_str(tab_id) {
            Ok(u) => u,
            Err(_) => return IpcResponse::error("Invalid tab ID"),
        };

        match engine {
            #[cfg(feature = "cef-browser")]
            Some(BrowserEngineWrapper::Cef(e)) => {
                // Engine trait default returns empty vec, so use JS-based extraction
                let js = r#"
                    (() => {
                        const frames = [{
                            frame_id: 'main',
                            parent_frame_id: null,
                            url: location.href,
                            name: '',
                            security_origin: location.origin
                        }];
                        document.querySelectorAll('iframe').forEach((f, i) => {
                            frames.push({
                                frame_id: f.id || f.name || `frame-${i}`,
                                parent_frame_id: 'main',
                                url: f.src || '',
                                name: f.name || '',
                                security_origin: (() => { try { return new URL(f.src || location.href).origin; } catch(e) { return ''; } })()
                            });
                        });
                        return JSON.stringify(frames);
                    })()
                "#;
                match e.execute_js_with_result(uuid, js).await {
                    Ok(Some(result)) => {
                        let frames: serde_json::Value = serde_json::from_str(&result)
                            .unwrap_or(serde_json::Value::Array(vec![]));
                        // Handle double-encoded JSON string
                        let frames = if let serde_json::Value::String(ref s) = frames {
                            serde_json::from_str(s).unwrap_or(serde_json::Value::Array(vec![]))
                        } else {
                            frames
                        };
                        IpcResponse::success_with_data(serde_json::json!({
                            "frames": frames
                        }))
                    }
                    Ok(None) => IpcResponse::success_with_data(serde_json::json!({
                        "frames": []
                    })),
                    Err(e) => IpcResponse::error(e.to_string()),
                }
            }
            _ => {
                IpcResponse::error("No browser engine available for GetFrameTree")
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
            #[cfg(feature = "cef-browser")]
            Some(BrowserEngineWrapper::Cef(e)) => {
                match e.evaluate_in_frame(_uuid, frame_id, script).await {
                    Ok(value) => {
                        IpcResponse::success_with_data(serde_json::json!({
                            "result": value
                        }))
                    }
                    Err(e) => IpcResponse::error(e.to_string()),
                }
            }
            _ => {
                IpcResponse::error("No browser engine available for EvaluateInFrame")
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
