//! CEF callback handler implementations for browser lifecycle and rendering.
//!
//! Contains the CEF v144 handler structs that receive callbacks from the Chromium
//! Embedded Framework: application startup, client routing, off-screen render
//! handler, life span handler for browser creation/closing, load handler
//! for navigation events and stealth script injection, and display handler
//! for console message interception (JS result communication).

use cef::{
    // CEF v144 API - uses wrap_*! macros for handler implementations
    App, Browser, BrowserSettings, CefString, Client, Errorcode, Frame,
    LifeSpanHandler, LoadHandler, PaintElementType, Rect, RenderHandler,
    RenderProcessHandler, ScreenInfo, TransitionType, WindowInfo,
    WindowOpenDisposition, PopupFeatures, DictionaryValue, DisplayHandler,
    LogSeverity,
    // Traits needed by wrap_*! macro expansions
    ImplApp, WrapApp,
    ImplClient, WrapClient,
    ImplDisplayHandler, WrapDisplayHandler,
    ImplRenderHandler, WrapRenderHandler,
    ImplLifeSpanHandler, WrapLifeSpanHandler,
    ImplLoadHandler, WrapLoadHandler,
    ImplRenderProcessHandler, WrapRenderProcessHandler,
    // Traits needed to call methods on CEF types
    ImplCommandLine, ImplFrame, ImplBrowser,
    // rc module for Rc trait (needed by wrap macros)
    rc::Rc,
};
use parking_lot::RwLock;
use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use tokio::sync::mpsc;
use tracing::{debug, error, info};
use uuid::Uuid;

use crate::browser::tab::TabStatus;
use crate::stealth::StealthConfig;
use super::tab::CefTab;
use super::CefCommand;
use super::{BROWSER_ROUTER, RENDERER_ROUTER, JS_RESULT_STORE};

use cef::wrapper::message_router::{
    BrowserSideHandler, BrowserSideCallback,
    MessageRouterBrowserSideHandlerCallbacks, MessageRouterRendererSideHandlerCallbacks,
};

// ============================================================================
// BrowserSideHandler: receives cefQuery results from JavaScript
// ============================================================================

/// Handler that receives results from JS via cefQuery.
/// Protocol: JS calls window.cefQuery({request: "ki_result:<id>:<json>"})
pub(crate) struct KiBrowserQueryHandler;

impl BrowserSideHandler for KiBrowserQueryHandler {
    fn on_query_str(
        &self,
        _browser: Option<cef::Browser>,
        _frame: Option<cef::Frame>,
        _query_id: i64,
        request: &str,
        _persistent: bool,
        callback: std::sync::Arc<std::sync::Mutex<dyn BrowserSideCallback>>,
    ) -> bool {
        tracing::info!("KiBrowserQueryHandler::on_query_str called! request={}", &request[..request.len().min(100)]);
        // Protocol: "ki_result:<id>:<json_result>"
        if let Some(rest) = request.strip_prefix("ki_result:") {
            if let Some(colon_pos) = rest.find(':') {
                let id_str = &rest[..colon_pos];
                let result = &rest[colon_pos + 1..];

                if let Ok(id) = id_str.parse::<i64>() {
                    let sender = {
                        let store = JS_RESULT_STORE.lock();
                        store.get(&id).cloned()
                    };
                    if let Some(tx) = sender {
                        let _ = tx.send(Ok(result.to_string()));
                        JS_RESULT_STORE.lock().remove(&id);
                    }
                }

                // Signal success back to JS
                if let Ok(cb) = callback.lock() {
                    cb.success_str("ok");
                }
                return true;
            }
        }
        false
    }
}

// ============================================================================
// RenderProcessHandler: hooks cefQuery into each new JS context
// ============================================================================

