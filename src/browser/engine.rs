//! Browser engine abstraction layer.
//!
//! This module provides a trait-based abstraction for browser engines,
//! allowing for different implementations (e.g., Chromium, Firefox) and
//! mock implementations for testing.
//!
//! # Example
//!
//! ```rust,ignore
//! use ki_browser::browser::{BrowserEngine, BrowserConfig, MockBrowserEngine};
//!
//! #[tokio::main]
//! async fn main() -> anyhow::Result<()> {
//!     let config = BrowserConfig::default();
//!     let engine = MockBrowserEngine::new(config).await?;
//!
//!     let tab = engine.create_tab("https://example.com").await?;
//!     println!("Created tab: {}", tab.id);
//!
//!     engine.shutdown().await?;
//!     Ok(())
//! }
//! ```

use crate::browser::tab::Tab;
use anyhow::{anyhow, Result};
use async_trait::async_trait;
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;
use uuid::Uuid;

/// Configuration options for browser engine initialization.
#[derive(Debug, Clone)]
pub struct BrowserConfig {
    /// Run browser in headless mode (no visible window).
    pub headless: bool,

    /// Window dimensions as (width, height) in pixels.
    pub window_size: (u32, u32),

    /// Custom user agent string. If None, uses browser default.
    pub user_agent: Option<String>,

    /// Proxy server URL (e.g., "http://proxy.example.com:8080").
    pub proxy: Option<String>,

    /// Path to browser executable. If None, uses system default.
    pub executable_path: Option<String>,

    /// Additional browser launch arguments.
    pub args: Vec<String>,

    /// Timeout for browser operations in milliseconds.
    pub timeout_ms: u64,

    /// Enable browser DevTools.
    pub devtools: bool,

    /// Ignore HTTPS certificate errors.
    pub ignore_certificate_errors: bool,

    /// Custom download directory path.
    pub download_path: Option<String>,
}

impl Default for BrowserConfig {
    fn default() -> Self {
        Self {
            headless: true,
            window_size: (1920, 1080),
            user_agent: None,
            proxy: None,
            executable_path: None,
            args: Vec::new(),
            timeout_ms: 30_000,
            devtools: false,
            ignore_certificate_errors: false,
            download_path: None,
        }
    }
}

impl BrowserConfig {
    /// Creates a new BrowserConfig with default settings.
    pub fn new() -> Self {
        Self::default()
    }

    /// Sets headless mode.
    pub fn headless(mut self, headless: bool) -> Self {
        self.headless = headless;
        self
    }

    /// Sets window size.
    pub fn window_size(mut self, width: u32, height: u32) -> Self {
        self.window_size = (width, height);
        self
    }

    /// Sets custom user agent.
    pub fn user_agent(mut self, user_agent: impl Into<String>) -> Self {
        self.user_agent = Some(user_agent.into());
        self
    }

    /// Sets proxy server.
    pub fn proxy(mut self, proxy: impl Into<String>) -> Self {
        self.proxy = Some(proxy.into());
        self
    }

    /// Sets operation timeout in milliseconds.
    pub fn timeout_ms(mut self, timeout: u64) -> Self {
        self.timeout_ms = timeout;
        self
    }

    /// Adds a browser launch argument.
    pub fn add_arg(mut self, arg: impl Into<String>) -> Self {
        self.args.push(arg.into());
        self
    }

    /// Enables DevTools.
    pub fn with_devtools(mut self) -> Self {
        self.devtools = true;
        self
    }
}

/// Trait defining the browser engine interface.
///
/// This trait provides an abstraction layer for browser automation,
/// allowing different browser implementations to be used interchangeably.
#[async_trait]
pub trait BrowserEngine: Send + Sync {
    /// Creates a new browser engine instance with the given configuration.
    ///
    /// # Arguments
    ///
    /// * `config` - Browser configuration options
    ///
    /// # Returns
    ///
    /// A Result containing the browser engine instance or an error.
    async fn new(config: BrowserConfig) -> Result<Self>
    where
        Self: Sized;

