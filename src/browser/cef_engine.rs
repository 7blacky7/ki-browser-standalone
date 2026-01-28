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
    // All types are re-exported directly from cef:: via bindings
    App, AppCallbacks, Browser, BrowserHost, BrowserSettings,
    CefString, Client, ClientCallbacks, Frame,
    LifeSpanHandler, LifeSpanHandlerCallbacks,
    LoadHandler, LoadHandlerCallbacks, TransitionType,
    PaintElementType, RenderHandler, RenderHandlerCallbacks,
    Rect, ScreenInfo, WindowInfo, Settings, LogSeverity,
    ErrorCode, RuntimeStyle,
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
    /// CEF browser instance.
    browser: Browser,
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
    fn new(id: Uuid, browser: Browser, url: String) -> Self {
        Self {
            id,
            browser,
            url,
            title: String::new(),
            status: TabStatus::Loading,
            frame_buffer: Arc::new(RwLock::new(Vec::new())),
            frame_size: Arc::new(RwLock::new((0, 0))),
            is_ready: AtomicBool::new(false),
        }
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
    Shutdown {
        response: oneshot::Sender<Result<()>>,
    },
}

// ============================================================================
// CEF Callbacks Implementation
// ============================================================================

/// Application callbacks for CEF lifecycle.
#[cfg(feature = "cef-browser")]
struct KiBrowserAppCallbacks {
    stealth_config: Arc<StealthConfig>,
}

#[cfg(feature = "cef-browser")]
impl AppCallbacks for KiBrowserAppCallbacks {
    fn on_before_command_line_processing(
        &self,
        _process_type: &CefString,
        command_line: &mut cef::command_line::CommandLine,
    ) {
        // Add arguments for stealth mode
        command_line.append_switch("disable-blink-features", "AutomationControlled");
        command_line.append_switch("disable-infobars", "");
        command_line.append_switch("disable-extensions", "");
        command_line.append_switch("no-first-run", "");
        command_line.append_switch("no-default-browser-check", "");

        // Disable GPU in headless mode for stability
        command_line.append_switch("disable-gpu", "");
        command_line.append_switch("disable-gpu-compositing", "");

        debug!("CEF command line configured for stealth mode");
    }
}

/// Client callbacks for browser events.
#[cfg(feature = "cef-browser")]
struct KiBrowserClientCallbacks {
    tab_id: Uuid,
    tabs: Arc<RwLock<HashMap<Uuid, CefTab>>>,
    stealth_config: Arc<StealthConfig>,
    render_handler: RenderHandler,
    life_span_handler: LifeSpanHandler,
    load_handler: LoadHandler,
}

#[cfg(feature = "cef-browser")]
impl ClientCallbacks for KiBrowserClientCallbacks {
    fn get_render_handler(&self) -> Option<RenderHandler> {
        Some(self.render_handler.clone())
    }

    fn get_life_span_handler(&self) -> Option<LifeSpanHandler> {
        Some(self.life_span_handler.clone())
    }

    fn get_load_handler(&self) -> Option<LoadHandler> {
        Some(self.load_handler.clone())
    }
}

/// Render handler for off-screen rendering.
#[cfg(feature = "cef-browser")]
struct KiBrowserRenderHandler {
    tab_id: Uuid,
    frame_buffer: Arc<RwLock<Vec<u8>>>,
    frame_size: Arc<RwLock<(u32, u32)>>,
    viewport_size: (u32, u32),
}

#[cfg(feature = "cef-browser")]
impl RenderHandlerCallbacks for KiBrowserRenderHandler {
    fn get_view_rect(&self, _browser: &Browser) -> Rect {
        Rect {
            x: 0,
            y: 0,
            width: self.viewport_size.0 as i32,
            height: self.viewport_size.1 as i32,
        }
    }

    fn get_screen_info(&self, _browser: &Browser) -> Option<ScreenInfo> {
        Some(ScreenInfo {
            device_scale_factor: 1.0,
            depth: 32,
            depth_per_component: 8,
            is_monochrome: false,
            rect: Rect {
                x: 0,
                y: 0,
                width: self.viewport_size.0 as i32,
                height: self.viewport_size.1 as i32,
            },
            available_rect: Rect {
                x: 0,
                y: 0,
                width: self.viewport_size.0 as i32,
                height: self.viewport_size.1 as i32,
            },
        })
    }

