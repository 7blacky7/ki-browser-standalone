//! Tests for the CEF browser engine module.

use crate::browser::engine::{BrowserConfig, BrowserEngine};
use crate::browser::tab::Tab;
use crate::stealth::StealthConfig;
use uuid::Uuid;

use super::CefBrowserEngine;

#[test]
fn test_cef_tab_creation() {
    // Create a mock browser for testing
    // Note: Full CEF tests require CEF runtime
    let _tab_id = Uuid::new_v4();
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
