//! CefBrowserEngine struct definition and BrowserEngine trait implementation.
//!
//! Contains the main engine struct that manages the CEF message loop thread,
//! tab state, stealth configuration, and the command channel for async-to-sync
//! communication. Implements the `BrowserEngine` trait for integration with
//! the ki-browser-standalone architecture.

use anyhow::{anyhow, Context, Result};
use async_trait::async_trait;
use parking_lot::RwLock;
use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, AtomicI32, Ordering};
use std::sync::Arc;
use tokio::sync::{mpsc, oneshot};
use tracing::info;
use uuid::Uuid;

use crate::browser::engine::{BrowserConfig, BrowserEngine};
use crate::browser::tab::Tab;
use crate::stealth::StealthConfig;
use super::CefCommand;
use super::event_sender::CefBrowserEventSender;
use super::tab::CefTab;
use super::TabFrameBuffer;

/// CEF-based browser engine implementation.
///
/// This struct provides a complete browser engine using the Chromium Embedded Framework.
/// It supports headless operation through off-screen rendering (OSR) and includes
/// built-in stealth capabilities. All CEF operations are dispatched to a dedicated
/// message loop thread via the command channel.
pub struct CefBrowserEngine {
    /// Browser configuration.
    pub(crate) config: BrowserConfig,
    /// Stealth configuration for anti-detection.
    pub(crate) stealth_config: Arc<StealthConfig>,
    /// Active tabs indexed by UUID.
    pub(crate) tabs: Arc<RwLock<HashMap<Uuid, CefTab>>>,
    /// Command sender for the CEF message loop thread (unbounded = never drops).
    pub(crate) command_tx: mpsc::UnboundedSender<CefCommand>,
    /// Whether the engine is running.
    pub(crate) is_running: Arc<AtomicBool>,
    /// CEF initialized flag (v144 doesn't have CefContext).
    pub(crate) _cef_initialized: Arc<AtomicBool>,
    /// Browser ID counter.
    pub(crate) _browser_id_counter: Arc<AtomicI32>,
}

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
            let result = super::message_loop::run_cef_message_loop(
                config_clone,
                stealth_config_clone,
                tabs_clone,
                is_running_clone,
                browser_id_counter_clone,
                cef_initialized_clone,
                command_rx,
            );

            if let Err(e) = result {
                tracing::error!("CEF message loop error: {}", e);
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
}
