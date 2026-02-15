//! Chromiumoxide-based browser engine implementation.
//!
//! This module provides a real browser engine implementation using chromiumoxide
//! which controls Chrome/Chromium via the Chrome DevTools Protocol (CDP).

use crate::browser::engine::{BrowserConfig, BrowserEngine};
use crate::browser::tab::Tab;
use crate::stealth::StealthConfig;
use anyhow::{anyhow, Result};
use async_trait::async_trait;
use chromiumoxide::browser::{Browser, BrowserConfig as ChromeConfig};
use chromiumoxide::cdp::browser_protocol::input::{
    DispatchKeyEventParams, DispatchKeyEventType, DispatchMouseEventParams, DispatchMouseEventType,
    MouseButton,
};
use chromiumoxide::cdp::browser_protocol::page::CloseParams;
use chromiumoxide::Page;
use futures::StreamExt;
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;
use tracing::{debug, info, warn};
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
    stealth_config: Arc<StealthConfig>,
    _handler_task: tokio::task::JoinHandle<()>,
}

/// Internal tab representation linking UUID to chromiumoxide Page
struct ChromiumTab {
    pub tab: Tab,
    pub page: Arc<Page>,
}

impl ChromiumBrowserEngine {
    /// Click at coordinates on a page using CDP Input.dispatchMouseEvent
    pub async fn click(&self, tab_id: Uuid, x: i32, y: i32) -> Result<()> {
        let tabs = self.tabs.read().await;
        let chrome_tab = tabs
            .get(&tab_id)
            .ok_or_else(|| anyhow!("Tab not found: {}", tab_id))?;

        // Mouse down
        let mouse_down = DispatchMouseEventParams::builder()
            .r#type(DispatchMouseEventType::MousePressed)
            .x(x as f64)
            .y(y as f64)
            .button(MouseButton::Left)
            .click_count(1)
            .build()
            .map_err(|e| anyhow!("Failed to build mouse down event: {}", e))?;
        chrome_tab.page.execute(mouse_down).await?;

        // Mouse up
        let mouse_up = DispatchMouseEventParams::builder()
            .r#type(DispatchMouseEventType::MouseReleased)
            .x(x as f64)
            .y(y as f64)
            .button(MouseButton::Left)
            .click_count(1)
            .build()
            .map_err(|e| anyhow!("Failed to build mouse up event: {}", e))?;
        chrome_tab.page.execute(mouse_up).await?;

        Ok(())
    }

    /// Type text using CDP Input.dispatchKeyEvent
    pub async fn type_text(&self, tab_id: Uuid, text: &str) -> Result<()> {
        let tabs = self.tabs.read().await;
        let chrome_tab = tabs
            .get(&tab_id)
            .ok_or_else(|| anyhow!("Tab not found: {}", tab_id))?;

        // Type each character using keyDown/keyUp events
        for c in text.chars() {
            let key_down = DispatchKeyEventParams::builder()
                .r#type(DispatchKeyEventType::KeyDown)
                .text(c.to_string())
                .build()
                .map_err(|e| anyhow!("Failed to build key down event: {}", e))?;
            chrome_tab.page.execute(key_down).await?;

            let key_up = DispatchKeyEventParams::builder()
                .r#type(DispatchKeyEventType::KeyUp)
                .text(c.to_string())
                .build()
                .map_err(|e| anyhow!("Failed to build key up event: {}", e))?;
            chrome_tab.page.execute(key_up).await?;
        }

        Ok(())
    }

    /// Press a special key
    pub async fn press_key(&self, tab_id: Uuid, key: &str) -> Result<()> {
        let tabs = self.tabs.read().await;
        let chrome_tab = tabs
            .get(&tab_id)
            .ok_or_else(|| anyhow!("Tab not found: {}", tab_id))?;

        let key_down = DispatchKeyEventParams::builder()
            .r#type(DispatchKeyEventType::KeyDown)
            .key(key.to_string())
            .build()
            .map_err(|e| anyhow!("Failed to build key down event: {}", e))?;
        chrome_tab.page.execute(key_down).await?;

        let key_up = DispatchKeyEventParams::builder()
            .r#type(DispatchKeyEventType::KeyUp)
            .key(key.to_string())
            .build()
            .map_err(|e| anyhow!("Failed to build key up event: {}", e))?;
        chrome_tab.page.execute(key_up).await?;

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

        let screenshot = chrome_tab
            .page
            .screenshot(
                chromiumoxide::page::ScreenshotParams::builder()
                    .format(
                        chromiumoxide::cdp::browser_protocol::page::CaptureScreenshotFormat::Png,
                    )
                    .build(),
            )
            .await?;

        Ok(screenshot)
    }

