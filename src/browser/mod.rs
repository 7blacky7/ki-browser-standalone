//! Browser module providing core browser automation functionality.
//!
//! This module contains abstractions for browser engine management, tab handling,
//! DOM access, and screenshot capture capabilities.
//!
//! # Submodules
//!
//! - [`engine`] - Browser engine abstraction and configuration
//! - [`tab`] - Tab management and state tracking
//! - [`dom`] - DOM element access and manipulation
//! - [`screenshot`] - Screenshot capture functionality
//! - [`cef_input`] - CEF-specific native input simulation (requires `cef-browser` feature)
//! - [`cef_render`] - CEF offscreen rendering (requires `cef-browser` feature)
//! - [`cef_engine`] - CEF browser engine implementation (requires `cef-browser` feature)

pub mod dom;
pub mod engine;
pub mod screenshot;
pub mod tab;

#[cfg(feature = "cef-browser")]
pub mod cef_input;

#[cfg(feature = "cef-browser")]
pub mod cef_render;

/// CEF browser engine implementation (requires `cef-browser` feature).
#[cfg(feature = "cef-browser")]
pub mod cef_engine;

// Re-export commonly used types for convenience
pub use dom::{BoundingBox, DomAccessor, DomElement, MockDomAccessor};
pub use engine::{BrowserConfig, BrowserEngine, MockBrowserEngine};
pub use screenshot::{ClipRegion, ScreenshotFormat, ScreenshotOptions};
pub use tab::{Tab, TabManager, TabStatus};

#[cfg(feature = "cef-browser")]
pub use cef_render::{CefRenderHandler, DirtyRect, OffScreenRenderHandler, ScreenInfo};

#[cfg(feature = "cef-browser")]
pub use cef_engine::CefBrowserEngine;

#[cfg(feature = "cef-browser")]
pub use cef_input::{
    CefEventSender, CefInputConfig, CefInputHandler, CefKeyEvent, CefKeyEventType,
    CefMouseButton, CefMouseEvent,
};
