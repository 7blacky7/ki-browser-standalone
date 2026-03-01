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
//! - [`callbacks`] - CEF callback handlers (App, Client, Render, LifeSpan, Load)
//! - [`engine`] - CefBrowserEngine struct and BrowserEngine trait implementation
//! - [`message_loop`] - CEF message loop, initialization, and browser creation on the CEF thread
//! - [`navigation`] - Navigation, JavaScript execution, and screenshot internal methods
//! - [`input`] - Mouse, keyboard, and text input internal methods on the CEF thread

#[cfg(feature = "cef-browser")]
mod tab;
#[cfg(feature = "cef-browser")]
mod event_sender;
#[cfg(feature = "cef-browser")]
mod callbacks;
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
use tokio::sync::oneshot;
#[cfg(feature = "cef-browser")]
use uuid::Uuid;

#[cfg(feature = "cef-browser")]
use crate::browser::screenshot::ScreenshotOptions;

/// Delay between CEF message loop iterations in milliseconds.
#[cfg(feature = "cef-browser")]
pub(crate) const CEF_MESSAGE_LOOP_DELAY_MS: u64 = 10;

/// Default off-screen rendering frame rate for CEF browsers.
#[cfg(feature = "cef-browser")]
pub(crate) const DEFAULT_FRAME_RATE: i32 = 30;

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
    Shutdown {
        response: oneshot::Sender<Result<()>>,
    },
}

#[cfg(feature = "cef-browser")]
use crate::browser::screenshot::Screenshot;

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
