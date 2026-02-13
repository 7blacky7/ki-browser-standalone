//! HTTP server implementation using axum
//!
//! Provides the main API server with CORS support, graceful shutdown,
//! and tracing middleware.

use std::collections::HashMap;
use std::net::SocketAddr;
use std::sync::Arc;
use std::time::Duration;

use axum::http::{header, Method};
use axum::Router;
use tokio::net::TcpListener;
use tokio::sync::{watch, RwLock};
use tower_http::cors::{Any, CorsLayer};
use tower_http::trace::TraceLayer;
use tracing::{error, info, warn};

use crate::api::routes::create_router;
use crate::api::websocket::WebSocketHandler;
use crate::api::ipc::IpcChannel;

/// Represents a browser tab's state
#[derive(Debug, Clone)]
pub struct TabState {
    pub id: String,
    pub url: String,
    pub title: String,
    pub is_loading: bool,
    pub can_go_back: bool,
    pub can_go_forward: bool,
}

impl Default for TabState {
    fn default() -> Self {
        Self {
            id: String::new(),
            url: String::from("about:blank"),
            title: String::from("New Tab"),
            is_loading: false,
            can_go_back: false,
            can_go_forward: false,
        }
    }
}

/// Shared browser state accessible by all API handlers
#[derive(Debug)]
pub struct BrowserState {
    /// Map of tab ID to tab state
    pub tabs: HashMap<String, TabState>,
    /// Currently active tab ID
    pub active_tab_id: Option<String>,
    /// Counter for generating unique tab IDs
    pub next_tab_id: u64,
}

impl Default for BrowserState {
    fn default() -> Self {
        Self {
            tabs: HashMap::new(),
            active_tab_id: None,
            next_tab_id: 1,
        }
    }
}

impl BrowserState {
    pub fn new() -> Self {
        Self::default()
    }

    /// Generate a new unique tab ID
    pub fn generate_tab_id(&mut self) -> String {
        let id = format!("tab_{}", self.next_tab_id);
        self.next_tab_id += 1;
        id
    }
}

/// Application state shared across all handlers
#[derive(Clone)]
pub struct AppState {
    /// Shared browser state protected by RwLock
    pub browser_state: Arc<RwLock<BrowserState>>,
    /// WebSocket handler for broadcasting events
    pub ws_handler: Arc<WebSocketHandler>,
    /// IPC channel for communicating with the browser core
    pub ipc_channel: Arc<IpcChannel>,
    /// Flag indicating if the API is enabled
    pub api_enabled: Arc<RwLock<bool>>,
}

impl AppState {
    pub fn new(ipc_channel: IpcChannel) -> Self {
        Self {
            browser_state: Arc::new(RwLock::new(BrowserState::new())),
            ws_handler: Arc::new(WebSocketHandler::new()),
            ipc_channel: Arc::new(ipc_channel),
            api_enabled: Arc::new(RwLock::new(true)),
        }
    }

    /// Check if the API is currently enabled
    pub async fn is_enabled(&self) -> bool {
        *self.api_enabled.read().await
    }

    /// Set the API enabled state
    pub async fn set_enabled(&self, enabled: bool) {
        let mut state = self.api_enabled.write().await;
        *state = enabled;
    }
}

/// HTTP API server
pub struct ApiServer {
    /// Port to listen on
    port: u16,
    /// Whether the server is enabled
    enabled: bool,
    /// Shared application state
    state: AppState,
    /// Shutdown signal sender
    shutdown_tx: Option<watch::Sender<bool>>,
    /// Server task handle
    server_handle: Option<tokio::task::JoinHandle<()>>,
}

impl ApiServer {
    /// Create a new API server instance
    pub fn new(port: u16, ipc_channel: IpcChannel) -> Self {
        Self {
            port,
            enabled: false,
            state: AppState::new(ipc_channel),
            shutdown_tx: None,
            server_handle: None,
        }
    }

    /// Create a new API server with existing state
    pub fn with_state(port: u16, state: AppState) -> Self {
        Self {
            port,
            enabled: false,
            state,
            shutdown_tx: None,
            server_handle: None,
        }
    }