cef::wrap_render_process_handler! {
    pub(crate) struct KiBrowserRenderProcessHandler {}

    impl RenderProcessHandler {
        fn on_context_created(
            &self,
            browser: Option<&mut cef::Browser>,
            frame: Option<&mut cef::Frame>,
            context: Option<&mut cef::V8Context>,
        ) {
            tracing::info!("RenderProcessHandler::on_context_created called!");
            RENDERER_ROUTER.on_context_created(
                browser.map(|b| b.clone()),
                frame.map(|f| f.clone()),
                context.map(|c| c.clone()),
            );
            tracing::info!("RenderProcessHandler::on_context_created - RENDERER_ROUTER done");
        }

        fn on_context_released(
            &self,
            browser: Option<&mut cef::Browser>,
            frame: Option<&mut cef::Frame>,
            context: Option<&mut cef::V8Context>,
        ) {
            RENDERER_ROUTER.on_context_released(
                browser.map(|b| b.clone()),
                frame.map(|f| f.clone()),
                context.map(|c| c.clone()),
            );
        }

        fn on_process_message_received(
            &self,
            browser: Option<&mut cef::Browser>,
            frame: Option<&mut cef::Frame>,
            source_process: cef::ProcessId,
            message: Option<&mut cef::ProcessMessage>,
        ) -> ::std::os::raw::c_int {
            let handled = RENDERER_ROUTER.on_process_message_received(
                browser.map(|b| b.clone()),
                frame.map(|f| f.clone()),
                Some(source_process),
                message.map(|m| m.clone()),
            );
            if handled { 1 } else { 0 }
        }
    }
}

// ============================================================================
// Application handler
// ============================================================================

/// Application handler for CEF lifecycle using v144 API.
///
/// Configures command line switches for stealth mode during CEF initialization.
/// In headless mode, GPU is disabled for stability. In GUI mode, GPU stays
/// enabled for hardware-accelerated rendering.
cef::wrap_app! {
    pub(crate) struct KiBrowserApp {
        stealth_config: Arc<StealthConfig>,
        render_process_handler_val: RenderProcessHandler,
        headless: bool,
    }

    impl App {
        fn on_before_command_line_processing(
            &self,
            _process_type: Option<&CefString>,
            command_line: Option<&mut cef::CommandLine>,
        ) {
            if let Some(cmd) = command_line {
                // Add arguments for stealth mode
                cmd.append_switch_with_value(Some(&CefString::from("disable-blink-features")), Some(&CefString::from("AutomationControlled")));
                cmd.append_switch(Some(&CefString::from("disable-infobars")));
                cmd.append_switch(Some(&CefString::from("disable-extensions")));
                cmd.append_switch(Some(&CefString::from("no-first-run")));
                cmd.append_switch(Some(&CefString::from("no-default-browser-check")));

                if self.headless {
                    // Headless: prefer real GPU if available, fall back to SwiftShader.
                    // A real GPU avoids the "SwiftShader" WebGL renderer string which
                    // is a strong bot-detection signal on sites like bot.sannysoft.com.
                    let has_real_gpu = std::path::Path::new("/dev/dri/renderD128").exists();

                    if has_real_gpu {
                        // Real GPU available — use hardware-accelerated ANGLE via EGL
                        cmd.append_switch_with_value(
                            Some(&CefString::from("use-gl")),
                            Some(&CefString::from("angle")),
                        );
                        cmd.append_switch_with_value(
                            Some(&CefString::from("use-angle")),
                            Some(&CefString::from("gl-egl")),
                        );
                        cmd.append_switch(Some(&CefString::from("enable-webgl")));
                        cmd.append_switch(Some(&CefString::from("in-process-gpu")));
                        cmd.append_switch(Some(&CefString::from("enable-gpu")));
                        cmd.append_switch(Some(&CefString::from("ignore-gpu-blocklist")));
                        cmd.append_switch(Some(&CefString::from("ignore-gpu-blacklist")));
                        debug!("CEF: Real GPU detected (/dev/dri/renderD128), using hardware WebGL (headless mode)");
                    } else {
                        // No real GPU — fall back to SwiftShader for software-based WebGL
                        cmd.append_switch_with_value(
                            Some(&CefString::from("use-gl")),
                            Some(&CefString::from("angle")),
                        );
                        cmd.append_switch_with_value(
                            Some(&CefString::from("use-angle")),
                            Some(&CefString::from("swiftshader")),
                        );
                        cmd.append_switch(Some(&CefString::from("enable-webgl")));
                        cmd.append_switch(Some(&CefString::from("disable-gpu-compositing")));
                        cmd.append_switch(Some(&CefString::from("in-process-gpu")));
                        debug!("CEF: No GPU found, using SwiftShader WebGL (headless mode)");
                    }
                } else {
                    // GUI: keep GPU enabled for hardware-accelerated rendering
                    cmd.append_switch(Some(&CefString::from("in-process-gpu")));
                    cmd.append_switch(Some(&CefString::from("enable-gpu-rasterization")));
                    debug!("CEF: GPU enabled (GUI mode)");
                }

                // Run network service in-process to avoid subprocess crashes
                cmd.append_switch_with_value(
                    Some(&CefString::from("disable-features")),
                    Some(&CefString::from("NetworkServiceSandbox")),
                );
                cmd.append_switch(Some(&CefString::from("single-process")));

                debug!("CEF command line configured for stealth mode");
            }
        }

        fn render_process_handler(&self) -> Option<RenderProcessHandler> {
            Some(self.render_process_handler_val.clone())
        }
    }
}

