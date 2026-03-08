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
//! # Submodules
//!
//! - [`tab`] - Internal CEF tab representation and lifecycle
//! - [`event_sender`] - Bridge between CefInputHandler and CefBrowserEngine command channel
//! - [`callbacks`] - CEF callback handlers (App, Client, Render, LifeSpan, Load, Display)
//! - [`engine`] - CefBrowserEngine struct and BrowserEngine trait implementation
//! - [`message_loop`] - CEF message loop, initialization, and browser creation on the CEF thread
//! - [`navigation`] - Navigation, JavaScript execution, and screenshot internal methods
//! - [`input`] - Mouse, keyboard, and text input internal methods on the CEF thread

#[cfg(feature = "cef-browser")]
mod tab;
#[cfg(feature = "cef-browser")]
mod event_sender;
#[cfg(feature = "cef-browser")]
pub(crate) mod callbacks;
#[cfg(feature = "cef-browser")]
mod engine;
#[cfg(feature = "cef-browser")]
mod message_loop;
#[cfg(feature = "cef-browser")]
mod navigation;
#[cfg(feature = "cef-browser")]
mod input;

#[cfg(feature = "cef-browser")]
pub use engine::CefBrowserEngine;
#[cfg(feature = "cef-browser")]
pub use event_sender::CefBrowserEventSender;

// ============================================================================
// Shared internal types used across submodules
// ============================================================================

#[cfg(feature = "cef-browser")]
use anyhow::Result;
#[cfg(feature = "cef-browser")]
use parking_lot::RwLock;
#[cfg(feature = "cef-browser")]
use std::sync::Arc;
#[cfg(feature = "cef-browser")]
use tokio::sync::oneshot;
#[cfg(feature = "cef-browser")]
use uuid::Uuid;

#[cfg(feature = "cef-browser")]
use crate::browser::screenshot::{Screenshot, ScreenshotOptions};

/// Delay between CEF message loop iterations in milliseconds.
#[cfg(feature = "cef-browser")]
pub(crate) const CEF_MESSAGE_LOOP_DELAY_MS: u64 = 10;

/// Default off-screen rendering frame rate for CEF browsers.
#[cfg(feature = "cef-browser")]
pub(crate) const DEFAULT_FRAME_RATE: i32 = 30;

/// Type alias for the JS result sender map to reduce type complexity.
#[cfg(feature = "cef-browser")]
type JsResultStore = parking_lot::Mutex<std::collections::HashMap<i64, std::sync::mpsc::Sender<Result<String, String>>>>;

/// Type alias for tab frame buffer data (pixel buffer + dimensions + frame version).
#[cfg(feature = "cef-browser")]
pub type TabFrameBuffer = (Arc<RwLock<Vec<u8>>>, Arc<RwLock<(u32, u32)>>, Arc<std::sync::atomic::AtomicU64>);

/// Global JS-Result-Store: query_id -> mpsc::Sender for results.
/// Used to pass cefQuery results back to execute_js_with_result_internal.
#[cfg(feature = "cef-browser")]
static JS_RESULT_STORE: once_cell::sync::Lazy<JsResultStore> =
    once_cell::sync::Lazy::new(|| parking_lot::Mutex::new(std::collections::HashMap::new()));

/// Global BrowserSideRouter (initialized once on first use on the CEF thread).
#[cfg(feature = "cef-browser")]
static BROWSER_ROUTER: once_cell::sync::Lazy<std::sync::Arc<cef::wrapper::message_router::BrowserSideRouter>> =
    once_cell::sync::Lazy::new(|| {
        use cef::wrapper::message_router::{MessageRouterBrowserSide, MessageRouterConfig};
        let router: std::sync::Arc<cef::wrapper::message_router::BrowserSideRouter> =
            MessageRouterBrowserSide::new(MessageRouterConfig::default());
        router.add_handler(std::sync::Arc::new(callbacks::KiBrowserQueryHandler), true);
        router
    });

/// Global RendererSideRouter (initialized once on first use on the render thread).
#[cfg(feature = "cef-browser")]
static RENDERER_ROUTER: once_cell::sync::Lazy<std::sync::Arc<cef::wrapper::message_router::RendererSideRouter>> =
    once_cell::sync::Lazy::new(|| {
        use cef::wrapper::message_router::{MessageRouterRendererSide, MessageRouterConfig};
        MessageRouterRendererSide::new(MessageRouterConfig::default())
    });

/// Commands sent from the async API to the synchronous CEF message loop thread.
///
/// Each variant represents an operation that must be executed on the CEF thread
/// because CEF requires single-threaded access. The oneshot response channel
/// allows the caller to await the result asynchronously.
#[cfg(feature = "cef-browser")]
pub(crate) enum CefCommand {
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
    // Input commands
    MouseMove {
        tab_id: Uuid,
        x: i32,
        y: i32,
        response: oneshot::Sender<Result<()>>,
    },
    MouseClick {
        tab_id: Uuid,
        x: i32,
        y: i32,
        button: i32,
        click_count: i32,
        response: oneshot::Sender<Result<()>>,
    },
    MouseWheel {
        tab_id: Uuid,
        x: i32,
        y: i32,
        delta_x: i32,
        delta_y: i32,
        response: oneshot::Sender<Result<()>>,
    },
    KeyEvent {
        tab_id: Uuid,
        event_type: i32,
        modifiers: u32,
        windows_key_code: i32,
        character: u16,
        response: oneshot::Sender<Result<()>>,
    },
    TypeText {
        tab_id: Uuid,
        text: String,
        response: oneshot::Sender<Result<()>>,
    },
    Drag {
        tab_id: Uuid,
        from_x: i32,
        from_y: i32,
        to_x: i32,
        to_y: i32,
        steps: u32,
        duration_ms: u64,
        response: oneshot::Sender<Result<()>>,
    },
    ExecuteJsWithResult {
        tab_id: Uuid,
        script: String,
        response: oneshot::Sender<Result<Option<String>>>,
    },
    /// Navigate the browser back in history.
    GoBack {
        tab_id: Uuid,
        response: oneshot::Sender<Result<()>>,
    },
    /// Navigate the browser forward in history.
    GoForward {
        tab_id: Uuid,
        response: oneshot::Sender<Result<()>>,
    },
    /// Resize the CEF viewport for a tab and notify the browser.
    ResizeViewport {
        tab_id: Uuid,
        width: u32,
        height: u32,
        response: oneshot::Sender<Result<()>>,
    },
    Shutdown {
        response: oneshot::Sender<Result<()>>,
    },
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(all(test, feature = "cef-browser"))]
mod tests;

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