    /// Get the server port
    pub fn port(&self) -> u16 {
        self.port
    }

    /// Check if the server is enabled
    pub fn is_enabled(&self) -> bool {
        self.enabled
    }

    /// Get a clone of the application state
    pub fn state(&self) -> AppState {
        self.state.clone()
    }

    /// Configure CORS for localhost development
    fn configure_cors() -> CorsLayer {
        CorsLayer::new()
            // Allow requests from localhost on common development ports
            .allow_origin(Any)
            .allow_methods([
                Method::GET,
                Method::POST,
                Method::PUT,
                Method::DELETE,
                Method::OPTIONS,
            ])
            .allow_headers([
                header::CONTENT_TYPE,
                header::AUTHORIZATION,
                header::ACCEPT,
                header::ORIGIN,
            ])
            .max_age(Duration::from_secs(3600))
    }

    /// Build the router with all middleware
    fn build_router(&self) -> Router {
        create_router(self.state.clone())
            .layer(Self::configure_cors())
            .layer(TraceLayer::new_for_http())
    }

    /// Start the HTTP server
    pub async fn start(&mut self) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        if self.enabled {
            warn!("API server is already running");
            return Ok(());
        }

        let addr = SocketAddr::from(([0, 0, 0, 0], self.port));
        let router = self.build_router();

        // Create shutdown channel
        let (shutdown_tx, mut shutdown_rx) = watch::channel(false);
        self.shutdown_tx = Some(shutdown_tx);

        // Bind the listener
        let listener = TcpListener::bind(addr).await?;
        info!("API server listening on http://{}", addr);

        self.enabled = true;

        // Spawn the server task
        let handle = tokio::spawn(async move {
            axum::serve(listener, router)
                .with_graceful_shutdown(async move {
                    // Wait for shutdown signal
                    while !*shutdown_rx.borrow() {
                        if shutdown_rx.changed().await.is_err() {
                            break;
                        }
                    }
                    info!("API server shutting down gracefully");
                })
                .await
                .unwrap_or_else(|e| {
                    error!("API server error: {}", e);
                });
        });

        self.server_handle = Some(handle);

        Ok(())
    }

    /// Stop the HTTP server gracefully
    pub async fn stop(&mut self) {
        if !self.enabled {
            warn!("API server is not running");
            return;
        }

        info!("Stopping API server...");

        // Send shutdown signal
        if let Some(tx) = self.shutdown_tx.take() {
            let _ = tx.send(true);
        }

        // Wait for the server to finish
        if let Some(handle) = self.server_handle.take() {
            // Give the server some time to shut down gracefully
            tokio::select! {
                _ = handle => {
                    info!("API server stopped successfully");
                }
                _ = tokio::time::sleep(Duration::from_secs(5)) => {
                    warn!("API server shutdown timed out");
                }
            }
        }

        self.enabled = false;
    }

    /// Toggle the server on/off
    pub async fn toggle(&mut self) -> Result<bool, Box<dyn std::error::Error + Send + Sync>> {
        if self.enabled {
            self.stop().await;
            Ok(false)
        } else {
            self.start().await?;
            Ok(true)
        }
    }
}

impl Drop for ApiServer {
    fn drop(&mut self) {
        // Send shutdown signal if server is still running
        if let Some(tx) = self.shutdown_tx.take() {
            let _ = tx.send(true);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_browser_state_default() {
        let state = BrowserState::default();
        assert!(state.tabs.is_empty());
        assert!(state.active_tab_id.is_none());
        assert_eq!(state.next_tab_id, 1);
    }

    #[test]
    fn test_generate_tab_id() {
        let mut state = BrowserState::new();
        assert_eq!(state.generate_tab_id(), "tab_1");
        assert_eq!(state.generate_tab_id(), "tab_2");
        assert_eq!(state.generate_tab_id(), "tab_3");
    }

    #[test]
    fn test_tab_state_default() {
        let tab = TabState::default();
        assert!(tab.id.is_empty());
        assert_eq!(tab.url, "about:blank");
        assert_eq!(tab.title, "New Tab");
        assert!(!tab.is_loading);
    }
}
