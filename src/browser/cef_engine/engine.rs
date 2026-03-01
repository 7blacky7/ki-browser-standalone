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
use tokio::sync::mpsc;
use tracing::info;
use uuid::Uuid;

use crate::browser::engine::{BrowserConfig, BrowserEngine};
use crate::browser::tab::Tab;
use crate::stealth::StealthConfig;
use super::CefCommand;
use super::event_sender::CefBrowserEventSender;
use super::tab::CefTab;

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
    /// Command sender for the CEF message loop thread.
    pub(crate) command_tx: mpsc::Sender<CefCommand>,
    /// Whether the engine is running.
    pub(crate) is_running: Arc<AtomicBool>,
    /// CEF initialized flag (v144 doesn't have CefContext).
    pub(crate) cef_initialized: Arc<AtomicBool>,
    /// Browser ID counter.
    pub(crate) browser_id_counter: Arc<AtomicI32>,
}

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
        let (command_tx, command_rx) = mpsc::channel::<CefCommand>(32);

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
            cef_initialized,
            browser_id_counter,
        })
    }

    async fn shutdown(&self) -> Result<()> {
        info!("Shutting down CEF browser engine");

        let (response_tx, response_rx) = tokio::sync::oneshot::channel();

        self.command_tx
            .send(CefCommand::Shutdown {
                response: response_tx,
            })
            .await
            .context("Failed to send shutdown command")?;

        response_rx
            .await
            .context("Failed to receive shutdown response")?
    }

    async fn create_tab(&self, url: &str) -> Result<Tab> {
        if !self.is_running.load(Ordering::SeqCst) {
            return Err(anyhow!("Browser engine is not running"));
        }

        let tab_id = Uuid::new_v4();
        let (response_tx, response_rx) = tokio::sync::oneshot::channel();

        self.command_tx
            .send(CefCommand::CreateBrowser {
                url: url.to_string(),
                tab_id,
                response: response_tx,
            })
            .await
            .context("Failed to send create browser command")?;

        response_rx
            .await
            .context("Failed to receive create browser response")??;

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

        let (response_tx, response_rx) = tokio::sync::oneshot::channel();

        self.command_tx
            .send(CefCommand::CloseBrowser {
                tab_id,
                response: response_tx,
            })
            .await
            .context("Failed to send close browser command")?;

        response_rx
            .await
            .context("Failed to receive close browser response")?
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
}
