//! Browser command handler for IPC processing
//!
//! This module provides a handler that processes IPC commands and forwards them
//! to the appropriate browser engine (CEF or Mock).

use std::sync::Arc;
use tokio::sync::RwLock;
use tracing::{debug, error, info, warn};
use uuid::Uuid;

use crate::api::ipc::{IpcCommand, IpcProcessor, IpcResponse};

#[cfg(feature = "cef-browser")]
use crate::browser::CefBrowserEngine;

use crate::browser::{BrowserEngine, MockBrowserEngine, ScreenshotFormat, ScreenshotOptions};

/// Browser engine wrapper that abstracts over different implementations
pub enum BrowserEngineWrapper {
    /// Mock browser for testing and fallback
    Mock(Arc<MockBrowserEngine>),
    /// CEF browser engine (when feature enabled)
    #[cfg(feature = "cef-browser")]
    Cef(Arc<CefBrowserEngine>),
}

impl BrowserEngineWrapper {
    /// Create a mock browser wrapper
    pub async fn mock() -> anyhow::Result<Self> {
        let config = crate::browser::BrowserConfig::default();
        let engine = MockBrowserEngine::new(config).await?;
        Ok(Self::Mock(Arc::new(engine)))
    }

    /// Create a CEF browser wrapper
    #[cfg(feature = "cef-browser")]
    pub fn cef(engine: CefBrowserEngine) -> Self {
        Self::Cef(Arc::new(engine))
    }

    /// Check if the browser is running
    pub async fn is_running(&self) -> bool {
        match self {
            Self::Mock(engine) => engine.is_running().await,
            #[cfg(feature = "cef-browser")]
            Self::Cef(engine) => engine.is_running().await,
        }
    }
}

/// Handles IPC commands by forwarding them to the browser engine
pub struct BrowserCommandHandler {
    /// The browser engine to use
    engine: Arc<RwLock<Option<BrowserEngineWrapper>>>,
}

impl BrowserCommandHandler {
    /// Create a new handler with no engine (will use mock responses)
    pub fn new() -> Self {
        Self {
            engine: Arc::new(RwLock::new(None)),
        }
    }

    /// Create a handler with a mock browser engine
    pub async fn with_mock() -> anyhow::Result<Self> {
        let wrapper = BrowserEngineWrapper::mock().await?;
        Ok(Self {
            engine: Arc::new(RwLock::new(Some(wrapper))),
        })
    }

    /// Create a handler with a CEF browser engine
    #[cfg(feature = "cef-browser")]
    pub fn with_cef(engine: CefBrowserEngine) -> Self {
        let wrapper = BrowserEngineWrapper::cef(engine);
        Self {
            engine: Arc::new(RwLock::new(Some(wrapper))),
        }
    }

    /// Set the browser engine
    pub async fn set_engine(&self, wrapper: BrowserEngineWrapper) {
        let mut guard = self.engine.write().await;
        *guard = Some(wrapper);
    }

    /// Process a single IPC command
    pub async fn handle_command(&self, command: IpcCommand) -> IpcResponse {
        let engine_guard = self.engine.read().await;

        match command {
            IpcCommand::CreateTab { url, active } => {
                self.handle_create_tab(&engine_guard, &url, active).await
            }
            IpcCommand::CloseTab { tab_id } => {
                self.handle_close_tab(&engine_guard, &tab_id).await
            }
            IpcCommand::Navigate { tab_id, url } => {
                self.handle_navigate(&engine_guard, &tab_id, &url).await
            }
            IpcCommand::ClickCoordinates { tab_id, x, y, button, modifiers } => {
                self.handle_click(&engine_guard, &tab_id, x, y, &button).await
            }
            IpcCommand::TypeText { tab_id, text, selector, clear_first } => {
                self.handle_type_text(&engine_guard, &tab_id, &text, selector.as_deref()).await
            }
            IpcCommand::Scroll { tab_id, x, y, delta_x, delta_y, selector, behavior } => {
                self.handle_scroll(&engine_guard, &tab_id, x, y, delta_x, delta_y).await
            }
            IpcCommand::CaptureScreenshot { tab_id, format, quality, full_page, selector } => {
                self.handle_screenshot(&engine_guard, &tab_id, &format, quality).await
            }
            IpcCommand::EvaluateScript { tab_id, script, await_promise } => {
                self.handle_evaluate(&engine_guard, &tab_id, &script).await
            }
            IpcCommand::GetTabs => {
                self.handle_get_tabs(&engine_guard).await
            }
            IpcCommand::Shutdown => {
                info!("Shutdown command received");
                IpcResponse::success()
            }
            // Handle other commands with mock responses for now
            _ => {
                warn!("Unhandled IPC command: {:?}", command);
                IpcResponse::success()
            }
        }
    }