// ============================================================================
// Client handler
// ============================================================================

/// Client handler for browser events using v144 API.
///
/// Routes CEF callbacks to the appropriate sub-handlers for rendering,
/// life span management, page loading events, and display events.
cef::wrap_client! {
    pub(crate) struct KiBrowserClient {
        tab_id: Uuid,
        tabs: Arc<RwLock<HashMap<Uuid, CefTab>>>,
        stealth_config: Arc<StealthConfig>,
        render_handler_val: RenderHandler,
        life_span_handler_val: LifeSpanHandler,
        load_handler_val: LoadHandler,
        display_handler_val: DisplayHandler,
    }

    impl Client {
        fn render_handler(&self) -> Option<RenderHandler> {
            Some(self.render_handler_val.clone())
        }

        fn life_span_handler(&self) -> Option<LifeSpanHandler> {
            Some(self.life_span_handler_val.clone())
        }

        fn load_handler(&self) -> Option<LoadHandler> {
            Some(self.load_handler_val.clone())
        }

        fn display_handler(&self) -> Option<DisplayHandler> {
            Some(self.display_handler_val.clone())
        }

        fn on_process_message_received(
            &self,
            browser: Option<&mut Browser>,
            frame: Option<&mut Frame>,
            source_process: cef::ProcessId,
            message: Option<&mut cef::ProcessMessage>,
        ) -> ::std::os::raw::c_int {
            let handled = BROWSER_ROUTER.on_process_message_received(
                browser.map(|b| b.clone()),
                frame.map(|f| f.clone()),
                source_process,
                message.map(|m| m.clone()),
            );
            if handled { 1 } else { 0 }
        }
    }
}

// ============================================================================
// Render handler
// ============================================================================

