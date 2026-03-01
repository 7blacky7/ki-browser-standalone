//! CEF (Chromium Embedded Framework) Browser Engine Implementation.
//!
//! This module provides a real browser engine implementation using CEF version 143.
//! It implements the `BrowserEngine` trait for seamless integration with the
//! ki-browser-standalone architecture.
//!
//! # Features
//!
//! - Off-screen rendering (OSR) for headless operation
//! - Multi-process architecture support
//! - Stealth script injection on page load
//! - Tab management with proper lifecycle handling
//! - JavaScript execution
//! - Screenshot capture
//!
//! # Example
//!
//! ```rust,no_run
//! use ki_browser_standalone::browser::{BrowserConfig, BrowserEngine, cef_engine::CefBrowserEngine};
//!
//! #[tokio::main]
//! async fn main() -> anyhow::Result<()> {
//!     let config = BrowserConfig::default().headless(true);
//!     let engine = CefBrowserEngine::new(config).await?;
//!
//!     let tab = engine.create_tab("https://example.com").await?;
//!     println!("Created tab: {}", tab.id);
//!
//!     engine.shutdown().await?;
//!     Ok(())
//! }
//! ```

#[cfg(feature = "cef-browser")]
use anyhow::{anyhow, Context, Result};
#[cfg(feature = "cef-browser")]
use async_trait::async_trait;
#[cfg(feature = "cef-browser")]
use cef::{
    // CEF v144 API - uses wrap_*! macros for handler implementations
    App, Browser, BrowserSettings,
    CefString, Client, Frame,
    LifeSpanHandler, LoadHandler, RenderHandler, RenderProcessHandler,
    PaintElementType, TransitionType,
    Rect, ScreenInfo, WindowInfo, Settings, LogSeverity,
    Errorcode, MainArgs, WindowOpenDisposition, PopupFeatures, DictionaryValue,
    DisplayHandler,
    // Traits needed by wrap_*! macro expansions
    ImplApp, WrapApp,
    ImplClient, WrapClient,
    ImplDisplayHandler, WrapDisplayHandler,
    ImplRenderHandler, WrapRenderHandler,
    ImplLifeSpanHandler, WrapLifeSpanHandler,
    ImplLoadHandler, WrapLoadHandler,
    ImplRenderProcessHandler, WrapRenderProcessHandler,
    // Traits needed to call methods on CEF types
    ImplCommandLine, ImplFrame, ImplBrowser, ImplBrowserHost,
    // rc module for Rc trait (needed by wrap macros)
    rc::Rc,
    sys,
};
#[cfg(feature = "cef-browser")]
use cef::wrapper::message_router::{
    MessageRouterConfig, BrowserSideRouter, RendererSideRouter,
    BrowserSideHandler, BrowserSideCallback,
    MessageRouterBrowserSide, MessageRouterRendererSide,
    MessageRouterBrowserSideHandlerCallbacks, MessageRouterRendererSideHandlerCallbacks,
};
#[cfg(feature = "cef-browser")]
use parking_lot::RwLock;
#[cfg(feature = "cef-browser")]
use std::collections::HashMap;
#[cfg(feature = "cef-browser")]
use std::sync::atomic::{AtomicBool, AtomicI32, Ordering};
#[cfg(feature = "cef-browser")]
use std::sync::Arc;
#[cfg(feature = "cef-browser")]
use tokio::sync::{mpsc, oneshot};
#[cfg(feature = "cef-browser")]
use tracing::{debug, error, info, trace, warn};
#[cfg(feature = "cef-browser")]
use uuid::Uuid;

#[cfg(feature = "cef-browser")]
use crate::browser::engine::{BrowserConfig, BrowserEngine};
#[cfg(feature = "cef-browser")]
use crate::browser::screenshot::{Screenshot, ScreenshotFormat, ScreenshotOptions};
#[cfg(feature = "cef-browser")]
use crate::browser::tab::{Tab, TabStatus};
#[cfg(feature = "cef-browser")]
use crate::stealth::StealthConfig;

// ============================================================================
// Constants
// ============================================================================

#[cfg(feature = "cef-browser")]
const CEF_MESSAGE_LOOP_DELAY_MS: u64 = 10;

// ============================================================================
// MessageRouter Global State
// ============================================================================

/// Type alias for the JS result sender map to reduce type complexity.
#[cfg(feature = "cef-browser")]
type JsResultStore = parking_lot::Mutex<std::collections::HashMap<i64, std::sync::mpsc::Sender<Result<String, String>>>>;

/// Type alias for tab frame buffer data (pixel buffer + dimensions).
#[cfg(feature = "cef-browser")]
pub type TabFrameBuffer = (Arc<RwLock<Vec<u8>>>, Arc<RwLock<(u32, u32)>>);

/// Global JS-Result-Store: query_id → mpsc::Sender for results.
/// Used to pass cefQuery results back to execute_js_with_result_internal.
#[cfg(feature = "cef-browser")]
static JS_RESULT_STORE: once_cell::sync::Lazy<JsResultStore> =
    once_cell::sync::Lazy::new(|| parking_lot::Mutex::new(std::collections::HashMap::new()));

/// Global BrowserSideRouter (initialized once on first use on the CEF thread).
#[cfg(feature = "cef-browser")]
static BROWSER_ROUTER: once_cell::sync::Lazy<std::sync::Arc<BrowserSideRouter>> =
    once_cell::sync::Lazy::new(|| {
        let router: std::sync::Arc<BrowserSideRouter> =
            MessageRouterBrowserSide::new(MessageRouterConfig::default());
        router.add_handler(std::sync::Arc::new(KiBrowserQueryHandler), true);
        router
    });

/// Global RendererSideRouter (initialized once on first use on the render thread).
#[cfg(feature = "cef-browser")]
static RENDERER_ROUTER: once_cell::sync::Lazy<std::sync::Arc<RendererSideRouter>> =
    once_cell::sync::Lazy::new(|| {
        MessageRouterRendererSide::new(MessageRouterConfig::default())
    });

// ============================================================================
// BrowserSideHandler: receives cefQuery results from JavaScript
// ============================================================================

/// Handler that receives results from JS via cefQuery.
/// Protocol: JS calls window.cefQuery({request: "ki_result:<id>:<json>"})
#[cfg(feature = "cef-browser")]
struct KiBrowserQueryHandler;

#[cfg(feature = "cef-browser")]
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