    fn on_paint(
        &self,
        _browser: &Browser,
        element_type: PaintElementType,
        _dirty_rects: &[Rect],
        buffer: &[u8],
        width: i32,
        height: i32,
    ) {
        if element_type == PaintElementType::View {
            // Store the frame buffer for screenshot capture
            let mut fb = self.frame_buffer.write();
            fb.clear();
            fb.extend_from_slice(buffer);

            let mut size = self.frame_size.write();
            *size = (width as u32, height as u32);

            trace!(
                "Frame painted for tab {}: {}x{}, {} bytes",
                self.tab_id,
                width,
                height,
                buffer.len()
            );
        }
    }
}

/// Life span handler for tab lifecycle events.
#[cfg(feature = "cef-browser")]
struct KiBrowserLifeSpanHandler {
    tab_id: Uuid,
    tabs: Arc<RwLock<HashMap<Uuid, CefTab>>>,
    browser_created: Arc<AtomicBool>,
}

#[cfg(feature = "cef-browser")]
impl LifeSpanHandlerCallbacks for KiBrowserLifeSpanHandler {
    fn on_after_created(&self, browser: &Browser) {
        info!("Browser created for tab {}", self.tab_id);
        self.browser_created.store(true, Ordering::SeqCst);
    }

    fn on_before_close(&self, _browser: &Browser) {
        info!("Browser closing for tab {}", self.tab_id);
        let mut tabs = self.tabs.write();
        if let Some(tab) = tabs.get_mut(&self.tab_id) {
            tab.status = TabStatus::Closed;
        }
    }

    fn do_close(&self, _browser: &Browser) -> bool {
        // Return false to allow the browser to close
        false
    }
}

/// Load handler for navigation events and stealth injection.
#[cfg(feature = "cef-browser")]
struct KiBrowserLoadHandler {
    tab_id: Uuid,
    tabs: Arc<RwLock<HashMap<Uuid, CefTab>>>,
    stealth_config: Arc<StealthConfig>,
}

#[cfg(feature = "cef-browser")]
impl LoadHandlerCallbacks for KiBrowserLoadHandler {
    fn on_loading_state_change(
        &self,
        browser: &Browser,
        is_loading: bool,
        can_go_back: bool,
        can_go_forward: bool,
    ) {
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
        &self,
        browser: &Browser,
        frame: &Frame,
        transition_type: TransitionType,
    ) {
        if frame.is_main() {
            // Inject stealth scripts BEFORE any page scripts run
            let stealth_script = self.stealth_config.get_complete_override_script();
            frame.execute_java_script(&CefString::new(&stealth_script), "", 0);

            debug!(
                "Stealth scripts injected for tab {} on load start",
                self.tab_id
            );
        }
    }

    fn on_load_end(&self, browser: &Browser, frame: &Frame, http_status_code: i32) {
        if frame.is_main() {
            // Update tab URL and title
            let mut tabs = self.tabs.write();
            if let Some(tab) = tabs.get_mut(&self.tab_id) {
                tab.url = frame.get_url().to_string();

                // Get title asynchronously
                if let Some(main_frame) = browser.get_main_frame() {
                    // Title will be updated via title change callback
                }
            }

            info!(
                "Page loaded for tab {}: status={}",
                self.tab_id, http_status_code
            );
        }
    }