/// Render handler for off-screen rendering using v144 API.
///
/// Receives paint callbacks from CEF and stores the raw BGRA frame buffer
/// for later screenshot capture. Manages viewport geometry for the headless
/// browser window.
cef::wrap_render_handler! {
    pub(crate) struct KiBrowserRenderHandlerImpl {
        tab_id: Uuid,
        frame_buffer: Arc<RwLock<Vec<u8>>>,
        frame_size: Arc<RwLock<(u32, u32)>>,
        viewport_size: Arc<RwLock<(u32, u32)>>,
        frame_version: Arc<std::sync::atomic::AtomicU64>,
    }

    impl RenderHandler {
        fn view_rect(&self, _browser: Option<&mut Browser>, rect: Option<&mut Rect>) {
            if let Some(r) = rect {
                let (w, h) = *self.viewport_size.read();
                r.x = 0;
                r.y = 0;
                r.width = w as i32;
                r.height = h as i32;
            }
        }

        fn screen_info(&self, _browser: Option<&mut Browser>, screen_info: Option<&mut ScreenInfo>) -> ::std::os::raw::c_int {
            if let Some(info) = screen_info {
                let (w, h) = *self.viewport_size.read();
                info.device_scale_factor = 1.0;
                info.depth = 32;
                info.depth_per_component = 8;
                info.is_monochrome = 0;
                info.rect = Rect {
                    x: 0,
                    y: 0,
                    width: w as i32,
                    height: h as i32,
                };
                info.available_rect = Rect {
                    x: 0,
                    y: 0,
                    width: w as i32,
                    height: h as i32,
                };
            }
            1 // Return true
        }

        fn on_paint(
            &self,
            _browser: Option<&mut Browser>,
            type_: PaintElementType,
            _dirty_rects: Option<&[Rect]>,
            buffer: *const u8,
            width: ::std::os::raw::c_int,
            height: ::std::os::raw::c_int,
        ) {
            if type_ == PaintElementType::VIEW {
                let buffer_size = (width * height * 4) as usize;
                if buffer.is_null() || buffer_size == 0 {
                    debug!("on_paint called with null/empty buffer for tab {}", self.tab_id);
                    return;
                }
                let buffer_slice = unsafe { std::slice::from_raw_parts(buffer, buffer_size) };

                let mut fb = self.frame_buffer.write();
                fb.clear();
                fb.extend_from_slice(buffer_slice);

                let mut size = self.frame_size.write();
                *size = (width as u32, height as u32);

                // Signal that a new frame is available (for stream encoder + GUI).
                self.frame_version.fetch_add(1, std::sync::atomic::Ordering::Release);
                #[cfg(feature = "gui")]
                crate::gui::viewport::bump_frame_version();

                debug!(
                    "on_paint: tab {} frame {}x{} ({} bytes)",
                    self.tab_id,
                    width,
                    height,
                    buffer_size
                );
            }
        }
    }
}

// ============================================================================
// Life span handler
// ============================================================================