    /// Shuts down the browser engine and releases all resources.
    ///
    /// This method should be called when the browser is no longer needed
    /// to ensure proper cleanup of browser processes and resources.
    async fn shutdown(&self) -> Result<()>;

    /// Creates a new browser tab and navigates to the specified URL.
    ///
    /// # Arguments
    ///
    /// * `url` - The URL to navigate to in the new tab
    ///
    /// # Returns
    ///
    /// A Result containing the created Tab or an error.
    async fn create_tab(&self, url: &str) -> Result<Tab>;

    /// Closes a browser tab by its ID.
    ///
    /// # Arguments
    ///
    /// * `tab_id` - The UUID of the tab to close
    ///
    /// # Returns
    ///
    /// A Result indicating success or an error if the tab doesn't exist.
    async fn close_tab(&self, tab_id: Uuid) -> Result<()>;

    /// Returns a list of all open tabs.
    ///
    /// # Returns
    ///
    /// A Result containing a vector of all open Tab instances.
    async fn get_tabs(&self) -> Result<Vec<Tab>>;

    /// Gets a specific tab by its ID.
    ///
    /// # Arguments
    ///
    /// * `tab_id` - The UUID of the tab to retrieve
    ///
    /// # Returns
    ///
    /// A Result containing an Option with the Tab if found.
    async fn get_tab(&self, tab_id: Uuid) -> Result<Option<Tab>>;

    /// Returns the browser configuration.
    fn config(&self) -> &BrowserConfig;

    /// Checks if the browser engine is running.
    async fn is_running(&self) -> bool;
}

/// Mock browser engine implementation for testing purposes.
///
/// This implementation simulates browser behavior without actually
/// launching a browser, making it suitable for unit tests.
pub struct MockBrowserEngine {
    config: BrowserConfig,
    tabs: Arc<RwLock<HashMap<Uuid, Tab>>>,
    is_running: Arc<RwLock<bool>>,
}

#[async_trait]
impl BrowserEngine for MockBrowserEngine {
    async fn new(config: BrowserConfig) -> Result<Self> {
        Ok(Self {
            config,
            tabs: Arc::new(RwLock::new(HashMap::new())),
            is_running: Arc::new(RwLock::new(true)),
        })
    }

    async fn shutdown(&self) -> Result<()> {
        let mut running = self.is_running.write().await;
        if !*running {
            return Err(anyhow!("Browser engine is not running"));
        }

        // Close all tabs
        let mut tabs = self.tabs.write().await;
        tabs.clear();

        *running = false;
        Ok(())
    }

    async fn create_tab(&self, url: &str) -> Result<Tab> {
        let running = self.is_running.read().await;
        if !*running {
            return Err(anyhow!("Browser engine is not running"));
        }
        drop(running);

        let tab = Tab::new(url.to_string());
        let tab_id = tab.id;

        let mut tabs = self.tabs.write().await;
        tabs.insert(tab_id, tab.clone());

        Ok(tab)
    }

    async fn close_tab(&self, tab_id: Uuid) -> Result<()> {
        let running = self.is_running.read().await;
        if !*running {
            return Err(anyhow!("Browser engine is not running"));
        }
        drop(running);

        let mut tabs = self.tabs.write().await;
        tabs.remove(&tab_id)
            .ok_or_else(|| anyhow!("Tab not found: {}", tab_id))?;

        Ok(())
    }

    async fn get_tabs(&self) -> Result<Vec<Tab>> {
        let running = self.is_running.read().await;
        if !*running {
            return Err(anyhow!("Browser engine is not running"));
        }
        drop(running);

        let tabs = self.tabs.read().await;
        Ok(tabs.values().cloned().collect())
    }

    async fn get_tab(&self, tab_id: Uuid) -> Result<Option<Tab>> {
        let running = self.is_running.read().await;
        if !*running {
            return Err(anyhow!("Browser engine is not running"));
        }
        drop(running);

        let tabs = self.tabs.read().await;
        Ok(tabs.get(&tab_id).cloned())
    }