#[cfg(feature = "cef-browser")]
cef::wrap_render_process_handler! {
    struct KiBrowserRenderProcessHandler {}

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

#[cfg(feature = "cef-browser")]
const DEFAULT_FRAME_RATE: i32 = 30;

// ============================================================================
// Internal Types
// ============================================================================

/// Internal representation of a CEF browser tab.
#[cfg(feature = "cef-browser")]
struct CefTab {
    /// Unique identifier for the tab.
    id: Uuid,
    /// CEF browser instance (set asynchronously after on_after_created).
    browser: Option<Browser>,
    /// CEF browser identifier used as CDP TargetId for remote debugging.
    browser_id: Option<i32>,
    /// Current URL of the tab.
    url: String,
    /// Page title.
    title: String,
    /// Current status.
    status: TabStatus,
    /// Last rendered frame buffer (BGRA format).
    frame_buffer: Arc<RwLock<Vec<u8>>>,
    /// Frame dimensions.
    frame_size: Arc<RwLock<(u32, u32)>>,
    /// Whether the tab is ready for interaction.
    is_ready: AtomicBool,
    /// Whether the browser can navigate back in history.
    can_go_back: AtomicBool,
    /// Whether the browser can navigate forward in history.
    can_go_forward: AtomicBool,
    /// Shared viewport dimensions for the render handler (updated on resize).
    viewport_size: Arc<RwLock<(u32, u32)>>,
}

#[cfg(feature = "cef-browser")]
impl CefTab {
    fn new(id: Uuid, url: String, frame_buffer: Arc<RwLock<Vec<u8>>>, frame_size: Arc<RwLock<(u32, u32)>>, viewport_size: Arc<RwLock<(u32, u32)>>) -> Self {
        Self {
            id,
            browser: None,
            browser_id: None,
            url,
            title: String::new(),
            status: TabStatus::Loading,
            frame_buffer,
            frame_size,
            is_ready: AtomicBool::new(false),
            can_go_back: AtomicBool::new(false),
            can_go_forward: AtomicBool::new(false),
            viewport_size,
        }
    }

    fn set_browser(&mut self, browser: Browser) {
        self.browser = Some(browser);
    }

    fn to_tab(&self) -> Tab {
        let mut tab = Tab::new(self.url.clone());
        // Override the auto-generated ID with our tracked ID
        tab.id = self.id;
        tab.title = self.title.clone();
        tab.status = self.status.clone();
        if self.is_ready.load(Ordering::SeqCst) {
            tab.set_ready();
        }
        tab
    }
}

/// Commands for the CEF message loop thread.
#[cfg(feature = "cef-browser")]
pub(crate) enum CefCommand {
    CreateBrowser {
        url: String,
        tab_id: Uuid,
        response: oneshot::Sender<Result<()>>,
    },
    CloseBrowser {
        tab_id: Uuid,
        response: oneshot::Sender<Result<()>>,
    },
    Navigate {
        tab_id: Uuid,
        url: String,
        response: oneshot::Sender<Result<()>>,
    },
    ExecuteJs {
        tab_id: Uuid,
        script: String,
        response: oneshot::Sender<Result<Option<String>>>,
    },
    Screenshot {
        tab_id: Uuid,
        options: ScreenshotOptions,
        response: oneshot::Sender<Result<Screenshot>>,
    },
    // Input commands
    MouseMove {
        tab_id: Uuid,
        x: i32,
        y: i32,
        response: oneshot::Sender<Result<()>>,
    },
    MouseClick {
        tab_id: Uuid,
        x: i32,
        y: i32,
        button: i32,
        click_count: i32,
        response: oneshot::Sender<Result<()>>,
    },
    MouseWheel {
        tab_id: Uuid,
        x: i32,
        y: i32,
        delta_x: i32,
        delta_y: i32,
        response: oneshot::Sender<Result<()>>,
    },
    KeyEvent {
        tab_id: Uuid,
        event_type: i32,
        modifiers: u32,
        windows_key_code: i32,
        character: u16,
        response: oneshot::Sender<Result<()>>,
    },
    TypeText {
        tab_id: Uuid,
        text: String,
        response: oneshot::Sender<Result<()>>,
    },
    Drag {
        tab_id: Uuid,
        from_x: i32,
        from_y: i32,
        to_x: i32,
        to_y: i32,
        steps: u32,
        duration_ms: u64,
        response: oneshot::Sender<Result<()>>,
    },
    ExecuteJsWithResult {
        tab_id: Uuid,
        script: String,
        response: oneshot::Sender<Result<Option<String>>>,
    },
    /// Navigate the browser back in history.
    GoBack {
        tab_id: Uuid,
        response: oneshot::Sender<Result<()>>,
    },
    /// Navigate the browser forward in history.
    GoForward {
        tab_id: Uuid,
        response: oneshot::Sender<Result<()>>,
    },
    /// Resize the CEF viewport for a tab and notify the browser.
    ResizeViewport {
        tab_id: Uuid,
        width: u32,
        height: u32,
        response: oneshot::Sender<Result<()>>,
    },
    Shutdown {
        response: oneshot::Sender<Result<()>>,
    },
}

// ============================================================================
// CEF Browser Event Sender Implementation
// ============================================================================

/// Event sender implementation for connecting CefInputHandler to CefBrowserEngine.
///
/// This struct bridges the input handler with the CEF browser by sending
/// input events through the command channel to be processed on the CEF thread.
#[cfg(feature = "cef-browser")]
pub struct CefBrowserEventSender {
    /// Tab ID this sender is associated with.
    tab_id: Uuid,
    /// Command sender for the CEF message loop (unbounded = never drops).
    command_tx: mpsc::UnboundedSender<CefCommand>,
}

#[cfg(feature = "cef-browser")]
impl CefBrowserEventSender {
    /// Creates a new event sender for a specific tab.
    pub(crate) fn new(tab_id: Uuid, command_tx: mpsc::UnboundedSender<CefCommand>) -> Self {
        Self {
            tab_id,
            command_tx,
        }
    }
}

#[cfg(feature = "cef-browser")]
impl crate::browser::cef_input::CefEventSender for CefBrowserEventSender {
    fn send_mouse_move_event(&self, event: &crate::browser::cef_input::CefMouseEvent, _mouse_leave: bool) {
        let (tx, _rx) = oneshot::channel();
        let cmd = CefCommand::MouseMove {
            tab_id: self.tab_id,
            x: event.x,
            y: event.y,
            response: tx,
        };
        // Fire and forget - we don't wait for response on move events
        let _ = self.command_tx.send(cmd);
    }

    fn send_mouse_click_event(
        &self,
        event: &crate::browser::cef_input::CefMouseEvent,
        button: crate::browser::cef_input::CefMouseButton,
        mouse_up: bool,
        click_count: i32,
    ) {
        let (tx, _rx) = oneshot::channel();
        // Encode mouse_up in the click_count (negative = up)
        let encoded_count = if mouse_up { -click_count } else { click_count };
        let cmd = CefCommand::MouseClick {
            tab_id: self.tab_id,
            x: event.x,
            y: event.y,
            button: button.to_cef_type(),
            click_count: encoded_count,
            response: tx,
        };
        let _ = self.command_tx.send(cmd);
    }

    fn send_mouse_wheel_event(&self, event: &crate::browser::cef_input::CefMouseEvent, delta_x: i32, delta_y: i32) {
        let (tx, _rx) = oneshot::channel();
        let cmd = CefCommand::MouseWheel {
            tab_id: self.tab_id,
            x: event.x,
            y: event.y,
            delta_x,
            delta_y,
            response: tx,
        };
        let _ = self.command_tx.send(cmd);
    }

    fn send_key_event(&self, event: &crate::browser::cef_input::CefKeyEvent) {
        let (tx, _rx) = oneshot::channel();
        let cmd = CefCommand::KeyEvent {
            tab_id: self.tab_id,
            event_type: event.event_type.to_cef_type(),
            modifiers: event.modifiers,
            windows_key_code: event.windows_key_code,
            character: event.character,
            response: tx,
        };
        let _ = self.command_tx.send(cmd);
    }
}

// ============================================================================
// CEF Callbacks Implementation
// ============================================================================

// Application handler for CEF lifecycle using v144 API.
#[cfg(feature = "cef-browser")]
cef::wrap_app! {
    struct KiBrowserApp {
        stealth_config: Arc<StealthConfig>,
        render_process_handler_val: RenderProcessHandler,
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

                // Disable GPU process (prevents GPU subprocess crash)
                cmd.append_switch(Some(&CefString::from("disable-gpu")));
                cmd.append_switch(Some(&CefString::from("disable-gpu-compositing")));
                cmd.append_switch(Some(&CefString::from("in-process-gpu")));
                cmd.append_switch(Some(&CefString::from("disable-software-rasterizer")));

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

// Client handler for browser events using v144 API.
#[cfg(feature = "cef-browser")]
cef::wrap_client! {
    struct KiBrowserClient {
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

// Render handler for off-screen rendering using v144 API.
#[cfg(feature = "cef-browser")]
cef::wrap_render_handler! {
    struct KiBrowserRenderHandlerImpl {
        tab_id: Uuid,
        frame_buffer: Arc<RwLock<Vec<u8>>>,
        frame_size: Arc<RwLock<(u32, u32)>>,
        viewport_size: Arc<RwLock<(u32, u32)>>,
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

                // Signal the GUI that a new frame is available.
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

// Life span handler for tab lifecycle events using v144 API.
// Includes popup interception for window.open() -> new tab.
#[cfg(feature = "cef-browser")]
cef::wrap_life_span_handler! {
    struct KiBrowserLifeSpanHandlerImpl {
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
            // Intercept popup: create new tab instead of new window
            if let Some(url) = target_url {
                let url_str = url.to_string();
                info!("Popup intercepted for tab {}: {} → creating new tab", self.tab_id, url_str);

                if let Some(ref tx) = self.popup_tx {
                    let new_tab_id = Uuid::new_v4();
                    let (response_tx, _response_rx) = oneshot::channel();
                    let cmd = CefCommand::CreateBrowser {
                        url: url_str,
                        tab_id: new_tab_id,
                        response: response_tx,
                    };
                    // Fire and forget - don't block the CEF callback
                    let _ = tx.send(cmd);
                }
            }
            // Return 1 = block the popup (we handle it ourselves)
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
            // Notify the MessageRouter so it can cancel pending queries for this browser.
            BROWSER_ROUTER.on_before_close(browser.map(|b| b.clone()));
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

// Load handler for navigation events and stealth injection using v144 API.
#[cfg(feature = "cef-browser")]
cef::wrap_load_handler! {
    struct KiBrowserLoadHandlerImpl {
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

// Display handler that intercepts console messages containing JS execution results.
// In single-process mode, CEF's MessageRouter IPC doesn't work, so we use
// console.log("KI_RESULT:<id>:<json>") as a reliable same-process callback mechanism.
#[cfg(feature = "cef-browser")]
cef::wrap_display_handler! {
    struct KiBrowserDisplayHandlerImpl {
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

// ============================================================================
// CefBrowserEngine Implementation
// ============================================================================

/// CEF-based browser engine implementation.
///
/// This struct provides a complete browser engine using the Chromium Embedded Framework.
/// It supports headless operation through off-screen rendering (OSR) and includes
/// built-in stealth capabilities.
#[cfg(feature = "cef-browser")]
pub struct CefBrowserEngine {
    /// Browser configuration.
    config: BrowserConfig,
    /// Stealth configuration for anti-detection.
    stealth_config: Arc<StealthConfig>,
    /// Active tabs indexed by UUID.
    tabs: Arc<RwLock<HashMap<Uuid, CefTab>>>,
    /// Command sender for the CEF message loop thread (unbounded = never drops).
    command_tx: mpsc::UnboundedSender<CefCommand>,
    /// Whether the engine is running.
    is_running: Arc<AtomicBool>,
    /// CEF initialized flag (v144 doesn't have CefContext).
    _cef_initialized: Arc<AtomicBool>,
    /// Browser ID counter.
    _browser_id_counter: Arc<AtomicI32>,
}

#[cfg(feature = "cef-browser")]
#[async_trait]
impl BrowserEngine for CefBrowserEngine {
    async fn new(config: BrowserConfig) -> Result<Self>
    where
        Self: Sized,
    {
        info!("Initializing CEF browser engine");

        // Create stealth configuration
        let mut stealth = if let Some(ref user_agent) = config.user_agent {
            // Use consistent fingerprint based on user agent
            StealthConfig::consistent(user_agent)
        } else {
            StealthConfig::random()
        };

        // Sync screen resolution to the actual viewport so that
        // screen.width >= outerWidth >= innerWidth and orientation is correct.
        stealth.sync_screen_to_viewport(config.window_size.0, config.window_size.1);

        let stealth_config = Arc::new(stealth);

        // Validate stealth config
        stealth_config
            .validate()
            .map_err(|e| anyhow!("Invalid stealth config: {}", e))?;

        let tabs = Arc::new(RwLock::new(HashMap::new()));
        let is_running = Arc::new(AtomicBool::new(false));
        let browser_id_counter = Arc::new(AtomicI32::new(0));

        // Create command channel for CEF thread communication
        let (command_tx, command_rx) = mpsc::unbounded_channel::<CefCommand>();

        // Clone references for the CEF thread
        let tabs_clone = tabs.clone();
        let is_running_clone = is_running.clone();
        let config_clone = config.clone();
        let stealth_config_clone = stealth_config.clone();
        let browser_id_counter_clone = browser_id_counter.clone();

        // CEF initialized flag (v144 doesn't have CefContext)
        let cef_initialized = Arc::new(AtomicBool::new(false));
        let cef_initialized_clone = cef_initialized.clone();

        // Spawn CEF message loop thread
        std::thread::spawn(move || {
            let result = Self::run_cef_message_loop(
                config_clone,
                stealth_config_clone,
                tabs_clone,
                is_running_clone,
                browser_id_counter_clone,
                cef_initialized_clone,
                command_rx,
            );

            if let Err(e) = result {
                error!("CEF message loop error: {}", e);
            }
        });

        // Wait for CEF to initialize
        tokio::time::sleep(tokio::time::Duration::from_millis(500)).await;

        if !is_running.load(Ordering::SeqCst) {
            return Err(anyhow!("Failed to initialize CEF"));
        }

        info!("CEF browser engine initialized successfully");

        Ok(Self {
            config,
            stealth_config,
            tabs,
            command_tx,
            is_running,
            _cef_initialized: cef_initialized,
            _browser_id_counter: browser_id_counter,
        })
    }

    async fn shutdown(&self) -> Result<()> {
        info!("Shutting down CEF browser engine");

        let (response_tx, response_rx) = oneshot::channel();

        self.command_tx
            .send(CefCommand::Shutdown {
                response: response_tx,
            })
            .map_err(|_| anyhow!("Failed to send shutdown command"))?;

        response_rx.await.context("Failed to receive shutdown response")?
    }

    async fn create_tab(&self, url: &str) -> Result<Tab> {
        if !self.is_running.load(Ordering::SeqCst) {
            return Err(anyhow!("Browser engine is not running"));
        }

        let tab_id = Uuid::new_v4();
        let (response_tx, response_rx) = oneshot::channel();

        self.command_tx
            .send(CefCommand::CreateBrowser {
                url: url.to_string(),
                tab_id,
                response: response_tx,
            })
            .map_err(|_| anyhow!("Failed to send create browser command"))?;

        response_rx.await.context("Failed to receive create browser response")??;

        // Wait for browser to be created
        tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;

        // Return the tab info
        let tabs = self.tabs.read();
        tabs.get(&tab_id)
            .map(|t| t.to_tab())
            .ok_or_else(|| anyhow!("Failed to create tab"))
    }

    async fn close_tab(&self, tab_id: Uuid) -> Result<()> {
        if !self.is_running.load(Ordering::SeqCst) {
            return Err(anyhow!("Browser engine is not running"));
        }

        let (response_tx, response_rx) = oneshot::channel();

        self.command_tx
            .send(CefCommand::CloseBrowser {
                tab_id,
                response: response_tx,
            })
            .map_err(|_| anyhow!("Failed to send close browser command"))?;

        response_rx.await.context("Failed to receive close browser response")?
    }

    async fn get_tabs(&self) -> Result<Vec<Tab>> {
        if !self.is_running.load(Ordering::SeqCst) {
            return Err(anyhow!("Browser engine is not running"));
        }

        let tabs = self.tabs.read();
        Ok(tabs.values().map(|t| t.to_tab()).collect())
    }

    async fn get_tab(&self, tab_id: Uuid) -> Result<Option<Tab>> {
        if !self.is_running.load(Ordering::SeqCst) {
            return Err(anyhow!("Browser engine is not running"));
        }

        let tabs = self.tabs.read();
        Ok(tabs.get(&tab_id).map(|t| t.to_tab()))
    }

    fn config(&self) -> &BrowserConfig {
        &self.config
    }

    async fn is_running(&self) -> bool {
        self.is_running.load(Ordering::SeqCst)
    }
}

#[cfg(feature = "cef-browser")]
impl CefBrowserEngine {
    /// Find the CEF directory containing libcef.so and resources (static version for main.rs).
    pub fn find_cef_dir_static() -> Option<std::path::PathBuf> {
        Self::find_cef_dir()
    }

    /// Returns the CEF browser_id for a given tab UUID, used as CDP TargetId.
    ///
    /// This identifier corresponds to the CDP target that external debugging
    /// tools (Puppeteer, Playwright, Chrome DevTools) use to connect to a
    /// specific browser tab via the remote debugging protocol.
    pub fn get_browser_id(&self, tab_id: &Uuid) -> Option<i32> {
        let tabs = self.tabs.read();
        tabs.get(tab_id).and_then(|t| t.browser_id)
    }

    /// Returns all tab UUID to browser_id mappings for CDP target discovery.
    ///
    /// Used by the CDP mapping service to synchronize mappings for all
    /// currently open tabs.
    pub fn get_all_browser_ids(&self) -> Vec<(Uuid, i32)> {
        let tabs = self.tabs.read();
        tabs.values()
            .filter_map(|t| t.browser_id.map(|bid| (t.id, bid)))
            .collect()
    }

    /// Find the CEF directory containing libcef.so and resources.
    /// Checks: CEF_PATH env, cef-dll-sys build output, ./cef/
    fn find_cef_dir() -> Option<std::path::PathBuf> {
        // 1. CEF_PATH environment variable
        if let Ok(path) = std::env::var("CEF_PATH") {
            let p = std::path::PathBuf::from(&path);
            if p.exists() {
                return Some(p);
            }
        }

        // 2. cef-dll-sys build output (look for libcef.so next to our binary in target/)
        if let Ok(exe) = std::env::current_exe() {
            // Walk up from the executable to find target/debug/build/cef-dll-sys-*/out/cef_linux_x86_64/
            if let Some(target_dir) = exe.parent() {
                // target/debug/ or target/release/
                let build_dir = target_dir.join("build");
                if build_dir.exists() {
                    if let Ok(entries) = std::fs::read_dir(&build_dir) {
                        for entry in entries.flatten() {
                            let name = entry.file_name().to_string_lossy().to_string();
                            if name.starts_with("cef-dll-sys-") {
                                let cef_path = entry.path().join("out/cef_linux_x86_64");
                                if cef_path.join("libcef.so").exists() {
                                    return Some(cef_path);
                                }
                            }
                        }
                    }
                }
            }
        }

        // 3. ./cef/Release/ (manual download)
        let local = std::path::PathBuf::from("./cef/Release");
        if local.join("libcef.so").exists() {
            return Some(local);
        }

        // 4. ./cef/ (flat structure)
        let local_flat = std::path::PathBuf::from("./cef");
        if local_flat.join("libcef.so").exists() {
            return Some(local_flat);
        }

        None
    }

    /// Runs the CEF message loop on a dedicated thread.
    fn run_cef_message_loop(
        config: BrowserConfig,
        stealth_config: Arc<StealthConfig>,
        tabs: Arc<RwLock<HashMap<Uuid, CefTab>>>,
        is_running: Arc<AtomicBool>,
        browser_id_counter: Arc<AtomicI32>,
        cef_initialized: Arc<AtomicBool>,
        mut command_rx: mpsc::UnboundedReceiver<CefCommand>,
    ) -> Result<()> {
        // Find CEF directory (build output or ./cef/)
        let cef_dir = Self::find_cef_dir();
        info!("CEF directory: {:?}", cef_dir);

        // Configure CEF settings - use run_message_loop() style (not external pump)
        let mut settings = Settings {
            windowless_rendering_enabled: 1,
            no_sandbox: 1,
            multi_threaded_message_loop: 0,
            external_message_pump: 1, // We pump CEF via do_message_loop_work()
            ..Default::default()
        };

        // Note: LD_LIBRARY_PATH is no longer needed here.
        // build.rs copies libcef.so to target/<profile>/ and sets RPATH=$ORIGIN,
        // so the dynamic linker finds it automatically.

        // Set unique cache path to avoid singleton conflicts
        let cache_dir = format!("/tmp/ki-browser-cef-{}", std::process::id());
        settings.root_cache_path = CefString::from(cache_dir.as_str());
        settings.cache_path = CefString::from(cache_dir.as_str());

        if config.headless {
            settings.windowless_rendering_enabled = 1;
        }

        // Set user agent if provided
        if let Some(ref user_agent) = config.user_agent {
            settings.user_agent = CefString::from(user_agent.as_str());
        }

        // Enable CDP remote debugging if configured (used by Playwright/DevTools)
        if let Some(port) = config.cdp_port {
            if port > 0 {
                settings.remote_debugging_port = port as i32;
                info!("CDP remote debugging enabled on port {}", port);
            }
        }

        // Set log level
        settings.log_severity = LogSeverity::WARNING;

        // CRITICAL: Initialize CEF API version BEFORE anything else
        // Without this, CEF v144 rejects all handler structs with "invalid version -1"
        let _ = cef::api_hash(sys::CEF_API_VERSION_LAST, 0);

        // Call execute_process for subprocess support (returns -1 for browser process)
        let args = MainArgs::default();
        let ret = cef::execute_process(Some(&args), None, std::ptr::null_mut());
        if ret >= 0 {
            // This is a subprocess, exit with the return code
            std::process::exit(ret);
        }
        // ret == -1 means we are the browser process, continue

        // Create render process handler for MessageRouter context hooks
        let rph = KiBrowserRenderProcessHandler::new();

        // Create app with v144 API (wrap_app! macro generates ::new())
        let mut app = KiBrowserApp::new(stealth_config.clone(), rph);

        // Initialize CEF using v144 API
        let result = cef::initialize(
            Some(&args),
            Some(&settings),
            Some(&mut app),
            std::ptr::null_mut(),
        );

        if result == 0 {
            return Err(anyhow!("Failed to initialize CEF"));
        }

        info!("CEF initialized successfully");

        cef_initialized.store(true, Ordering::SeqCst);
        is_running.store(true, Ordering::SeqCst);

        // Message loop
        'main_loop: loop {
            // Process CEF work
            cef::do_message_loop_work();

            // Drain ALL pending commands (not just one per iteration)
            loop {
                match command_rx.try_recv() {
                    Ok(command) => {
                        match command {
                            CefCommand::CreateBrowser {
                                url,
                                tab_id,
                                response,
                            } => {
                                let result = Self::create_browser_internal(
                                    &url,
                                    tab_id,
                                    &config,
                                    stealth_config.clone(),
                                    tabs.clone(),
                                    browser_id_counter.clone(),
                                );
                                let _ = response.send(result);
                            }
                            CefCommand::CloseBrowser { tab_id, response } => {
                                let result = Self::close_browser_internal(tab_id, tabs.clone());
                                let _ = response.send(result);
                            }
                            CefCommand::Navigate {
                                tab_id,
                                url,
                                response,
                            } => {
                                let result = Self::navigate_internal(tab_id, &url, tabs.clone());
                                let _ = response.send(result);
                            }
                            CefCommand::ExecuteJs {
                                tab_id,
                                script,
                                response,
                            } => {
                                let result = Self::execute_js_internal(tab_id, &script, tabs.clone());
                                let _ = response.send(result);
                            }
                            CefCommand::ExecuteJsWithResult {
                                tab_id,
                                script,
                                response,
                            } => {
                                let result = Self::execute_js_with_result_internal(tab_id, &script, tabs.clone());
                                let _ = response.send(result);
                            }
                            CefCommand::Screenshot {
                                tab_id,
                                options,
                                response,
                            } => {
                                let result = Self::screenshot_internal(tab_id, &options, tabs.clone());
                                let _ = response.send(result);
                            }
                            CefCommand::MouseMove {
                                tab_id,
                                x,
                                y,
                                response,
                            } => {
                                let result = Self::mouse_move_internal(tab_id, x, y, tabs.clone());
                                let _ = response.send(result);
                            }
                            CefCommand::MouseClick {
                                tab_id,
                                x,
                                y,
                                button,
                                click_count,
                                response,
                            } => {
                                let result = Self::mouse_click_internal(tab_id, x, y, button, click_count, tabs.clone());
                                let _ = response.send(result);
                            }
                            CefCommand::MouseWheel {
                                tab_id,
                                x,
                                y,
                                delta_x,
                                delta_y,
                                response,
                            } => {
                                let result = Self::mouse_wheel_internal(tab_id, x, y, delta_x, delta_y, tabs.clone());
                                let _ = response.send(result);
                            }
                            CefCommand::KeyEvent {
                                tab_id,
                                event_type,
                                modifiers,
                                windows_key_code,
                                character,
                                response,
                            } => {
                                let result = Self::key_event_internal(tab_id, event_type, modifiers, windows_key_code, character, tabs.clone());
                                let _ = response.send(result);
                            }
                            CefCommand::TypeText {
                                tab_id,
                                text,
                                response,
                            } => {
                                let result = Self::type_text_internal(tab_id, &text, tabs.clone());
                                let _ = response.send(result);
                            }
                            CefCommand::Drag {
                                tab_id,
                                from_x,
                                from_y,
                                to_x,
                                to_y,
                                steps,
                                duration_ms,
                                response,
                            } => {
                                let result = Self::drag_internal(tab_id, from_x, from_y, to_x, to_y, steps, duration_ms, tabs.clone());
                                let _ = response.send(result);
                            }
                            CefCommand::GoBack { tab_id, response } => {
                                let result = Self::go_back_internal(tab_id, tabs.clone());
                                let _ = response.send(result);
                            }
                            CefCommand::GoForward { tab_id, response } => {
                                let result = Self::go_forward_internal(tab_id, tabs.clone());
                                let _ = response.send(result);
                            }
                            CefCommand::ResizeViewport {
                                tab_id,
                                width,
                                height,
                                response,
                            } => {
                                let result = Self::resize_viewport_internal(tab_id, width, height, tabs.clone());
                                let _ = response.send(result);
                            }
                            CefCommand::Shutdown { response } => {
                                info!("Processing shutdown command");

                                // Close all browsers
                                let tab_ids: Vec<Uuid> = {
                                    let tabs_guard = tabs.read();
                                    tabs_guard.keys().cloned().collect()
                                };

                                for tab_id in &tab_ids {
                                    let _ = Self::close_browser_internal(*tab_id, tabs.clone());
                                }

                                // Pump the CEF message loop so on_before_close callbacks
                                // can fire and CEF can clean up its internal browser_info_map.
                                // Without this, cef::shutdown() panics with "missing browser info map".
                                if !tab_ids.is_empty() {
                                    info!("Pumping CEF message loop for browser cleanup ({} browsers)", tab_ids.len());
                                    for _ in 0..50 {
                                        cef::do_message_loop_work();
                                        std::thread::sleep(std::time::Duration::from_millis(10));
                                    }
                                }

                                is_running.store(false, Ordering::SeqCst);
                                let _ = response.send(Ok(()));
                                break 'main_loop;
                            }
                        }
                    }
                    Err(mpsc::error::TryRecvError::Empty) => {
                        // All commands drained, back to CEF work
                        break;
                    }
                    Err(mpsc::error::TryRecvError::Disconnected) => {
                        warn!("Command channel disconnected");
                        break 'main_loop;
                    }
                }
            }

            // Small delay to prevent CPU spinning
            std::thread::sleep(std::time::Duration::from_millis(CEF_MESSAGE_LOOP_DELAY_MS));
        }

        // Shutdown CEF
        info!("Shutting down CEF context");
        cef::shutdown();

        Ok(())
    }

    /// Creates a browser instance internally on the CEF thread.
    fn create_browser_internal(
        url: &str,
        tab_id: Uuid,
        config: &BrowserConfig,
        stealth_config: Arc<StealthConfig>,
        tabs: Arc<RwLock<HashMap<Uuid, CefTab>>>,
        browser_id_counter: Arc<AtomicI32>,
    ) -> Result<()> {
        let viewport_dims = config.window_size;
        let viewport_size = Arc::new(RwLock::new(viewport_dims));

        // Create frame buffer for OSR
        let frame_buffer = Arc::new(RwLock::new(Vec::with_capacity(
            (viewport_dims.0 * viewport_dims.1 * 4) as usize,
        )));
        let frame_size = Arc::new(RwLock::new((0u32, 0u32)));
        let browser_created = Arc::new(AtomicBool::new(false));

        // Create render handler using v144 wrap_render_handler! macro
        let render_handler = KiBrowserRenderHandlerImpl::new(
            tab_id,
            frame_buffer.clone(),
            frame_size.clone(),
            viewport_size.clone(),
        );

        // Create life span handler with popup_tx for popup interception
        let life_span_handler = KiBrowserLifeSpanHandlerImpl::new(
            tab_id,
            tabs.clone(),
            browser_created.clone(),
            None, // popup_tx set later if needed
        );

        // Create load handler
        let load_handler = KiBrowserLoadHandlerImpl::new(
            tab_id,
            tabs.clone(),
            stealth_config.clone(),
        );

        // Create display handler (captures console.log for JS result communication)
        let display_handler = KiBrowserDisplayHandlerImpl::new(tab_id, tabs.clone());

        // Create client using v144 API
        let mut client = KiBrowserClient::new(
            tab_id,
            tabs.clone(),
            stealth_config.clone(),
            render_handler,
            life_span_handler,
            load_handler,
            display_handler,
        );

        // Browser settings
        let browser_settings = BrowserSettings {
            windowless_frame_rate: DEFAULT_FRAME_RATE,
            ..Default::default()
        };

        // Window info for OSR (off-screen rendering)
        let window_info = WindowInfo {
            bounds: Rect {
                x: 0,
                y: 0,
                width: viewport_dims.0 as i32,
                height: viewport_dims.1 as i32,
            },
            windowless_rendering_enabled: 1,
            ..Default::default()
        };

        // Create browser using v144 API
        let url_string = CefString::from(url);
        let result = cef::browser_host_create_browser(
            Some(&window_info),
            Some(&mut client),
            Some(&url_string),
            Some(&browser_settings),
            None,
            None,
        );

        if result == 0 {
            return Err(anyhow!("Failed to create CEF browser"));
        }

        // Store tab BEFORE browser creation (browser will be set in on_after_created)
        let cef_tab = CefTab::new(tab_id, url.to_string(), frame_buffer, frame_size, viewport_size);
        tabs.write().insert(tab_id, cef_tab);

        // Wait for browser to be created (callback will be triggered)
        let start = std::time::Instant::now();
        while !browser_created.load(Ordering::SeqCst) {
            if start.elapsed() > std::time::Duration::from_secs(10) {
                // Remove the tab if browser creation failed
                tabs.write().remove(&tab_id);
                return Err(anyhow!("Timeout waiting for browser creation"));
            }
            std::thread::sleep(std::time::Duration::from_millis(10));
            cef::do_message_loop_work();
        }

        browser_id_counter.fetch_add(1, Ordering::SeqCst);

        info!("Browser created for tab {} with URL: {}", tab_id, url);
        Ok(())
    }

    /// Closes a browser instance internally on the CEF thread.
    fn close_browser_internal(
        tab_id: Uuid,
        tabs: Arc<RwLock<HashMap<Uuid, CefTab>>>,
    ) -> Result<()> {
        let tab = {
            let mut tabs_guard = tabs.write();
            tabs_guard.remove(&tab_id)
        };

        if let Some(tab) = tab {
            // Close the browser
            if let Some(ref browser) = tab.browser {
                if let Some(host) = browser.host() {
                    host.close_browser(1);
                }
            }
            info!("Browser closed for tab {}", tab_id);
            Ok(())
        } else {
            Err(anyhow!("Tab not found: {}", tab_id))
        }
    }

    /// Navigates a tab to a URL internally on the CEF thread.
    /// IMPORTANT: Must NOT hold the tabs RwLock while calling CEF methods,
    /// because CEF may fire callbacks (e.g. on_loading_state_change) that
    /// need a write lock on tabs — causing a deadlock on the same thread.
    fn navigate_internal(
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
    fn go_back_internal(
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
    fn go_forward_internal(
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
    fn resize_viewport_internal(
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
    fn execute_js_internal(
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
    fn execute_js_with_result_internal(
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
    fn screenshot_internal(
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
        let image_data = Self::convert_frame_to_image(
            &frame_buffer,
            width,
            height,
            options.format,
            options.quality,
        )?;

        let data = base64::Engine::encode(&base64::engine::general_purpose::STANDARD, &image_data);

        Ok(Screenshot::new(data, options.format, width, height, 1.0))
    }

    // ========================================================================
    // Internal Input Methods (CEF Thread)
    // ========================================================================

    /// Sends a mouse move event internally on the CEF thread.
    fn mouse_move_internal(
        tab_id: Uuid,
        x: i32,
        y: i32,
        tabs: Arc<RwLock<HashMap<Uuid, CefTab>>>,
    ) -> Result<()> {
        // Clone browser ref and release read lock BEFORE calling CEF methods
        // (CEF callbacks may need write lock on same thread → deadlock prevention)
        let browser = {
            let tabs_guard = tabs.read();
            let tab = tabs_guard
                .get(&tab_id)
                .ok_or_else(|| anyhow!("Tab not found: {}", tab_id))?;
            tab.browser.clone()
                .ok_or_else(|| anyhow!("Browser not initialized for tab: {}", tab_id))?
        }; // Read lock released here.

        if let Some(host) = browser.host() {
            let event = cef::MouseEvent {
                x,
                y,
                modifiers: 0u32,
            };
            host.send_mouse_move_event(Some(&event), 0);
            trace!("Mouse move sent to tab {}: ({}, {})", tab_id, x, y);
            Ok(())
        } else {
            Err(anyhow!("No browser host for tab: {}", tab_id))
        }
    }

    /// Sends a mouse click event internally on the CEF thread.
    fn mouse_click_internal(
        tab_id: Uuid,
        x: i32,
        y: i32,
        button: i32,
        click_count: i32,
        tabs: Arc<RwLock<HashMap<Uuid, CefTab>>>,
    ) -> Result<()> {
        // Clone browser ref and release read lock BEFORE calling CEF methods
        // (CEF callbacks may need write lock on same thread → deadlock prevention)
        let browser = {
            let tabs_guard = tabs.read();
            let tab = tabs_guard
                .get(&tab_id)
                .ok_or_else(|| anyhow!("Tab not found: {}", tab_id))?;
            tab.browser.clone()
                .ok_or_else(|| anyhow!("Browser not initialized for tab: {}", tab_id))?
        }; // Read lock released here.

        if let Some(host) = browser.host() {
            let event = cef::MouseEvent {
                x,
                y,
                modifiers: 0u32,
            };

            // Decode click_count: positive = down, negative = up
            let mouse_up = if click_count < 0 { 1 } else { 0 };
            let actual_count = click_count.abs();

            let button_type = match button {
                0 => cef::MouseButtonType::LEFT,
                1 => cef::MouseButtonType::MIDDLE,
                2 => cef::MouseButtonType::RIGHT,
                _ => cef::MouseButtonType::LEFT,
            };

            host.send_mouse_click_event(Some(&event), button_type, mouse_up, actual_count);
            trace!(
                "Mouse click sent to tab {}: ({}, {}), button={}, up={}, count={}",
                tab_id, x, y, button, mouse_up, actual_count
            );
            Ok(())
        } else {
            Err(anyhow!("No browser host for tab: {}", tab_id))
        }
    }

    /// Sends a mouse wheel event internally on the CEF thread.
    fn mouse_wheel_internal(
        tab_id: Uuid,
        x: i32,
        y: i32,
        delta_x: i32,
        delta_y: i32,
        tabs: Arc<RwLock<HashMap<Uuid, CefTab>>>,
    ) -> Result<()> {
        // Clone browser ref and release read lock BEFORE calling CEF methods
        // (CEF callbacks may need write lock on same thread → deadlock prevention)
        let browser = {
            let tabs_guard = tabs.read();
            let tab = tabs_guard
                .get(&tab_id)
                .ok_or_else(|| anyhow!("Tab not found: {}", tab_id))?;
            tab.browser.clone()
                .ok_or_else(|| anyhow!("Browser not initialized for tab: {}", tab_id))?
        }; // Read lock released here.

        if let Some(host) = browser.host() {
            let event = cef::MouseEvent {
                x,
                y,
                modifiers: 0u32,
            };
            host.send_mouse_wheel_event(Some(&event), delta_x, delta_y);
            trace!(
                "Mouse wheel sent to tab {}: ({}, {}), delta=({}, {})",
                tab_id, x, y, delta_x, delta_y
            );
            Ok(())
        } else {
            Err(anyhow!("No browser host for tab: {}", tab_id))
        }
    }

    /// Simulates a drag-and-drop by sending mousedown, mousemoves, mouseup.
    #[allow(clippy::too_many_arguments)]
    fn drag_internal(
        tab_id: Uuid,
        from_x: i32,
        from_y: i32,
        to_x: i32,
        to_y: i32,
        steps: u32,
        duration_ms: u64,
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

        if let Some(host) = browser.host() {
            let step_delay = if steps > 0 {
                std::time::Duration::from_millis(duration_ms / steps as u64)
            } else {
                std::time::Duration::from_millis(10)
            };

            // 1. Move to start position
            let start_event = cef::MouseEvent { x: from_x, y: from_y, modifiers: 0u32 };
            host.send_mouse_move_event(Some(&start_event), 0);
            std::thread::sleep(std::time::Duration::from_millis(50));

            // 2. Mouse down at start
            host.send_mouse_click_event(Some(&start_event), cef::MouseButtonType::LEFT, 0, 1);
            std::thread::sleep(std::time::Duration::from_millis(100));

            // 3. Intermediate moves (with left button held = modifier bit 5)
            let left_button_down: u32 = 1 << 5; // EVENTFLAG_LEFT_MOUSE_BUTTON
            let actual_steps = steps.max(1);
            for i in 1..=actual_steps {
                let t = i as f64 / actual_steps as f64;
                let cx = from_x + ((to_x - from_x) as f64 * t) as i32;
                let cy = from_y + ((to_y - from_y) as f64 * t) as i32;
                let move_event = cef::MouseEvent { x: cx, y: cy, modifiers: left_button_down };
                host.send_mouse_move_event(Some(&move_event), 0);
                std::thread::sleep(step_delay);
            }

            // 4. Mouse up at end
            std::thread::sleep(std::time::Duration::from_millis(50));
            let end_event = cef::MouseEvent { x: to_x, y: to_y, modifiers: left_button_down };
            host.send_mouse_click_event(Some(&end_event), cef::MouseButtonType::LEFT, 1, 1);

            info!("Drag on tab {}: ({},{}) → ({},{}) in {} steps", tab_id, from_x, from_y, to_x, to_y, actual_steps);
            Ok(())
        } else {
            Err(anyhow!("No browser host for tab: {}", tab_id))
        }
    }

    /// Sends a keyboard event internally on the CEF thread.
    fn key_event_internal(
        tab_id: Uuid,
        event_type: i32,
        modifiers: u32,
        windows_key_code: i32,
        character: u16,
        tabs: Arc<RwLock<HashMap<Uuid, CefTab>>>,
    ) -> Result<()> {
        // Clone browser ref and release read lock BEFORE calling CEF methods
        // (CEF callbacks may need write lock on same thread → deadlock prevention)
        let browser = {
            let tabs_guard = tabs.read();
            let tab = tabs_guard
                .get(&tab_id)
                .ok_or_else(|| anyhow!("Tab not found: {}", tab_id))?;
            tab.browser.clone()
                .ok_or_else(|| anyhow!("Browser not initialized for tab: {}", tab_id))?
        }; // Read lock released here.

        if let Some(host) = browser.host() {
            let key_event_type = match event_type {
                0 => cef::KeyEventType::RAWKEYDOWN,
                1 => cef::KeyEventType::KEYDOWN,
                2 => cef::KeyEventType::KEYUP,
                3 => cef::KeyEventType::CHAR,
                _ => cef::KeyEventType::KEYDOWN,
            };

            // Use modifiers directly as u32
            let key_modifiers = modifiers;

            let event = cef::KeyEvent {
                size: std::mem::size_of::<cef::KeyEvent>(),
                type_: key_event_type,
                modifiers: key_modifiers,
                windows_key_code,
                native_key_code: 0,
                is_system_key: 0,
                character,
                unmodified_character: character,
                focus_on_editable_field: 0,
            };

            host.send_key_event(Some(&event));
            trace!(
                "Key event sent to tab {}: type={}, code={}, char={}",
                tab_id, event_type, windows_key_code, character
            );
            Ok(())
        } else {
            Err(anyhow!("No browser host for tab: {}", tab_id))
        }
    }

    /// Maps a character to its Windows Virtual Key code for KEYDOWN/KEYUP events.
    /// Without this, characters like '.' (char code 46 = VK_DELETE!) would be
    /// misinterpreted as control keys.
    fn char_to_vk_code(c: char) -> (i32, u32) {
        // Returns (vk_code, modifiers). Shift = 1 << 1 = 2 (EVENTFLAG_SHIFT_DOWN).
        const SHIFT: u32 = 2;
        match c {
            'a'..='z' => ((c as u8 - b'a' + b'A') as i32, 0),
            'A'..='Z' => (c as i32, SHIFT),
            '0'..='9' => (c as i32, 0),
            ' ' => (0x20, 0),        // VK_SPACE
            '\r' | '\n' => (0x0D, 0), // VK_RETURN
            '\t' => (0x09, 0),       // VK_TAB
            '.' => (190, 0),         // VK_OEM_PERIOD  (NOT 46 = VK_DELETE!)
            ',' => (188, 0),         // VK_OEM_COMMA
            '-' => (189, 0),         // VK_OEM_MINUS
            '=' => (187, 0),         // VK_OEM_PLUS (unshifted)
            ';' => (186, 0),         // VK_OEM_1
            '\'' => (222, 0),        // VK_OEM_7
            '/' => (191, 0),         // VK_OEM_2
            '\\' => (220, 0),        // VK_OEM_5
            '[' => (219, 0),         // VK_OEM_4
            ']' => (221, 0),         // VK_OEM_6
            '`' => (192, 0),         // VK_OEM_3
            // Shifted variants
            '!' => (0x31, SHIFT),    // Shift+1
            '@' => (0x32, SHIFT),    // Shift+2
            '#' => (0x33, SHIFT),    // Shift+3
            '$' => (0x34, SHIFT),    // Shift+4
            '%' => (0x35, SHIFT),    // Shift+5
            '^' => (0x36, SHIFT),    // Shift+6
            '&' => (0x37, SHIFT),    // Shift+7
            '*' => (0x38, SHIFT),    // Shift+8
            '(' => (0x39, SHIFT),    // Shift+9
            ')' => (0x30, SHIFT),    // Shift+0
            '_' => (189, SHIFT),     // Shift+minus
            '+' => (187, SHIFT),     // Shift+=
            ':' => (186, SHIFT),     // Shift+;
            '"' => (222, SHIFT),     // Shift+'
            '?' => (191, SHIFT),     // Shift+/
            '>' => (190, SHIFT),     // Shift+.
            '<' => (188, SHIFT),     // Shift+,
            '|' => (220, SHIFT),     // Shift+backslash
            '{' => (219, SHIFT),     // Shift+[
            '}' => (221, SHIFT),     // Shift+]
            '~' => (192, SHIFT),     // Shift+`
            // Fallback: use char code directly (works for basic ASCII)
            _ => (c as i32, 0),
        }
    }

    /// Types text by sending character events internally on the CEF thread.
    fn type_text_internal(
        tab_id: Uuid,
        text: &str,
        tabs: Arc<RwLock<HashMap<Uuid, CefTab>>>,
    ) -> Result<()> {
        // Clone browser ref and release read lock BEFORE calling CEF methods
        // (CEF callbacks may need write lock on same thread → deadlock prevention)
        let browser = {
            let tabs_guard = tabs.read();
            let tab = tabs_guard
                .get(&tab_id)
                .ok_or_else(|| anyhow!("Tab not found: {}", tab_id))?;
            tab.browser.clone()
                .ok_or_else(|| anyhow!("Browser not initialized for tab: {}", tab_id))?
        }; // Read lock released here.

        if let Some(host) = browser.host() {
            for c in text.chars() {
                let char_code = c as u16;
                let (vk_code, modifiers) = Self::char_to_vk_code(c);

                // Send KeyDown (uses VK code, not char code!)
                let key_down = cef::KeyEvent {
                    size: std::mem::size_of::<cef::KeyEvent>(),
                    type_: cef::KeyEventType::KEYDOWN,
                    modifiers,
                    windows_key_code: vk_code,
                    native_key_code: 0,
                    is_system_key: 0,
                    character: char_code,
                    unmodified_character: char_code,
                    focus_on_editable_field: 0,
                };
                host.send_key_event(Some(&key_down));

                // Send Char event (uses char code — this is what produces text input)
                let char_event = cef::KeyEvent {
                    size: std::mem::size_of::<cef::KeyEvent>(),
                    type_: cef::KeyEventType::CHAR,
                    modifiers,
                    windows_key_code: char_code as i32,
                    native_key_code: 0,
                    is_system_key: 0,
                    character: char_code,
                    unmodified_character: char_code,
                    focus_on_editable_field: 0,
                };
                host.send_key_event(Some(&char_event));

                // Send KeyUp (uses VK code, not char code!)
                let key_up = cef::KeyEvent {
                    size: std::mem::size_of::<cef::KeyEvent>(),
                    type_: cef::KeyEventType::KEYUP,
                    modifiers,
                    windows_key_code: vk_code,
                    native_key_code: 0,
                    is_system_key: 0,
                    character: char_code,
                    unmodified_character: char_code,
                    focus_on_editable_field: 0,
                };
                host.send_key_event(Some(&key_up));
            }

            debug!("Typed text on tab {}: {} chars", tab_id, text.len());
            Ok(())
        } else {
            Err(anyhow!("No browser host for tab: {}", tab_id))
        }
    }

    /// Converts raw BGRA frame buffer to encoded image.
    fn convert_frame_to_image(
        buffer: &[u8],
        width: u32,
        height: u32,
        format: ScreenshotFormat,
        quality: u8,
    ) -> Result<Vec<u8>> {
        use image::{ImageBuffer, ImageOutputFormat, Rgba};

        // Create image from BGRA buffer
        let mut img: ImageBuffer<Rgba<u8>, Vec<u8>> =
            ImageBuffer::new(width, height);

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

    // ========================================================================
    // Public Extended API
    // ========================================================================

    /// Navigates a tab to the specified URL.
    ///
    /// # Arguments
    ///
    /// * `tab_id` - The UUID of the tab to navigate
    /// * `url` - The URL to navigate to
    ///
    /// # Returns
    ///
    /// A Result indicating success or an error.
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
    /// # Arguments
    ///
    /// * `tab_id` - The UUID of the tab
    /// * `script` - The JavaScript code to execute
    ///
    /// # Returns
    ///
    /// A Result containing the optional result string, or an error.
    ///
    /// # Note
    ///
    /// CEF doesn't provide synchronous JavaScript return values.
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
    ///
    /// # Arguments
    ///
    /// * `tab_id` - The UUID of the tab
    /// * `script` - The JavaScript code to execute (must return a value)
    ///
    /// # Returns
    ///
    /// A Result containing the optional JSON-serialised result string, or an error.
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
    ///
    /// # Arguments
    ///
    /// * `tab_id` - The UUID of the tab to capture
    /// * `options` - Screenshot configuration options
    ///
    /// # Returns
    ///
    /// A Result containing the Screenshot or an error.
    pub async fn screenshot(&self, tab_id: Uuid, options: ScreenshotOptions) -> Result<Screenshot> {
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

    /// Returns the stealth configuration.
    pub fn stealth_config(&self) -> &StealthConfig {
        &self.stealth_config
    }

    /// Returns the frame buffer and size Arcs for a tab (for GUI rendering).
    pub fn get_tab_frame_buffer(&self, tab_id: Uuid) -> Option<TabFrameBuffer> {
        let tabs = self.tabs.read();
        tabs.get(&tab_id).map(|tab| {
            (tab.frame_buffer.clone(), tab.frame_size.clone())
        })
    }

    /// Creates an event sender for a specific tab.
    ///
    /// This can be used to create a `CefInputHandler` for input simulation.
    pub fn create_event_sender(&self, tab_id: Uuid) -> CefBrowserEventSender {
        CefBrowserEventSender::new(
            tab_id,
            self.command_tx.clone(),
        )
    }

    // ========================================================================
    // Synchronous GUI Methods (fire-and-forget via try_send)
    // ========================================================================

    /// Returns all tabs synchronously (no async needed, reads directly from shared state).
    pub fn get_tabs_sync(&self) -> Vec<Tab> {
        if !self.is_running.load(Ordering::SeqCst) {
            return Vec::new();
        }
        let tabs = self.tabs.read();
        tabs.values().map(|t| t.to_tab()).collect()
    }

    /// Creates a tab without blocking. Returns the pre-generated tab_id.
    /// The tab will appear in get_tabs_sync() once CEF processes the command.
    pub fn send_create_tab(&self, url: &str) -> Uuid {
        let tab_id = Uuid::new_v4();
        let (response_tx, _) = oneshot::channel();
        let _ = self.command_tx.send(CefCommand::CreateBrowser {
            url: url.to_string(),
            tab_id,
            response: response_tx,
        });
        tab_id
    }

    /// Closes a tab without blocking.
    pub fn send_close_tab(&self, tab_id: Uuid) {
        let (response_tx, _) = oneshot::channel();
        let _ = self.command_tx.send(CefCommand::CloseBrowser {
            tab_id,
            response: response_tx,
        });
    }

    /// Navigates a tab without blocking.
    pub fn send_navigate(&self, tab_id: Uuid, url: &str) {
        let (response_tx, _) = oneshot::channel();
        let _ = self.command_tx.send(CefCommand::Navigate {
            tab_id,
            url: url.to_string(),
            response: response_tx,
        });
    }

    /// Sends a mouse move event without blocking.
    pub fn send_mouse_move(&self, tab_id: Uuid, x: i32, y: i32) {
        let (response_tx, _) = oneshot::channel();
        let _ = self.command_tx.send(CefCommand::MouseMove {
            tab_id,
            x,
            y,
            response: response_tx,
        });
    }

    /// Sends a mouse click (down + up) without blocking.
    pub fn send_mouse_click(&self, tab_id: Uuid, x: i32, y: i32, button: i32) {
        // Mouse down
        let (response_tx, _) = oneshot::channel();
        let _ = self.command_tx.send(CefCommand::MouseClick {
            tab_id,
            x,
            y,
            button,
            click_count: 1,
            response: response_tx,
        });
        // Mouse up (CEF thread processes commands in order, no delay needed)
        let (response_tx, _) = oneshot::channel();
        let _ = self.command_tx.send(CefCommand::MouseClick {
            tab_id,
            x,
            y,
            button,
            click_count: -1,
            response: response_tx,
        });
    }

    /// Sends a mouse wheel event without blocking.
    pub fn send_mouse_wheel(&self, tab_id: Uuid, x: i32, y: i32, delta_x: i32, delta_y: i32) {
        let (response_tx, _) = oneshot::channel();
        let _ = self.command_tx.send(CefCommand::MouseWheel {
            tab_id,
            x,
            y,
            delta_x,
            delta_y,
            response: response_tx,
        });
    }

    /// Sends a key event without blocking.
    pub fn send_key_event(&self, tab_id: Uuid, event_type: i32, modifiers: u32, windows_key_code: i32, character: u16) {
        let (response_tx, _) = oneshot::channel();
        let _ = self.command_tx.send(CefCommand::KeyEvent {
            tab_id,
            event_type,
            modifiers,
            windows_key_code,
            character,
            response: response_tx,
        });
    }

    /// Sends a type text command without blocking.
    pub fn send_type_text(&self, tab_id: Uuid, text: &str) {
        let (response_tx, _) = oneshot::channel();
        let _ = self.command_tx.send(CefCommand::TypeText {
            tab_id,
            text: text.to_string(),
            response: response_tx,
        });
    }

    /// Executes JavaScript in a tab without blocking (fire-and-forget).
    pub fn send_execute_js(&self, tab_id: Uuid, script: &str) {
        let (response_tx, _) = oneshot::channel();
        let _ = self.command_tx.send(CefCommand::ExecuteJs {
            tab_id,
            script: script.to_string(),
            response: response_tx,
        });
    }

    /// Sends shutdown command without blocking.
    pub fn send_shutdown(&self) {
        self.is_running.store(false, Ordering::SeqCst);
        let (response_tx, _) = oneshot::channel();
        let _ = self.command_tx.send(CefCommand::Shutdown {
            response: response_tx,
        });
    }

    /// Navigates the active tab back in history without blocking (fire-and-forget).
    pub fn send_go_back(&self, tab_id: Uuid) {
        let (response_tx, _) = oneshot::channel();
        let _ = self.command_tx.send(CefCommand::GoBack {
            tab_id,
            response: response_tx,
        });
    }

    /// Navigates the active tab forward in history without blocking (fire-and-forget).
    pub fn send_go_forward(&self, tab_id: Uuid) {
        let (response_tx, _) = oneshot::channel();
        let _ = self.command_tx.send(CefCommand::GoForward {
            tab_id,
            response: response_tx,
        });
    }

    /// Resizes the CEF viewport without blocking (fire-and-forget).
    ///
    /// Notifies CEF that the viewport dimensions changed so it re-renders
    /// at the new size on the next paint cycle.
    pub fn send_resize_viewport(&self, tab_id: Uuid, width: u32, height: u32) {
        let (response_tx, _) = oneshot::channel();
        let _ = self.command_tx.send(CefCommand::ResizeViewport {
            tab_id,
            width,
            height,
            response: response_tx,
        });
    }

    /// Returns whether the given tab can navigate back in history.
    pub fn can_go_back(&self, tab_id: Uuid) -> bool {
        let tabs = self.tabs.read();
        tabs.get(&tab_id)
            .map(|t| t.can_go_back.load(Ordering::SeqCst))
            .unwrap_or(false)
    }

    /// Returns whether the given tab can navigate forward in history.
    pub fn can_go_forward(&self, tab_id: Uuid) -> bool {
        let tabs = self.tabs.read();
        tabs.get(&tab_id)
            .map(|t| t.can_go_forward.load(Ordering::SeqCst))
            .unwrap_or(false)
    }

    // ========================================================================
    // Input Operations (async, for REST API use)
    // ========================================================================

    /// Clicks at the specified coordinates in a tab.
    ///
    /// # Arguments
    ///
    /// * `tab_id` - The UUID of the tab
    /// * `x` - X coordinate
    /// * `y` - Y coordinate
    /// * `button` - Mouse button (0 = left, 1 = middle, 2 = right)
    pub async fn click(&self, tab_id: Uuid, x: i32, y: i32, button: i32) -> Result<()> {
        if !self.is_running.load(Ordering::SeqCst) {
            return Err(anyhow!("Browser engine is not running"));
        }

        // Mouse down
        let (response_tx, response_rx) = oneshot::channel();
        self.command_tx
            .send(CefCommand::MouseClick {
                tab_id,
                x,
                y,
                button,
                click_count: 1, // Positive = down
                response: response_tx,
            })
            .map_err(|_| anyhow!("Failed to send mouse down command"))?;
        response_rx.await.context("Failed to receive mouse down response")??;

        // Small delay between down and up
        tokio::time::sleep(tokio::time::Duration::from_millis(50)).await;

        // Mouse up
        let (response_tx, response_rx) = oneshot::channel();
        self.command_tx
            .send(CefCommand::MouseClick {
                tab_id,
                x,
                y,
                button,
                click_count: -1, // Negative = up
                response: response_tx,
            })
            .map_err(|_| anyhow!("Failed to send mouse up command"))?;
        response_rx.await.context("Failed to receive mouse up response")?
    }

    /// Types text in the currently focused element of a tab.
    ///
    /// # Arguments
    ///
    /// * `tab_id` - The UUID of the tab
    /// * `text` - The text to type
    pub async fn type_text(&self, tab_id: Uuid, text: &str) -> Result<()> {
        if !self.is_running.load(Ordering::SeqCst) {
            return Err(anyhow!("Browser engine is not running"));
        }

        let (response_tx, response_rx) = oneshot::channel();
        self.command_tx
            .send(CefCommand::TypeText {
                tab_id,
                text: text.to_string(),
                response: response_tx,
            })
            .map_err(|_| anyhow!("Failed to send type text command"))?;

        response_rx.await.context("Failed to receive type text response")?
    }

    /// Scrolls at the specified position in a tab.
    ///
    /// # Arguments
    ///
    /// * `tab_id` - The UUID of the tab
    /// * `x` - X coordinate for scroll location
    /// * `y` - Y coordinate for scroll location
    /// * `delta_x` - Horizontal scroll amount
    /// * `delta_y` - Vertical scroll amount
    pub async fn scroll(&self, tab_id: Uuid, x: i32, y: i32, delta_x: i32, delta_y: i32) -> Result<()> {
        if !self.is_running.load(Ordering::SeqCst) {
            return Err(anyhow!("Browser engine is not running"));
        }

        let (response_tx, response_rx) = oneshot::channel();
        self.command_tx
            .send(CefCommand::MouseWheel {
                tab_id,
                x,
                y,
                delta_x,
                delta_y,
                response: response_tx,
            })
            .map_err(|_| anyhow!("Failed to send scroll command"))?;

        response_rx.await.context("Failed to receive scroll response")?
    }

    /// Moves the mouse to the specified coordinates in a tab.
    ///
    /// # Arguments
    ///
    /// * `tab_id` - The UUID of the tab
    /// * `x` - X coordinate
    /// * `y` - Y coordinate
    pub async fn mouse_move(&self, tab_id: Uuid, x: i32, y: i32) -> Result<()> {
        if !self.is_running.load(Ordering::SeqCst) {
            return Err(anyhow!("Browser engine is not running"));
        }

        let (response_tx, response_rx) = oneshot::channel();
        self.command_tx
            .send(CefCommand::MouseMove {
                tab_id,
                x,
                y,
                response: response_tx,
            })
            .map_err(|_| anyhow!("Failed to send mouse move command"))?;

        response_rx.await.context("Failed to receive mouse move response")?
    }

    /// Performs a drag operation from one point to another.
    ///
    /// # Arguments
    ///
    /// * `tab_id` - The UUID of the tab
    /// * `from_x` - Start X coordinate
    /// * `from_y` - Start Y coordinate
    /// * `to_x` - End X coordinate
    /// * `to_y` - End Y coordinate
    /// * `steps` - Number of intermediate move steps
    /// * `duration_ms` - Total duration in milliseconds
    #[allow(clippy::too_many_arguments)]
    pub async fn drag(&self, tab_id: Uuid, from_x: i32, from_y: i32, to_x: i32, to_y: i32, steps: u32, duration_ms: u64) -> Result<()> {
        if !self.is_running.load(Ordering::SeqCst) {
            return Err(anyhow!("Browser engine is not running"));
        }

        let (response_tx, response_rx) = oneshot::channel();
        self.command_tx
            .send(CefCommand::Drag {
                tab_id,
                from_x,
                from_y,
                to_x,
                to_y,
                steps,
                duration_ms,
                response: response_tx,
            })
            .map_err(|_| anyhow!("Failed to send drag command"))?;

        response_rx.await.context("Failed to receive drag response")?
    }

    /// Waits for a tab to be ready for interaction.
    ///
    /// # Arguments
    ///
    /// * `tab_id` - The UUID of the tab to wait for
    /// * `timeout_ms` - Maximum time to wait in milliseconds
    ///
    /// # Returns
    ///
    /// A Result indicating success or a timeout error.
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

// ============================================================================
// Tests
// ============================================================================

#[cfg(all(test, feature = "cef-browser"))]
mod tests {
    use super::*;

    #[test]
    fn test_cef_tab_creation() {
        // Create a mock browser for testing
        // Note: Full CEF tests require CEF runtime
        let tab_id = Uuid::new_v4();
        let tab = Tab::new("https://example.com".to_string());
        assert!(!tab.url.is_empty());
    }

    #[test]
    fn test_stealth_config_validation() {
        let config = StealthConfig::default();
        assert!(config.validate().is_ok());
        assert!(!config.navigator.webdriver, "webdriver must be false");
    }

    #[tokio::test]
    #[ignore = "Requires CEF runtime"]
    async fn test_cef_engine_lifecycle() {
        let config = BrowserConfig::default().headless(true);
        let engine = CefBrowserEngine::new(config).await.unwrap();

        assert!(engine.is_running().await);

        let tab = engine.create_tab("about:blank").await.unwrap();
        assert_eq!(tab.url, "about:blank");

        engine.close_tab(tab.id).await.unwrap();
        engine.shutdown().await.unwrap();

        assert!(!engine.is_running().await);
    }
}

// ============================================================================
// Feature-gated stub for when CEF is not enabled
// ============================================================================

#[cfg(not(feature = "cef-browser"))]
pub struct CefBrowserEngine;

#[cfg(not(feature = "cef-browser"))]
impl CefBrowserEngine {
    pub fn new(_config: crate::browser::engine::BrowserConfig) -> anyhow::Result<Self> {
        Err(anyhow::anyhow!(
            "CEF browser engine is not available. Enable the 'cef-browser' feature."
        ))
    }
}
