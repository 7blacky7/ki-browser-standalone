//! Chromiumoxide-based browser engine implementation.
//!
//! This module provides a real browser engine implementation using chromiumoxide
//! which controls Chrome/Chromium via the Chrome DevTools Protocol (CDP).

use crate::browser::engine::{BrowserConfig, BrowserEngine};
use crate::browser::tab::Tab;
use anyhow::{anyhow, Result};
use async_trait::async_trait;
use chromiumoxide::browser::{Browser, BrowserConfig as ChromeConfig};
use chromiumoxide::Page;
use futures::StreamExt;
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;
use tracing::{debug, error, info, warn};
use uuid::Uuid;

/// Chromiumoxide-based browser engine.
///
/// This implementation uses the Chrome DevTools Protocol to control
/// a real Chrome/Chromium browser instance.
pub struct ChromiumBrowserEngine {
    config: BrowserConfig,
    browser: Arc<Browser>,
    tabs: Arc<RwLock<HashMap<Uuid, ChromiumTab>>>,
    is_running: Arc<RwLock<bool>>,
    _handler_task: tokio::task::JoinHandle<()>,
}

/// Internal tab representation linking UUID to chromiumoxide Page
struct ChromiumTab {
    pub tab: Tab,
    pub page: Arc<Page>,
}

impl ChromiumBrowserEngine {
    /// Click at coordinates on a page
    pub async fn click(&self, tab_id: Uuid, x: i32, y: i32) -> Result<()> {
        let tabs = self.tabs.read().await;
        let chrome_tab = tabs
            .get(&tab_id)
            .ok_or_else(|| anyhow!("Tab not found: {}", tab_id))?;

        chrome_tab
            .page
            .click_point(chromiumoxide::cdp::browser_protocol::dom::Point {
                x: x as f64,
                y: y as f64,
            })
            .await?;

        Ok(())
    }

    /// Type text on the focused element
    pub async fn type_text(&self, tab_id: Uuid, text: &str) -> Result<()> {
        let tabs = self.tabs.read().await;
        let chrome_tab = tabs
            .get(&tab_id)
            .ok_or_else(|| anyhow!("Tab not found: {}", tab_id))?;

        // Type each character
        for c in text.chars() {
            chrome_tab.page.type_str(&c.to_string()).await?;
        }

        Ok(())
    }

    /// Press a key
    pub async fn press_key(&self, tab_id: Uuid, key: &str) -> Result<()> {
        let tabs = self.tabs.read().await;
        let chrome_tab = tabs
            .get(&tab_id)
            .ok_or_else(|| anyhow!("Tab not found: {}", tab_id))?;

        chrome_tab.page.press_key(key).await?;

        Ok(())
    }

    /// Navigate to a URL
    pub async fn navigate(&self, tab_id: Uuid, url: &str) -> Result<()> {
        let tabs = self.tabs.read().await;
        let chrome_tab = tabs
            .get(&tab_id)
            .ok_or_else(|| anyhow!("Tab not found: {}", tab_id))?;

        chrome_tab.page.goto(url).await?;

        Ok(())
    }

    /// Take a screenshot
    pub async fn screenshot(&self, tab_id: Uuid) -> Result<Vec<u8>> {
        let tabs = self.tabs.read().await;
        let chrome_tab = tabs
            .get(&tab_id)
            .ok_or_else(|| anyhow!("Tab not found: {}", tab_id))?;

        let screenshot = chrome_tab.page.screenshot(
            chromiumoxide::page::ScreenshotParams::builder()
                .format(chromiumoxide::cdp::browser_protocol::page::CaptureScreenshotFormat::Png)
                .build(),
        ).await?;

        Ok(screenshot.inner().to_vec())
    }

    /// Execute JavaScript
    pub async fn evaluate(&self, tab_id: Uuid, script: &str) -> Result<serde_json::Value> {
        let tabs = self.tabs.read().await;
        let chrome_tab = tabs
            .get(&tab_id)
            .ok_or_else(|| anyhow!("Tab not found: {}", tab_id))?;

        let result = chrome_tab.page.evaluate(script).await?;

        Ok(result.value().cloned().unwrap_or(serde_json::Value::Null))
    }