    async fn handle_create_tab(
        &self,
        engine: &Option<BrowserEngineWrapper>,
        url: &str,
        _active: bool,
    ) -> IpcResponse {
        match engine {
            Some(BrowserEngineWrapper::Mock(e)) => {
                match e.create_tab(url).await {
                    Ok(tab) => IpcResponse::success_with_tab(tab.id.to_string()),
                    Err(e) => IpcResponse::error(e.to_string()),
                }
            }
            #[cfg(feature = "cef-browser")]
            Some(BrowserEngineWrapper::Cef(e)) => {
                match e.create_tab(url).await {
                    Ok(tab) => IpcResponse::success_with_tab(tab.id.to_string()),
                    Err(e) => IpcResponse::error(e.to_string()),
                }
            }
            None => {
                // Generate a mock tab ID
                let tab_id = format!("mock_tab_{}", Uuid::new_v4());
                IpcResponse::success_with_tab(tab_id)
            }
        }
    }

    async fn handle_close_tab(
        &self,
        engine: &Option<BrowserEngineWrapper>,
        tab_id: &str,
    ) -> IpcResponse {
        let uuid = match Uuid::parse_str(tab_id) {
            Ok(u) => u,
            Err(_) => return IpcResponse::error("Invalid tab ID"),
        };

        match engine {
            Some(BrowserEngineWrapper::Mock(e)) => {
                match e.close_tab(uuid).await {
                    Ok(_) => IpcResponse::success(),
                    Err(e) => IpcResponse::error(e.to_string()),
                }
            }
            #[cfg(feature = "cef-browser")]
            Some(BrowserEngineWrapper::Cef(e)) => {
                match e.close_tab(uuid).await {
                    Ok(_) => IpcResponse::success(),
                    Err(e) => IpcResponse::error(e.to_string()),
                }
            }
            None => IpcResponse::success(),
        }
    }

    async fn handle_navigate(
        &self,
        engine: &Option<BrowserEngineWrapper>,
        tab_id: &str,
        url: &str,
    ) -> IpcResponse {
        let uuid = match Uuid::parse_str(tab_id) {
            Ok(u) => u,
            Err(_) => return IpcResponse::error("Invalid tab ID"),
        };

        match engine {
            #[cfg(feature = "cef-browser")]
            Some(BrowserEngineWrapper::Cef(e)) => {
                match e.navigate(uuid, url).await {
                    Ok(_) => IpcResponse::success(),
                    Err(e) => IpcResponse::error(e.to_string()),
                }
            }
            _ => {
                // Mock or no engine - just return success
                debug!("Navigate (mock): {} -> {}", tab_id, url);
                IpcResponse::success()
            }
        }
    }

    async fn handle_click(
        &self,
        engine: &Option<BrowserEngineWrapper>,
        tab_id: &str,
        x: i32,
        y: i32,
        button: &str,
    ) -> IpcResponse {
        let uuid = match Uuid::parse_str(tab_id) {
            Ok(u) => u,
            Err(_) => return IpcResponse::error("Invalid tab ID"),
        };

        let button_code = match button {
            "left" => 0,
            "middle" => 1,
            "right" => 2,
            _ => 0,
        };

        match engine {
            #[cfg(feature = "cef-browser")]
            Some(BrowserEngineWrapper::Cef(e)) => {
                match e.click(uuid, x, y, button_code).await {
                    Ok(_) => IpcResponse::success(),
                    Err(e) => IpcResponse::error(e.to_string()),
                }
            }
            _ => {
                debug!("Click (mock): {} at ({}, {})", tab_id, x, y);
                IpcResponse::success()
            }
        }
    }

    async fn handle_type_text(
        &self,
        engine: &Option<BrowserEngineWrapper>,
        tab_id: &str,
        text: &str,
        _selector: Option<&str>,
    ) -> IpcResponse {
        let uuid = match Uuid::parse_str(tab_id) {
            Ok(u) => u,
            Err(_) => return IpcResponse::error("Invalid tab ID"),
        };

        match engine {
            #[cfg(feature = "cef-browser")]
            Some(BrowserEngineWrapper::Cef(e)) => {
                match e.type_text(uuid, text).await {
                    Ok(_) => IpcResponse::success(),
                    Err(e) => IpcResponse::error(e.to_string()),
                }
            }
            _ => {
                debug!("Type (mock): {} chars to {}", text.len(), tab_id);
                IpcResponse::success()
            }
        }
    }