/// Life span handler for tab lifecycle events using v144 API.
/// Includes popup interception for window.open() -> new tab.
cef::wrap_life_span_handler! {
    pub(crate) struct KiBrowserLifeSpanHandlerImpl {
        tab_id: Uuid,
        tabs: Arc<RwLock<HashMap<Uuid, CefTab>>>,
        browser_created: Arc<AtomicBool>,
        popup_tx: Option<mpsc::UnboundedSender<CefCommand>>,
    }

    impl LifeSpanHandler {
        fn on_before_popup(
            &self,
            _browser: Option<&mut Browser>,
            _frame: Option<&mut Frame>,
            _popup_id: ::std::os::raw::c_int,
            target_url: Option<&CefString>,
            _target_frame_name: Option<&CefString>,
            _target_disposition: WindowOpenDisposition,
            _user_gesture: ::std::os::raw::c_int,
            _popup_features: Option<&PopupFeatures>,
            _window_info: Option<&mut WindowInfo>,
            _client: Option<&mut Option<Client>>,
            _settings: Option<&mut BrowserSettings>,
            _extra_info: Option<&mut Option<DictionaryValue>>,
            _no_javascript_access: Option<&mut ::std::os::raw::c_int>,
        ) -> ::std::os::raw::c_int {
            // Intercept popup: store URL for API access + create new tab
            if let Some(url) = target_url {
                let url_str = url.to_string();
                info!("Popup intercepted for tab {}: {} -> stored + creating new tab", self.tab_id, url_str);

                // Store in global POPUP_URL_STORE so agents can query it
                {
                    let mut store = super::POPUP_URL_STORE.lock();
                    store.push_back((self.tab_id, url_str.clone(), std::time::Instant::now()));
                    // Keep max 32 entries
                    while store.len() > 32 {
                        store.pop_front();
                    }
                }

                // Also create a new internal tab with this URL
                if let Some(ref tx) = self.popup_tx {
                    let new_tab_id = Uuid::new_v4();
                    let (response_tx, _response_rx) = tokio::sync::oneshot::channel();
                    let cmd = CefCommand::CreateBrowser {
                        url: url_str,
                        tab_id: new_tab_id,
                        response: response_tx,
                    };
                    let _ = tx.send(cmd);
                }
            }
            // Return 1 = block the native popup (we handle it ourselves)
            1
        }

        fn on_after_created(&self, browser: Option<&mut Browser>) {
            info!("Browser created for tab {}", self.tab_id);

            // Store browser reference and browser_id in tab
            if let Some(b) = browser {
                let bid = b.identifier();
                let mut tabs = self.tabs.write();
                if let Some(tab) = tabs.get_mut(&self.tab_id) {
                    tab.set_browser(b.clone());
                    tab.browser_id = Some(bid);
                }
                info!(
                    "Tab {} mapped to CEF browser_id {} (CDP TargetId)",
                    self.tab_id, bid
                );
            }

            self.browser_created.store(true, Ordering::SeqCst);
        }

        fn on_before_close(&self, browser: Option<&mut Browser>) {
            info!("Browser closing for tab {}", self.tab_id);
            // Note: BROWSER_ROUTER.on_before_close() is NOT called here because in
            // single-process mode we use console.log for JS results, not MessageRouter.
            // Calling it without prior query registration causes a panic in
            // BrowserInfoMap::find_browser_all ("missing browser info map").
            let mut tabs = self.tabs.write();
            if let Some(tab) = tabs.get_mut(&self.tab_id) {
                tab.status = TabStatus::Closed;
                tab.browser = None;
            }
        }

        fn do_close(&self, _browser: Option<&mut Browser>) -> ::std::os::raw::c_int {
            // Return 0 (false) to allow the browser to close
            0
        }
    }
}

// ============================================================================
// Load handler
// ============================================================================

/// Load handler for navigation events and stealth injection using v144 API.
cef::wrap_load_handler! {
    pub(crate) struct KiBrowserLoadHandlerImpl {
        tab_id: Uuid,
        tabs: Arc<RwLock<HashMap<Uuid, CefTab>>>,
        stealth_config: Arc<StealthConfig>,
    }

    impl LoadHandler {
        fn on_loading_state_change(
            &self,
            _browser: Option<&mut Browser>,
            is_loading: ::std::os::raw::c_int,
            can_go_back: ::std::os::raw::c_int,
            can_go_forward: ::std::os::raw::c_int,
        ) {
            let is_loading_bool = is_loading != 0;
            let can_go_back_bool = can_go_back != 0;
            let can_go_forward_bool = can_go_forward != 0;

            let mut tabs = self.tabs.write();
            if let Some(tab) = tabs.get_mut(&self.tab_id) {
                if is_loading_bool {
                    tab.status = TabStatus::Loading;
                    tab.is_ready.store(false, Ordering::SeqCst);
                } else {
                    tab.status = TabStatus::Ready;
                    tab.is_ready.store(true, Ordering::SeqCst);
                }
                tab.can_go_back.store(can_go_back_bool, Ordering::SeqCst);
                tab.can_go_forward.store(can_go_forward_bool, Ordering::SeqCst);
            }

            debug!(
                "Loading state changed for tab {}: loading={}, back={}, forward={}",
                self.tab_id, is_loading_bool, can_go_back_bool, can_go_forward_bool
            );
        }

        fn on_load_start(
            &self,
            _browser: Option<&mut Browser>,
            frame: Option<&mut Frame>,
            _transition_type: TransitionType,
        ) {
            if let Some(f) = frame {
                if f.is_main() != 0 {
                    // Inject stealth scripts BEFORE any page scripts run
                    let stealth_script = self.stealth_config.get_complete_override_script();
                    let script_cef = CefString::from(stealth_script.as_str());
                    let empty_url = CefString::from("");
                    f.execute_java_script(Some(&script_cef), Some(&empty_url), 0);

                    debug!(
                        "Stealth scripts injected for tab {} on load start",
                        self.tab_id
                    );
                }
            }
        }

        fn on_load_end(
            &self,
            _browser: Option<&mut Browser>,
            frame: Option<&mut Frame>,
            http_status_code: ::std::os::raw::c_int,
        ) {
            if let Some(f) = frame {
                if f.is_main() != 0 {
                    // Update tab URL
                    let mut tabs = self.tabs.write();
                    if let Some(tab) = tabs.get_mut(&self.tab_id) {
                        let url = f.url();
                        tab.url = CefString::from(&url).to_string();
                    }

                    info!(
                        "Page loaded for tab {}: status={}",
                        self.tab_id, http_status_code
                    );
                }
            }
        }

        fn on_load_error(
            &self,
            _browser: Option<&mut Browser>,
            frame: Option<&mut Frame>,
            error_code: Errorcode,
            error_text: Option<&CefString>,
            failed_url: Option<&CefString>,
        ) {
            if let Some(f) = frame {
                if f.is_main() != 0 {
                    let url_str = failed_url.map(|u| u.to_string()).unwrap_or_default();
                    let err_str = error_text.map(|e| e.to_string()).unwrap_or_default();

                    let error_msg = format!(
                        "Failed to load {}: {:?} - {}",
                        url_str,
                        error_code,
                        err_str
                    );

                    let mut tabs = self.tabs.write();
                    if let Some(tab) = tabs.get_mut(&self.tab_id) {
                        tab.status = TabStatus::Error(error_msg.clone());
                    }

                    error!("Load error for tab {}: {}", self.tab_id, error_msg);
                }
            }
        }
    }
}

