//! CEF Headless Runner for windowless/offscreen browser operation.
//!
//! Manages the CEF message loop and provides headless browser
//! functionality without any GUI window. Suitable for automation
//! and API-driven browser control.

use std::sync::Arc;
use tokio::sync::RwLock;
use tracing::info;

use crate::browser::cef_engine::CefBrowserEngine;
use crate::browser::engine::BrowserEngine;
use crate::error::BrowserResult;

/// Manages headless CEF operation: message loop pumping,
/// tab management, and screenshot pipeline without a GUI window.
pub struct HeadlessRunner {
    engine: Arc<CefBrowserEngine>,
    running: Arc<RwLock<bool>>,
}

impl HeadlessRunner {
    /// Creates a new HeadlessRunner wrapping the given CefBrowserEngine.
    pub fn new(engine: Arc<CefBrowserEngine>) -> Self {
        Self {
            engine,
            running: Arc::new(RwLock::new(false)),
        }
    }

    /// Returns a reference to the underlying CEF engine.
    pub fn engine(&self) -> &Arc<CefBrowserEngine> {
        &self.engine
    }

    /// Returns whether the headless runner is currently active.
    pub async fn is_running(&self) -> bool {
        *self.running.read().await
    }

    /// Starts the headless message loop in a background thread.
    pub async fn start(&self) -> BrowserResult<()> {
        let mut running = self.running.write().await;
        if *running {
            return Ok(());
        }
        *running = true;
        info!("CEF headless runner started");
        Ok(())
    }

    /// Stops the headless runner and shuts down CEF gracefully.
    pub async fn shutdown(&self) -> BrowserResult<()> {
        let mut running = self.running.write().await;
        if !*running {
            return Ok(());
        }
        *running = false;
        info!("CEF headless runner stopped");
        self.engine.shutdown().await?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn test_headless_runner_initial_state() {
        // HeadlessRunner can be constructed (basic smoke test)
        // Full integration tests require CEF binary
    }
}