    async fn handle_scroll(
        &self,
        engine: &Option<BrowserEngineWrapper>,
        tab_id: &str,
        x: Option<i32>,
        y: Option<i32>,
        delta_x: Option<i32>,
        delta_y: Option<i32>,
    ) -> IpcResponse {
        let uuid = match Uuid::parse_str(tab_id) {
            Ok(u) => u,
            Err(_) => return IpcResponse::error("Invalid tab ID"),
        };

        let scroll_x = x.unwrap_or(0);
        let scroll_y = y.unwrap_or(0);
        let dx = delta_x.unwrap_or(0);
        let dy = delta_y.unwrap_or(100); // Default scroll down

        match engine {
            #[cfg(feature = "cef-browser")]
            Some(BrowserEngineWrapper::Cef(e)) => {
                match e.scroll(uuid, scroll_x, scroll_y, dx, dy).await {
                    Ok(_) => IpcResponse::success(),
                    Err(e) => IpcResponse::error(e.to_string()),
                }
            }
            _ => {
                debug!("Scroll (mock): {} by ({}, {})", tab_id, dx, dy);
                IpcResponse::success()
            }
        }
    }

    async fn handle_screenshot(
        &self,
        engine: &Option<BrowserEngineWrapper>,
        tab_id: &str,
        format: &str,
        quality: Option<u8>,
    ) -> IpcResponse {
        let uuid = match Uuid::parse_str(tab_id) {
            Ok(u) => u,
            Err(_) => return IpcResponse::error("Invalid tab ID"),
        };

        let screenshot_format = match format.to_lowercase().as_str() {
            "jpeg" | "jpg" => ScreenshotFormat::Jpeg,
            "webp" => ScreenshotFormat::WebP,
            _ => ScreenshotFormat::Png,
        };

        let options = ScreenshotOptions {
            format: screenshot_format,
            quality: quality.unwrap_or(90),
            ..Default::default()
        };

        match engine {
            #[cfg(feature = "cef-browser")]
            Some(BrowserEngineWrapper::Cef(e)) => {
                match e.screenshot(uuid, options).await {
                    Ok(screenshot) => {
                        IpcResponse::success_with_data(serde_json::json!({
                            "screenshot": screenshot.data,
                            "width": screenshot.width,
                            "height": screenshot.height,
                            "format": format
                        }))
                    }
                    Err(e) => IpcResponse::error(e.to_string()),
                }
            }
            _ => {
                // Return empty mock screenshot
                debug!("Screenshot (mock): {}", tab_id);
                IpcResponse::success_with_data(serde_json::json!({
                    "screenshot": "",
                    "width": 1920,
                    "height": 1080,
                    "format": format
                }))
            }
        }
    }

    async fn handle_evaluate(
        &self,
        engine: &Option<BrowserEngineWrapper>,
        tab_id: &str,
        script: &str,
    ) -> IpcResponse {
        let uuid = match Uuid::parse_str(tab_id) {
            Ok(u) => u,
            Err(_) => return IpcResponse::error("Invalid tab ID"),
        };

        match engine {
            #[cfg(feature = "cef-browser")]
            Some(BrowserEngineWrapper::Cef(e)) => {
                match e.execute_js(uuid, script).await {
                    Ok(result) => {
                        IpcResponse::success_with_data(serde_json::json!({
                            "result": result
                        }))
                    }
                    Err(e) => IpcResponse::error(e.to_string()),
                }
            }
            _ => {
                debug!("Evaluate (mock): {} chars to {}", script.len(), tab_id);
                IpcResponse::success_with_data(serde_json::json!({
                    "result": null
                }))
            }
        }
    }

    async fn handle_get_tabs(&self, engine: &Option<BrowserEngineWrapper>) -> IpcResponse {
        match engine {
            Some(BrowserEngineWrapper::Mock(e)) => {
                match e.get_tabs().await {
                    Ok(tabs) => {
                        let tabs_data: Vec<_> = tabs.iter().map(|t| {
                            serde_json::json!({
                                "id": t.id.to_string(),
                                "url": t.url,
                                "title": t.title,
                            })
                        }).collect();
                        IpcResponse::success_with_data(serde_json::json!({ "tabs": tabs_data }))
                    }
                    Err(e) => IpcResponse::error(e.to_string()),
                }
            }
            #[cfg(feature = "cef-browser")]
            Some(BrowserEngineWrapper::Cef(e)) => {
                match e.get_tabs().await {
                    Ok(tabs) => {
                        let tabs_data: Vec<_> = tabs.iter().map(|t| {
                            serde_json::json!({
                                "id": t.id.to_string(),
                                "url": t.url,
                                "title": t.title,
                            })
                        }).collect();
                        IpcResponse::success_with_data(serde_json::json!({ "tabs": tabs_data }))
                    }
                    Err(e) => IpcResponse::error(e.to_string()),
                }
            }
            None => {
                IpcResponse::success_with_data(serde_json::json!({ "tabs": [] }))
            }
        }
    }

    /// Run the command processing loop
    pub async fn run(&self, processor: &mut IpcProcessor) {
        processor.process(|cmd| self.handle_command(cmd)).await;
    }
}

impl Default for BrowserCommandHandler {
    fn default() -> Self {
        Self::new()
    }
}