    /// Find element by selector
    pub async fn find_element(&self, tab_id: Uuid, selector: &str) -> Result<bool> {
        let tabs = self.tabs.read().await;
        let chrome_tab = tabs
            .get(&tab_id)
            .ok_or_else(|| anyhow!("Tab not found: {}", tab_id))?;

        match chrome_tab.page.find_element(selector).await {
            Ok(_) => Ok(true),
            Err(_) => Ok(false),
        }
    }

    /// Click on element by selector
    pub async fn click_element(&self, tab_id: Uuid, selector: &str) -> Result<()> {
        let tabs = self.tabs.read().await;
        let chrome_tab = tabs
            .get(&tab_id)
            .ok_or_else(|| anyhow!("Tab not found: {}", tab_id))?;

        let element = chrome_tab.page.find_element(selector).await?;
        element.click().await?;

        Ok(())
    }

    /// Type into element by selector
    pub async fn type_into_element(&self, tab_id: Uuid, selector: &str, text: &str) -> Result<()> {
        let tabs = self.tabs.read().await;
        let chrome_tab = tabs
            .get(&tab_id)
            .ok_or_else(|| anyhow!("Tab not found: {}", tab_id))?;

        let element = chrome_tab.page.find_element(selector).await?;
        element.click().await?;
        element.type_str(text).await?;

        Ok(())
    }

    /// Scroll the page
    pub async fn scroll(&self, tab_id: Uuid, delta_x: i32, delta_y: i32) -> Result<()> {
        let tabs = self.tabs.read().await;
        let chrome_tab = tabs
            .get(&tab_id)
            .ok_or_else(|| anyhow!("Tab not found: {}", tab_id))?;

        // Use JavaScript to scroll
        let script = format!(
            "window.scrollBy({}, {})",
            delta_x, delta_y
        );
        chrome_tab.page.evaluate(&script).await?;

        Ok(())
    }

    /// Get page content (HTML)
    pub async fn get_content(&self, tab_id: Uuid) -> Result<String> {
        let tabs = self.tabs.read().await;
        let chrome_tab = tabs
            .get(&tab_id)
            .ok_or_else(|| anyhow!("Tab not found: {}", tab_id))?;

        let content = chrome_tab.page.content().await?;
        Ok(content)
    }

    /// Get page URL
    pub async fn get_url(&self, tab_id: Uuid) -> Result<String> {
        let tabs = self.tabs.read().await;
        let chrome_tab = tabs
            .get(&tab_id)
            .ok_or_else(|| anyhow!("Tab not found: {}", tab_id))?;

        let url = chrome_tab.page.url().await?.unwrap_or_default();
        Ok(url.to_string())
    }

    /// Get page title
    pub async fn get_title(&self, tab_id: Uuid) -> Result<String> {
        let tabs = self.tabs.read().await;
        let chrome_tab = tabs
            .get(&tab_id)
            .ok_or_else(|| anyhow!("Tab not found: {}", tab_id))?;

        let title = chrome_tab.page.get_title().await?.unwrap_or_default();
        Ok(title)
    }

    /// Wait for navigation to complete
    pub async fn wait_for_navigation(&self, tab_id: Uuid) -> Result<()> {
        let tabs = self.tabs.read().await;
        let chrome_tab = tabs
            .get(&tab_id)
            .ok_or_else(|| anyhow!("Tab not found: {}", tab_id))?;

        chrome_tab.page.wait_for_navigation().await?;
        Ok(())
    }
}