// ============================================================================
// DisplayHandler: captures console.log for JS result communication
// ============================================================================

/// Display handler that intercepts console messages containing JS execution results.
/// In single-process mode, CEF's MessageRouter IPC doesn't work, so we use
/// console.log("KI_RESULT:<id>:<json>") as a reliable same-process callback mechanism.
cef::wrap_display_handler! {
    pub(crate) struct KiBrowserDisplayHandlerImpl {
        tab_id: Uuid,
        tabs: Arc<RwLock<HashMap<Uuid, CefTab>>>,
    }

    impl DisplayHandler {
        fn on_title_change(
            &self,
            _browser: Option<&mut Browser>,
            title: Option<&CefString>,
        ) {
            if let Some(t) = title {
                let title_str = t.to_string();
                debug!("Title changed for tab {}: {}", self.tab_id, title_str);
                let mut tabs = self.tabs.write();
                if let Some(tab) = tabs.get_mut(&self.tab_id) {
                    tab.title = title_str;
                }
            }
        }

        fn on_console_message(
            &self,
            _browser: Option<&mut Browser>,
            _level: LogSeverity,
            message: Option<&CefString>,
            _source: Option<&CefString>,
            _line: ::std::os::raw::c_int,
        ) -> ::std::os::raw::c_int {
            if let Some(msg) = message {
                let msg_str = msg.to_string();
                if let Some(rest) = msg_str.strip_prefix("KI_RESULT:") {
                    if let Some(colon_pos) = rest.find(':') {
                        let id_str = &rest[..colon_pos];
                        let result = &rest[colon_pos + 1..];

                        if let Ok(id) = id_str.parse::<i64>() {
                            let sender = {
                                let store = JS_RESULT_STORE.lock();
                                store.get(&id).cloned()
                            };
                            if let Some(tx) = sender {
                                let _ = tx.send(Ok(result.to_string()));
                                JS_RESULT_STORE.lock().remove(&id);
                            }
                        }
                        return 1; // Suppress this console message from normal output
                    }
                }
            }
            0 // Don't suppress normal console messages
        }
    }
}