    fn config(&self) -> &BrowserConfig {
        &self.config
    }

    async fn is_running(&self) -> bool {
        *self.is_running.read().await
    }
}

impl MockBrowserEngine {
    /// Simulates a tab finishing loading.
    ///
    /// This method is useful for testing scenarios where you need
    /// to simulate page load completion.
    pub async fn simulate_tab_ready(&self, tab_id: Uuid) -> Result<()> {
        let mut tabs = self.tabs.write().await;
        if let Some(tab) = tabs.get_mut(&tab_id) {
            tab.set_ready();
            Ok(())
        } else {
            Err(anyhow!("Tab not found: {}", tab_id))
        }
    }

    /// Simulates a tab encountering an error.
    pub async fn simulate_tab_error(&self, tab_id: Uuid, error: &str) -> Result<()> {
        let mut tabs = self.tabs.write().await;
        if let Some(tab) = tabs.get_mut(&tab_id) {
            tab.set_error(error.to_string());
            Ok(())
        } else {
            Err(anyhow!("Tab not found: {}", tab_id))
        }
    }

    /// Updates the title of a tab (simulating title change after page load).
    pub async fn simulate_title_change(&self, tab_id: Uuid, title: &str) -> Result<()> {
        let mut tabs = self.tabs.write().await;
        if let Some(tab) = tabs.get_mut(&tab_id) {
            tab.title = title.to_string();
            Ok(())
        } else {
            Err(anyhow!("Tab not found: {}", tab_id))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_browser_config_builder() {
        let config = BrowserConfig::new()
            .headless(false)
            .window_size(1280, 720)
            .user_agent("TestAgent/1.0")
            .proxy("http://localhost:8080")
            .timeout_ms(60_000)
            .with_devtools();

        assert!(!config.headless);
        assert_eq!(config.window_size, (1280, 720));
        assert_eq!(config.user_agent, Some("TestAgent/1.0".to_string()));
        assert_eq!(config.proxy, Some("http://localhost:8080".to_string()));
        assert_eq!(config.timeout_ms, 60_000);
        assert!(config.devtools);
    }

    #[tokio::test]
    async fn test_mock_engine_create_and_close_tab() {
        let config = BrowserConfig::default();
        let engine = MockBrowserEngine::new(config).await.unwrap();

        let tab = engine.create_tab("https://example.com").await.unwrap();
        assert_eq!(tab.url, "https://example.com");

        let tabs = engine.get_tabs().await.unwrap();
        assert_eq!(tabs.len(), 1);

        engine.close_tab(tab.id).await.unwrap();

        let tabs = engine.get_tabs().await.unwrap();
        assert!(tabs.is_empty());
    }

    #[tokio::test]
    async fn test_mock_engine_shutdown() {
        let config = BrowserConfig::default();
        let engine = MockBrowserEngine::new(config).await.unwrap();

        assert!(engine.is_running().await);

        engine.shutdown().await.unwrap();

        assert!(!engine.is_running().await);

        // Operations should fail after shutdown
        assert!(engine.create_tab("https://example.com").await.is_err());
    }

    #[tokio::test]
    async fn test_mock_engine_simulate_states() {
        let config = BrowserConfig::default();
        let engine = MockBrowserEngine::new(config).await.unwrap();

        let tab = engine.create_tab("https://example.com").await.unwrap();

        engine.simulate_tab_ready(tab.id).await.unwrap();
        let updated_tab = engine.get_tab(tab.id).await.unwrap().unwrap();
        assert!(matches!(updated_tab.status, crate::browser::tab::TabStatus::Ready));

        engine
            .simulate_title_change(tab.id, "Example Domain")
            .await
            .unwrap();
        let updated_tab = engine.get_tab(tab.id).await.unwrap().unwrap();
        assert_eq!(updated_tab.title, "Example Domain");
    }
}