#[async_trait]
impl BrowserEngine for ChromiumBrowserEngine {
    async fn new(config: BrowserConfig) -> Result<Self> {
        info!("Initializing Chromiumoxide browser engine...");

        // Build chromiumoxide config
        let mut chrome_config = ChromeConfig::builder();

        if config.headless {
            chrome_config = chrome_config.no_sandbox().disable_gpu();
        } else {
            chrome_config = chrome_config.with_head();
        }

        // Set window size
        chrome_config = chrome_config.window_size(
            config.window_size.0,
            config.window_size.1,
        );

        // Set user agent if provided
        if let Some(ref ua) = config.user_agent {
            chrome_config = chrome_config.user_agent(ua);
        }

        // Add custom args
        for arg in &config.args {
            chrome_config = chrome_config.arg(arg);
        }

        // Add stealth args
        chrome_config = chrome_config
            .arg("--disable-blink-features=AutomationControlled")
            .arg("--disable-infobars")
            .arg("--disable-dev-shm-usage");

        // Ignore certificate errors if configured
        if config.ignore_certificate_errors {
            chrome_config = chrome_config.arg("--ignore-certificate-errors");
        }

        let chrome_config = chrome_config.build()?;

        // Launch browser
        let (browser, mut handler) = Browser::launch(chrome_config).await?;

        info!("Browser launched successfully");

        // Spawn handler task
        let handler_task = tokio::spawn(async move {
            loop {
                match handler.next().await {
                    Some(event) => {
                        debug!("Browser event: {:?}", event);
                    }
                    None => {
                        warn!("Browser handler stream ended");
                        break;
                    }
                }
            }
        });

        Ok(Self {
            config,
            browser: Arc::new(browser),
            tabs: Arc::new(RwLock::new(HashMap::new())),
            is_running: Arc::new(RwLock::new(true)),
            _handler_task: handler_task,
        })
    }

    async fn shutdown(&self) -> Result<()> {
        info!("Shutting down browser engine...");

        let mut running = self.is_running.write().await;
        if !*running {
            return Err(anyhow!("Browser engine is not running"));
        }

        // Clear tabs
        let mut tabs = self.tabs.write().await;
        tabs.clear();

        *running = false;

        info!("Browser engine shut down");
        Ok(())
    }

    async fn create_tab(&self, url: &str) -> Result<Tab> {
        let running = self.is_running.read().await;
        if !*running {
            return Err(anyhow!("Browser engine is not running"));
        }
        drop(running);

        info!("Creating new tab with URL: {}", url);

        // Create new page
        let page = self.browser.new_page(url).await?;

        // Wait for initial load
        let _ = page.wait_for_navigation().await;

        // Get page info
        let title = page.get_title().await?.unwrap_or_else(|| "New Tab".to_string());
        let current_url = page.url().await?.map(|u| u.to_string()).unwrap_or_else(|| url.to_string());

        // Create Tab
        let mut tab = Tab::new(current_url);
        tab.title = title;
        tab.set_ready();

        let tab_id = tab.id;

        // Store tab
        let chrome_tab = ChromiumTab {
            tab: tab.clone(),
            page: Arc::new(page),
        };

        let mut tabs = self.tabs.write().await;
        tabs.insert(tab_id, chrome_tab);

        info!("Tab created: {}", tab_id);
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

        info!("Tab closed: {}", tab_id);
        Ok(())
    }

    async fn get_tabs(&self) -> Result<Vec<Tab>> {
        let running = self.is_running.read().await;
        if !*running {
            return Err(anyhow!("Browser engine is not running"));
        }
        drop(running);

        let tabs = self.tabs.read().await;
        Ok(tabs.values().map(|ct| ct.tab.clone()).collect())
    }

    async fn get_tab(&self, tab_id: Uuid) -> Result<Option<Tab>> {
        let running = self.is_running.read().await;
        if !*running {
            return Err(anyhow!("Browser engine is not running"));
        }
        drop(running);

        let tabs = self.tabs.read().await;
        Ok(tabs.get(&tab_id).map(|ct| ct.tab.clone()))
    }

    fn config(&self) -> &BrowserConfig {
        &self.config
    }

    async fn is_running(&self) -> bool {
        *self.is_running.read().await
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // Note: These tests require a Chrome/Chromium installation
    // They are ignored by default and can be run with:
    // cargo test --features chromium-browser -- --ignored

    #[tokio::test]
    #[ignore]
    async fn test_chromium_engine_launch() {
        let config = BrowserConfig::default();
        let engine = ChromiumBrowserEngine::new(config).await.unwrap();

        assert!(engine.is_running().await);

        engine.shutdown().await.unwrap();
        assert!(!engine.is_running().await);
    }

    #[tokio::test]
    #[ignore]
    async fn test_chromium_engine_create_tab() {
        let config = BrowserConfig::default();
        let engine = ChromiumBrowserEngine::new(config).await.unwrap();

        let tab = engine.create_tab("https://example.com").await.unwrap();
        assert!(!tab.url.is_empty());

        let tabs = engine.get_tabs().await.unwrap();
        assert_eq!(tabs.len(), 1);

        engine.shutdown().await.unwrap();
    }
}
