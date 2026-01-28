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
//! use ki_browser_standalone::browser::{BrowserConfig, cef_engine::CefBrowserEngine};
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
    // CEF v144 API - uses attribute macros and trait implementations
    App, Browser, BrowserSettings,
    CefString, Client, Frame,
    LifeSpanHandler, LoadHandler, RenderHandler,
    PaintElementType, TransitionType,
    Rect, ScreenInfo, WindowInfo, Settings, LogSeverity,
    Errorcode, MainArgs,
    // Traits for handler implementations
    ImplApp, ImplRenderHandler, ImplLoadHandler, ImplLifeSpanHandler,
    ImplBrowser, ImplBrowserHost, ImplFrame,
    // Traits for wrapper implementations
    WrapApp, WrapClient, WrapRenderHandler, WrapLoadHandler, WrapLifeSpanHandler,
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
use tokio::sync::{mpsc, oneshot, Mutex};
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
}

#[cfg(feature = "cef-browser")]
impl CefTab {
    fn new(id: Uuid, url: String, frame_buffer: Arc<RwLock<Vec<u8>>>, frame_size: Arc<RwLock<(u32, u32)>>) -> Self {
        Self {
            id,
            browser: None,
            url,
            title: String::new(),
            status: TabStatus::Loading,
            frame_buffer,
            frame_size,
            is_ready: AtomicBool::new(false),
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
enum CefCommand {
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
    /// Command sender for the CEF message loop.
    command_tx: mpsc::Sender<CefCommand>,
    /// Runtime handle for blocking send operations.
    runtime: tokio::runtime::Handle,
}

#[cfg(feature = "cef-browser")]
impl CefBrowserEventSender {
    /// Creates a new event sender for a specific tab.
    pub fn new(tab_id: Uuid, command_tx: mpsc::Sender<CefCommand>, runtime: tokio::runtime::Handle) -> Self {
        Self {
            tab_id,
            command_tx,
            runtime,
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
        let _ = self.runtime.block_on(self.command_tx.send(cmd));
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
        let _ = self.runtime.block_on(self.command_tx.send(cmd));
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
        let _ = self.runtime.block_on(self.command_tx.send(cmd));
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
        let _ = self.runtime.block_on(self.command_tx.send(cmd));
    }
}

// ============================================================================
// CEF Callbacks Implementation
// ============================================================================

/// Application handler for CEF lifecycle using v144 API.
#[cfg(feature = "cef-browser")]
#[cef::wrap_app]
struct KiBrowserApp {
    stealth_config: Arc<StealthConfig>,
}

#[cfg(feature = "cef-browser")]
impl WrapApp for KiBrowserApp {
    fn on_before_command_line_processing(
        &mut self,
        command_line: Option<&mut cef::CommandLine>,
    ) {
        if let Some(cmd) = command_line {
            // Add arguments for stealth mode
            cmd.append_switch_with_value(&CefString::from("disable-blink-features"), &CefString::from("AutomationControlled"));
            cmd.append_switch(&CefString::from("disable-infobars"));
            cmd.append_switch(&CefString::from("disable-extensions"));
            cmd.append_switch(&CefString::from("no-first-run"));
            cmd.append_switch(&CefString::from("no-default-browser-check"));

            // Disable GPU in headless mode for stability
            cmd.append_switch(&CefString::from("disable-gpu"));
            cmd.append_switch(&CefString::from("disable-gpu-compositing"));

            debug!("CEF command line configured for stealth mode");
        }
    }
}

/// Client handler for browser events using v144 API.
#[cfg(feature = "cef-browser")]
#[cef::wrap_client]
struct KiBrowserClient {
    tab_id: Uuid,
    tabs: Arc<RwLock<HashMap<Uuid, CefTab>>>,
    stealth_config: Arc<StealthConfig>,
    render_handler: RenderHandler,
    life_span_handler: LifeSpanHandler,
    load_handler: LoadHandler,
}

#[cfg(feature = "cef-browser")]
impl WrapClient for KiBrowserClient {
    fn get_render_handler(&mut self) -> Option<RenderHandler> {
        Some(self.render_handler.clone())
    }

    fn get_life_span_handler(&mut self) -> Option<LifeSpanHandler> {
        Some(self.life_span_handler.clone())
    }

    fn get_load_handler(&mut self) -> Option<LoadHandler> {
        Some(self.load_handler.clone())
    }
}

/// Render handler for off-screen rendering using v144 API.
#[cfg(feature = "cef-browser")]
#[cef::wrap_render_handler]
struct KiBrowserRenderHandlerImpl {
    tab_id: Uuid,
    frame_buffer: Arc<RwLock<Vec<u8>>>,
    frame_size: Arc<RwLock<(u32, u32)>>,
    viewport_size: (u32, u32),
}

#[cfg(feature = "cef-browser")]
impl WrapRenderHandler for KiBrowserRenderHandlerImpl {
    fn get_view_rect(&mut self, _browser: Option<&mut Browser>, rect: Option<&mut Rect>) -> i32 {
        if let Some(r) = rect {
            r.x = 0;
            r.y = 0;
            r.width = self.viewport_size.0 as i32;
            r.height = self.viewport_size.1 as i32;
        }
        1 // Return true
    }

    fn get_screen_info(&mut self, _browser: Option<&mut Browser>, screen_info: Option<&mut ScreenInfo>) -> i32 {
        if let Some(info) = screen_info {
            info.device_scale_factor = 1.0;
            info.depth = 32;
            info.depth_per_component = 8;
            info.is_monochrome = 0;
            info.rect = Rect {
                x: 0,
                y: 0,
                width: self.viewport_size.0 as i32,
                height: self.viewport_size.1 as i32,
            };
            info.available_rect = Rect {
                x: 0,
                y: 0,
                width: self.viewport_size.0 as i32,
                height: self.viewport_size.1 as i32,
            };
        }
        1 // Return true
    }

    fn on_paint(
        &mut self,
        _browser: Option<&mut Browser>,
        element_type: PaintElementType,
        _dirty_rects: &[Rect],
        buffer: *const u8,
        width: i32,
        height: i32,
    ) {
        if element_type == PaintElementType::View {
            // Store the frame buffer for screenshot capture
            let buffer_size = (width * height * 4) as usize;
            let buffer_slice = unsafe { std::slice::from_raw_parts(buffer, buffer_size) };

            let mut fb = self.frame_buffer.write();
            fb.clear();
            fb.extend_from_slice(buffer_slice);

            let mut size = self.frame_size.write();
            *size = (width as u32, height as u32);

            trace!(
                "Frame painted for tab {}: {}x{}, {} bytes",
                self.tab_id,
                width,
                height,
                buffer_size
            );
        }
    }
}

/// Life span handler for tab lifecycle events using v144 API.
#[cfg(feature = "cef-browser")]
#[cef::wrap_life_span_handler]
struct KiBrowserLifeSpanHandlerImpl {
    tab_id: Uuid,
    tabs: Arc<RwLock<HashMap<Uuid, CefTab>>>,
    browser_created: Arc<AtomicBool>,
}

#[cfg(feature = "cef-browser")]
impl WrapLifeSpanHandler for KiBrowserLifeSpanHandlerImpl {
    fn on_after_created(&mut self, browser: Option<&mut Browser>) {
        info!("Browser created for tab {}", self.tab_id);

        // Store browser reference in tab
        if let Some(b) = browser {
            let mut tabs = self.tabs.write();
            if let Some(tab) = tabs.get_mut(&self.tab_id) {
                tab.set_browser(b.clone());
            }
        }

        self.browser_created.store(true, Ordering::SeqCst);
    }

    fn on_before_close(&mut self, _browser: Option<&mut Browser>) {
        info!("Browser closing for tab {}", self.tab_id);
        let mut tabs = self.tabs.write();
        if let Some(tab) = tabs.get_mut(&self.tab_id) {
            tab.status = TabStatus::Closed;
            tab.browser = None;
        }
    }

    fn do_close(&mut self, _browser: Option<&mut Browser>) -> i32 {
        // Return 0 (false) to allow the browser to close
        0
    }
}

/// Load handler for navigation events and stealth injection using v144 API.
#[cfg(feature = "cef-browser")]
#[cef::wrap_load_handler]
struct KiBrowserLoadHandlerImpl {
    tab_id: Uuid,
    tabs: Arc<RwLock<HashMap<Uuid, CefTab>>>,
    stealth_config: Arc<StealthConfig>,
}

#[cfg(feature = "cef-browser")]
impl WrapLoadHandler for KiBrowserLoadHandlerImpl {
    fn on_loading_state_change(
        &mut self,
        _browser: Option<&mut Browser>,
        is_loading: i32,
        can_go_back: i32,
        can_go_forward: i32,
    ) {
        let is_loading = is_loading != 0;
        let can_go_back = can_go_back != 0;
        let can_go_forward = can_go_forward != 0;

        let mut tabs = self.tabs.write();
        if let Some(tab) = tabs.get_mut(&self.tab_id) {
            if is_loading {
                tab.status = TabStatus::Loading;
                tab.is_ready.store(false, Ordering::SeqCst);
            } else {
                tab.status = TabStatus::Ready;
                tab.is_ready.store(true, Ordering::SeqCst);
            }
        }

        debug!(
            "Loading state changed for tab {}: loading={}, back={}, forward={}",
            self.tab_id, is_loading, can_go_back, can_go_forward
        );
    }

    fn on_load_start(
        &mut self,
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
        &mut self,
        _browser: Option<&mut Browser>,
        frame: Option<&mut Frame>,
        http_status_code: i32,
    ) {
        if let Some(f) = frame {
            if f.is_main() != 0 {
                // Update tab URL
                let mut tabs = self.tabs.write();
                if let Some(tab) = tabs.get_mut(&self.tab_id) {
                    if let Some(url) = f.get_url() {
                        tab.url = url.to_string();
                    }
                }

                info!(
                    "Page loaded for tab {}: status={}",
                    self.tab_id, http_status_code
                );
            }
        }
    }

    fn on_load_error(
        &mut self,
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
    /// Command sender for the CEF message loop thread.
    command_tx: mpsc::Sender<CefCommand>,
    /// Whether the engine is running.
    is_running: Arc<AtomicBool>,
    /// CEF initialized flag (v144 doesn't have CefContext).
    cef_initialized: Arc<AtomicBool>,
    /// Browser ID counter.
    browser_id_counter: Arc<AtomicI32>,
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
        let stealth_config = Arc::new(if let Some(ref user_agent) = config.user_agent {
            // Use consistent fingerprint based on user agent
            StealthConfig::consistent(user_agent)
        } else {
            StealthConfig::random()
        });

        // Validate stealth config
        stealth_config
            .validate()
            .map_err(|e| anyhow!("Invalid stealth config: {}", e))?;

        let tabs = Arc::new(RwLock::new(HashMap::new()));
        let is_running = Arc::new(AtomicBool::new(false));
        let browser_id_counter = Arc::new(AtomicI32::new(0));

        // Create command channel for CEF thread communication
        let (command_tx, mut command_rx) = mpsc::channel::<CefCommand>(32);

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
            cef_initialized,
            browser_id_counter,
        })
    }

    async fn shutdown(&self) -> Result<()> {
        info!("Shutting down CEF browser engine");

        let (response_tx, response_rx) = oneshot::channel();

        self.command_tx
            .send(CefCommand::Shutdown {
                response: response_tx,
            })
            .await
            .context("Failed to send shutdown command")?;

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
            .await
            .context("Failed to send create browser command")?;

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
            .await
            .context("Failed to send close browser command")?;

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
    /// Runs the CEF message loop on a dedicated thread.
    fn run_cef_message_loop(
        config: BrowserConfig,
        stealth_config: Arc<StealthConfig>,
        tabs: Arc<RwLock<HashMap<Uuid, CefTab>>>,
        is_running: Arc<AtomicBool>,
        browser_id_counter: Arc<AtomicI32>,
        cef_initialized: Arc<AtomicBool>,
        mut command_rx: mpsc::Receiver<CefCommand>,
    ) -> Result<()> {
        // Configure CEF settings
        let mut settings = Settings::default();
        settings.windowless_rendering_enabled = 1;
        settings.no_sandbox = 1;
        settings.multi_threaded_message_loop = 0;
        settings.external_message_pump = 1;

        if config.headless {
            settings.windowless_rendering_enabled = 1;
        }

        // Set user agent if provided
        if let Some(ref user_agent) = config.user_agent {
            settings.user_agent = CefString::from(user_agent.as_str());
        }

        // Set log level
        settings.log_severity = LogSeverity::WARNING;

        // Create app with v144 API
        let mut app = KiBrowserApp {
            stealth_config: stealth_config.clone(),
        };

        // Create main args
        let args = MainArgs::default();

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
        loop {
            // Process CEF work
            cef::do_message_loop_work();

            // Process commands with timeout
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
                        CefCommand::Shutdown { response } => {
                            info!("Processing shutdown command");

                            // Close all browsers
                            let tab_ids: Vec<Uuid> = {
                                let tabs_guard = tabs.read();
                                tabs_guard.keys().cloned().collect()
                            };

                            for tab_id in tab_ids {
                                let _ = Self::close_browser_internal(tab_id, tabs.clone());
                            }

                            is_running.store(false, Ordering::SeqCst);
                            let _ = response.send(Ok(()));
                            break;
                        }
                    }
                }
                Err(mpsc::error::TryRecvError::Empty) => {
                    // No command, continue message loop
                }
                Err(mpsc::error::TryRecvError::Disconnected) => {
                    warn!("Command channel disconnected");
                    break;
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
        let viewport_size = config.window_size;

        // Create frame buffer for OSR
        let frame_buffer = Arc::new(RwLock::new(Vec::with_capacity(
            (viewport_size.0 * viewport_size.1 * 4) as usize,
        )));
        let frame_size = Arc::new(RwLock::new((0u32, 0u32)));
        let browser_created = Arc::new(AtomicBool::new(false));

        // Create render handler using v144 API
        let render_handler_impl = KiBrowserRenderHandlerImpl {
            tab_id,
            frame_buffer: frame_buffer.clone(),
            frame_size: frame_size.clone(),
            viewport_size,
        };
        let render_handler = RenderHandler::new(render_handler_impl);

        // Create life span handler
        let life_span_handler_impl = KiBrowserLifeSpanHandlerImpl {
            tab_id,
            tabs: tabs.clone(),
            browser_created: browser_created.clone(),
        };
        let life_span_handler = LifeSpanHandler::new(life_span_handler_impl);

        // Create load handler
        let load_handler_impl = KiBrowserLoadHandlerImpl {
            tab_id,
            tabs: tabs.clone(),
            stealth_config: stealth_config.clone(),
        };
        let load_handler = LoadHandler::new(load_handler_impl);

        // Create client using v144 API
        let client_impl = KiBrowserClient {
            tab_id,
            tabs: tabs.clone(),
            stealth_config: stealth_config.clone(),
            render_handler,
            life_span_handler,
            load_handler,
        };
        let mut client = Client::new(client_impl);

        // Browser settings
        let mut browser_settings = BrowserSettings::default();
        browser_settings.windowless_frame_rate = DEFAULT_FRAME_RATE;

        // Window info for OSR (off-screen rendering)
        let mut window_info = WindowInfo::default();
        window_info.bounds = Rect {
            x: 0,
            y: 0,
            width: viewport_size.0 as i32,
            height: viewport_size.1 as i32,
        };
        window_info.windowless_rendering_enabled = 1;

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
        let cef_tab = CefTab::new(tab_id, url.to_string(), frame_buffer, frame_size);
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
    fn navigate_internal(
        tab_id: Uuid,
        url: &str,
        tabs: Arc<RwLock<HashMap<Uuid, CefTab>>>,
    ) -> Result<()> {
        let tabs_guard = tabs.read();
        let tab = tabs_guard
            .get(&tab_id)
            .ok_or_else(|| anyhow!("Tab not found: {}", tab_id))?;

        let browser = tab.browser.as_ref()
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
    fn execute_js_internal(
        tab_id: Uuid,
        script: &str,
        tabs: Arc<RwLock<HashMap<Uuid, CefTab>>>,
    ) -> Result<Option<String>> {
        let tabs_guard = tabs.read();
        let tab = tabs_guard
            .get(&tab_id)
            .ok_or_else(|| anyhow!("Tab not found: {}", tab_id))?;

        let browser = tab.browser.as_ref()
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
        let tabs_guard = tabs.read();
        let tab = tabs_guard
            .get(&tab_id)
            .ok_or_else(|| anyhow!("Tab not found: {}", tab_id))?;

        let browser = tab.browser.as_ref()
            .ok_or_else(|| anyhow!("Browser not initialized for tab: {}", tab_id))?;

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
        let tabs_guard = tabs.read();
        let tab = tabs_guard
            .get(&tab_id)
            .ok_or_else(|| anyhow!("Tab not found: {}", tab_id))?;

        let browser = tab.browser.as_ref()
            .ok_or_else(|| anyhow!("Browser not initialized for tab: {}", tab_id))?;

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
        let tabs_guard = tabs.read();
        let tab = tabs_guard
            .get(&tab_id)
            .ok_or_else(|| anyhow!("Tab not found: {}", tab_id))?;

        let browser = tab.browser.as_ref()
            .ok_or_else(|| anyhow!("Browser not initialized for tab: {}", tab_id))?;

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

    /// Sends a keyboard event internally on the CEF thread.
    fn key_event_internal(
        tab_id: Uuid,
        event_type: i32,
        modifiers: u32,
        windows_key_code: i32,
        character: u16,
        tabs: Arc<RwLock<HashMap<Uuid, CefTab>>>,
    ) -> Result<()> {
        let tabs_guard = tabs.read();
        let tab = tabs_guard
            .get(&tab_id)
            .ok_or_else(|| anyhow!("Tab not found: {}", tab_id))?;

        let browser = tab.browser.as_ref()
            .ok_or_else(|| anyhow!("Browser not initialized for tab: {}", tab_id))?;

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

    /// Types text by sending character events internally on the CEF thread.
    fn type_text_internal(
        tab_id: Uuid,
        text: &str,
        tabs: Arc<RwLock<HashMap<Uuid, CefTab>>>,
    ) -> Result<()> {
        let tabs_guard = tabs.read();
        let tab = tabs_guard
            .get(&tab_id)
            .ok_or_else(|| anyhow!("Tab not found: {}", tab_id))?;

        let browser = tab.browser.as_ref()
            .ok_or_else(|| anyhow!("Browser not initialized for tab: {}", tab_id))?;

        if let Some(host) = browser.host() {
            for c in text.chars() {
                let char_code = c as u16;

                // Send KeyDown
                let key_down = cef::KeyEvent {
                    size: std::mem::size_of::<cef::KeyEvent>(),
                    type_: cef::KeyEventType::KEYDOWN,
                    modifiers: 0u32,
                    windows_key_code: char_code as i32,
                    native_key_code: 0,
                    is_system_key: 0,
                    character: char_code,
                    unmodified_character: char_code,
                    focus_on_editable_field: 0,
                };
                host.send_key_event(Some(&key_down));

                // Send Char event
                let char_event = cef::KeyEvent {
                    size: std::mem::size_of::<cef::KeyEvent>(),
                    type_: cef::KeyEventType::CHAR,
                    modifiers: 0u32,
                    windows_key_code: char_code as i32,
                    native_key_code: 0,
                    is_system_key: 0,
                    character: char_code,
                    unmodified_character: char_code,
                    focus_on_editable_field: 0,
                };
                host.send_key_event(Some(&char_event));

                // Send KeyUp
                let key_up = cef::KeyEvent {
                    size: std::mem::size_of::<cef::KeyEvent>(),
                    type_: cef::KeyEventType::KEYUP,
                    modifiers: 0u32,
                    windows_key_code: char_code as i32,
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
            .await
            .context("Failed to send navigate command")?;

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
            .await
            .context("Failed to send execute JS command")?;

        response_rx.await.context("Failed to receive execute JS response")?
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
            .await
            .context("Failed to send screenshot command")?;

        response_rx.await.context("Failed to receive screenshot response")?
    }

    /// Returns the stealth configuration.
    pub fn stealth_config(&self) -> &StealthConfig {
        &self.stealth_config
    }

    /// Creates an event sender for a specific tab.
    ///
    /// This can be used to create a `CefInputHandler` for input simulation.
    pub fn create_event_sender(&self, tab_id: Uuid) -> CefBrowserEventSender {
        CefBrowserEventSender::new(
            tab_id,
            self.command_tx.clone(),
            tokio::runtime::Handle::current(),
        )
    }

    // ========================================================================
    // Input Operations
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
            .await
            .context("Failed to send mouse down command")?;
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
            .await
            .context("Failed to send mouse up command")?;
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
            .await
            .context("Failed to send type text command")?;

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
            .await
            .context("Failed to send scroll command")?;

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
            .await
            .context("Failed to send mouse move command")?;

        response_rx.await.context("Failed to receive mouse move response")?
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