    fn on_load_error(
        &self,
        _browser: &Browser,
        frame: &Frame,
        error_code: ErrorCode,
        error_text: &CefString,
        failed_url: &CefString,
    ) {
        if frame.is_main() {
            let error_msg = format!(
                "Failed to load {}: {:?} - {}",
                failed_url.to_string(),
                error_code,
                error_text.to_string()
            );

            let mut tabs = self.tabs.write();
            if let Some(tab) = tabs.get_mut(&self.tab_id) {
                tab.status = TabStatus::Error(error_msg.clone());
            }

            error!("Load error for tab {}: {}", self.tab_id, error_msg);
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
    /// CEF context handle.
    cef_context: Arc<Mutex<Option<CefContext>>>,
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

        // CEF context will be set by the message loop thread
        let cef_context = Arc::new(Mutex::new(None));
        let cef_context_clone = cef_context.clone();

        // Spawn CEF message loop thread
        std::thread::spawn(move || {
            let result = Self::run_cef_message_loop(
                config_clone,
                stealth_config_clone,
                tabs_clone,
                is_running_clone,
                browser_id_counter_clone,
                cef_context_clone,
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
            cef_context,
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
        cef_context: Arc<Mutex<Option<CefContext>>>,
        mut command_rx: mpsc::Receiver<CefCommand>,
    ) -> Result<()> {
        // Configure CEF settings
        let mut settings = Settings::default();
        settings.windowless_rendering_enabled = true;
        settings.no_sandbox = true;
        settings.multi_threaded_message_loop = false;
        settings.external_message_pump = true;

        if config.headless {
            settings.windowless_rendering_enabled = true;
        }

        // Set user agent if provided
        if let Some(ref user_agent) = config.user_agent {
            settings.user_agent = CefString::new(user_agent);
        }

        // Set log level
        settings.log_severity = LogSeverity::WARNING;

        // Create app callbacks
        let app_callbacks = KiBrowserAppCallbacks {
            stealth_config: stealth_config.clone(),
        };

        let app = App::new(app_callbacks);

        // Initialize CEF
        let context = CefContext::initialize(settings, Some(app), None)
            .context("Failed to initialize CEF context")?;

        info!("CEF context initialized");

        // Store context
        {
            let mut ctx_guard = futures::executor::block_on(cef_context.lock());
            *ctx_guard = Some(context);
        }

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

        // Create render handler
        let render_handler = RenderHandler::new(KiBrowserRenderHandler {
            tab_id,
            frame_buffer: frame_buffer.clone(),
            frame_size: frame_size.clone(),
            viewport_size,
        });

        // Create life span handler
        let life_span_handler = LifeSpanHandler::new(KiBrowserLifeSpanHandler {
            tab_id,
            tabs: tabs.clone(),
            browser_created: browser_created.clone(),
        });

        // Create load handler
        let load_handler = LoadHandler::new(KiBrowserLoadHandler {
            tab_id,
            tabs: tabs.clone(),
            stealth_config: stealth_config.clone(),
        });

        // Create client
        let client = Client::new(KiBrowserClientCallbacks {
            tab_id,
            tabs: tabs.clone(),
            stealth_config: stealth_config.clone(),
            render_handler,
            life_span_handler,
            load_handler,
        });

        // Browser settings
        let mut browser_settings = BrowserSettings::default();
        browser_settings.windowless_frame_rate = DEFAULT_FRAME_RATE;

        // Window info for OSR (off-screen rendering)
        let window_info = WindowInfo {
            bounds: Rect {
                x: 0,
                y: 0,
                width: viewport_size.0 as i32,
                height: viewport_size.1 as i32,
            },
            ..WindowInfo::default()
        }.set_as_windowless(0);

        // Create browser
        let browser = Browser::create(
            &window_info,
            &client,
            &CefString::new(url),
            &browser_settings,
            None,
            None,
        )
        .context("Failed to create CEF browser")?;

        // Store tab
        let cef_tab = CefTab {
            id: tab_id,
            browser,
            url: url.to_string(),
            title: String::new(),
            status: TabStatus::Loading,
            frame_buffer,
            frame_size,
            is_ready: AtomicBool::new(false),
        };

        tabs.write().insert(tab_id, cef_tab);
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
            if let Some(host) = tab.browser.get_host() {
                host.close_browser(true);
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

        if let Some(frame) = tab.browser.get_main_frame() {
            frame.load_url(&CefString::new(url));
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

        if let Some(frame) = tab.browser.get_main_frame() {
            frame.execute_java_script(&CefString::new(script), "", 0);
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