    /// Execute JavaScript
    pub async fn evaluate(&self, tab_id: Uuid, script: &str) -> Result<serde_json::Value> {
        let tabs = self.tabs.read().await;
        let chrome_tab = tabs
            .get(&tab_id)
            .ok_or_else(|| anyhow!("Tab not found: {}", tab_id))?;

        let result = chrome_tab.page.evaluate(script.to_string()).await?;

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
    pub async fn type_into_element(
        &self,
        tab_id: Uuid,
        selector: &str,
        text: &str,
    ) -> Result<()> {
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
        let script = format!("window.scrollBy({}, {})", delta_x, delta_y);
        chrome_tab.page.evaluate(script).await?;

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

        // Build chromiumoxide config using args
        let mut chrome_config = ChromeConfig::builder();

        if config.headless {
            chrome_config = chrome_config.no_sandbox();
            // --headless=new ist Chromes neuer Headless-Modus (schwerer erkennbar)
            chrome_config = chrome_config.arg("--headless=new");
            // KEIN --disable-gpu! Das erzwingt SwiftShader und ist sofort erkennbar
        } else {
            chrome_config = chrome_config.with_head();
        }

        // Set window size
        chrome_config = chrome_config.window_size(config.window_size.0, config.window_size.1);

        // User-Agent: Immer einen realistischen setzen (egal ob custom oder default)
        let ua = config.user_agent.as_deref().unwrap_or(
            "Mozilla/5.0 (X11; Linux x86_64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/131.0.0.0 Safari/537.36"
        );
        chrome_config = chrome_config.arg(format!("--user-agent={}", ua));

        // Add custom args
        for arg in &config.args {
            chrome_config = chrome_config.arg(arg);
        }

        // Stealth args - Automation-Spuren entfernen
        chrome_config = chrome_config
            .arg("--disable-blink-features=AutomationControlled")
            .arg("--disable-infobars")
            .arg("--disable-dev-shm-usage")
            .arg("--no-first-run")
            .arg("--no-default-browser-check")
            .arg("--disable-background-timer-throttling")
            .arg("--disable-backgrounding-occluded-windows")
            .arg("--disable-renderer-backgrounding")
            .arg("--disable-ipc-flooding-protection");

        // Ignore certificate errors if configured
        if config.ignore_certificate_errors {
            chrome_config = chrome_config.arg("--ignore-certificate-errors");
        }

        let chrome_config = chrome_config
            .build()
            .map_err(|e| anyhow!("Failed to build browser config: {}", e))?;

        // Launch browser
        let (browser, mut handler) = Browser::launch(chrome_config).await?;

        info!("Browser launched successfully");

        // Create stealth configuration - use Chrome-only profiles for Chromium engine
        // to avoid detectable mismatches (e.g. Safari UA on a Chrome browser)
        let mut stealth = if let Some(ref user_agent) = config.user_agent {
            StealthConfig::consistent(user_agent)
        } else {
            StealthConfig::random_chrome()
        };

        // Sync screen resolution to the actual viewport so that
        // screen.width >= outerWidth >= innerWidth and orientation is correct.
        stealth.sync_screen_to_viewport(config.window_size.0, config.window_size.1);

        let stealth_config = Arc::new(stealth);

        stealth_config
            .validate()
            .map_err(|e| anyhow!("Invalid stealth config: {}", e))?;

        info!("Stealth config initialized with WebRTC, Canvas, Audio protection (screen synced to {}x{} viewport)",
              config.window_size.0, config.window_size.1);

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
            stealth_config,
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

        // Seite erstellen und Stealth injizieren
        let page = self.browser.new_page("about:blank").await?;

        // === StealthConfig: Comprehensive anti-detection overrides ===
        // Each section (Navigator, WebGL, Fingerprint, WebRTC, Canvas, Audio) is
        // injected as a SEPARATE evaluate_on_new_document call.  This ensures
        // that a failure in one section (e.g. WebGL prototype override failing at
        // document-creation time) does not prevent the other sections from running.
        let stealth_sections = self.stealth_config.get_section_scripts();

        // === Chrome-specific supplement ===
        // StealthConfig does NOT cover Chrome-specific APIs that are absent in
        // headless mode or automation artifacts injected by ChromeDriver/CDP.
        let chrome_supplement_js = r#"
            (function() {
            'use strict';

            // === 1. Chrome Runtime faking (missing in headless) ===
            if (!window.chrome) window.chrome = {};
            if (!window.chrome.runtime) {
                window.chrome.runtime = {
                    connect: function() {},
                    sendMessage: function() {},
                    id: undefined
                };
            }
            if (!window.chrome.loadTimes) {
                window.chrome.loadTimes = function() {
                    return { commitLoadTime: Date.now() / 1000 };
                };
            }
            if (!window.chrome.csi) {
                window.chrome.csi = function() {
                    return { startE: Date.now(), onloadT: Date.now() };
                };
            }

            // === 2. CDC/Automation artifacts removal ===
            delete window.cdc_adoQpoasnfa76pfcZLmcfl_Array;
            delete window.cdc_adoQpoasnfa76pfcZLmcfl_Promise;
            delete window.cdc_adoQpoasnfa76pfcZLmcfl_Symbol;

            // Clean automation traces from Error stack traces
            const originalError = Error;
            window.Error = function(...args) {
                const err = new originalError(...args);
                const stack = err.stack || '';
                err.stack = stack.replace(/at Object\.apply \(<anonymous>\)/g, '');
                return err;
            };
            window.Error.prototype = originalError.prototype;

            // === 3. navigator.connection.rtt (Chrome-specific NetworkInformation API) ===
            if (navigator.connection) {
                Object.defineProperty(navigator.connection, 'rtt', { get: () => 50 });
            }

            // === 4. outerWidth/outerHeight ===
            // Now handled by BrowserFingerprint::to_js_overrides() with values
            // consistent with screen resolution and orientation.
            // Only apply a minimal fallback if outerHeight is still 0 (headless
            // mode before the fingerprint script has run).
            if (window.outerHeight === 0 && !window.__fp_outer_applied) {
                Object.defineProperty(window, 'outerHeight', { get: () => window.innerHeight + 85, configurable: true });
                Object.defineProperty(window, 'outerWidth', { get: () => window.innerWidth + 16, configurable: true });
            }

            // === 5. Permissions API spoofing ===
            const originalQuery = window.navigator.permissions?.query;
            if (originalQuery) {
                window.navigator.permissions.query = function(parameters) {
                    if (parameters.name === 'notifications') {
                        return Promise.resolve({ state: Notification.permission });
                    }
                    return originalQuery(parameters);
                };
            }

            })();
        "#;

        // Inject each stealth section as a SEPARATE evaluate_on_new_document call
        for section in &stealth_sections {
            page.evaluate_on_new_document(section.clone()).await?;
        }
        // Then inject Chrome-specific supplement
        page.evaluate_on_new_document(chrome_supplement_js.to_string()).await?;

        // Also execute immediately on the current about:blank page
        for section in &stealth_sections {
            let _ = page.evaluate(section.clone()).await;
        }
        let _ = page.evaluate(chrome_supplement_js.to_string()).await;

        // JETZT erst zur Zielseite navigieren - Stealth-Scripts sind registriert
        if url != "about:blank" {
            let _ = page.goto(url).await;
            let _ = page.wait_for_navigation().await;
        }

        // Get page info
        let title = page
            .get_title()
            .await?
            .unwrap_or_else(|| "New Tab".to_string());
        let current_url = page
            .url()
            .await?
            .map(|u| u.to_string())
            .unwrap_or_else(|| url.to_string());

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
        let chrome_tab = tabs.remove(&tab_id)
            .ok_or_else(|| anyhow!("Tab not found: {}", tab_id))?;

        // CDP-Befehl senden um den Tab tatsaechlich im Browser zu schliessen
        if let Err(e) = chrome_tab.page.execute(CloseParams::default()).await {
            warn!("Failed to close tab via CDP: {}", e);
        }

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
