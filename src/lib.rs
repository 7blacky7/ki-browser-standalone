//! # KI-Browser Standalone
//!
//! A high-performance, detection-resistant browser automation library written in Rust.
//!
//! KI-Browser provides comprehensive browser control capabilities with built-in
//! anti-detection features, human-like input simulation, and a REST/WebSocket API
//! for external control.
//!
//! ## Features
//!
//! - **Browser Engine Abstraction**: Unified interface for browser control
//! - **Human-like Input Simulation**: Bezier curve mouse movements, realistic typing patterns
//! - **Anti-Detection/Stealth Mode**: Fingerprint spoofing, WebGL noise, navigator overrides
//! - **REST API**: HTTP endpoints for remote browser control
//! - **WebSocket Support**: Real-time event streaming
//! - **Flexible Configuration**: TOML/JSON files, environment variables, CLI arguments
//!
//! ## Quick Start
//!
//! ```rust,no_run
//! use ki_browser_standalone::{
//!     config::BrowserSettings,
//!     browser::BrowserEngine,
//!     stealth::StealthConfig,
//! };
//!
//! #[tokio::main]
//! async fn main() -> anyhow::Result<()> {
//!     // Load configuration
//!     let settings = BrowserSettings::default()
//!         .with_headless(true)
//!         .with_stealth_mode(true);
//!
//!     // Create stealth configuration
//!     let stealth = StealthConfig::random();
//!
//!     // Initialize browser (implementation pending)
//!     // let browser = BrowserEngine::new(settings)?;
//!
//!     Ok(())
//! }
//! ```
//!
//! ## Module Overview
//!
//! - [`browser`]: Core browser engine, tab management, DOM access, screenshots
//! - [`input`]: Human-like mouse and keyboard simulation
//! - [`stealth`]: Anti-detection and fingerprint management
//! - [`api`]: REST API server and WebSocket handlers
//! - [`config`]: Configuration loading and management
//!
//! ## Architecture
//!
//! KI-Browser follows a modular architecture where each component can be used
//! independently or combined for full browser automation:
//!
//! ```text
//! ┌─────────────────────────────────────────────────────────────────┐
//! │                        KI-Browser                               │
//! ├─────────────────────────────────────────────────────────────────┤
//! │  ┌─────────┐  ┌─────────┐  ┌─────────┐  ┌─────────┐            │
//! │  │ Browser │  │  Input  │  │ Stealth │  │   API   │            │
//! │  │ Engine  │  │  Sim    │  │  Mode   │  │ Server  │            │
//! │  └────┬────┘  └────┬────┘  └────┬────┘  └────┬────┘            │
//! │       │            │            │            │                  │
//! │       └────────────┴────────────┴────────────┘                  │
//! │                          │                                      │
//! │                    ┌─────┴─────┐                                │
//! │                    │  Config   │                                │
//! │                    └───────────┘                                │
//! └─────────────────────────────────────────────────────────────────┘
//! ```
//!
//! ## Configuration
//!
//! Configuration follows a precedence chain:
//! 1. Default values
//! 2. Configuration file (TOML/JSON)
//! 3. Environment variables (`KI_BROWSER_*`)
//! 4. CLI arguments
//!
//! See [`config::BrowserSettings`] for all available options.

/// Library version
pub const VERSION: &str = env!("CARGO_PKG_VERSION");

/// Library name
pub const NAME: &str = env!("CARGO_PKG_NAME");

/// Full version string with name
pub const FULL_VERSION: &str = concat!(env!("CARGO_PKG_NAME"), " v", env!("CARGO_PKG_VERSION"));

// ============================================================================
// Module Exports
// ============================================================================

/// Browser engine, tab management, DOM access, and screenshot functionality.
pub mod browser;

/// Human-like input simulation including mouse movements and keyboard input.
pub mod input;

/// Anti-detection features including fingerprint management and stealth mode.
pub mod stealth;

/// REST API server and WebSocket handlers for external browser control.
pub mod api;

/// Configuration management for loading settings from files, env, and CLI.
pub mod config;

// ============================================================================
// Re-exports for Convenience
// ============================================================================

// Browser types
pub use browser::{
    BoundingBox, BrowserConfig, BrowserEngine, DomAccessor, DomElement, MockBrowserEngine,
    MockDomAccessor, ScreenshotFormat, ScreenshotOptions, Tab, TabManager, TabStatus,
};

// Chromiumoxide types (when feature enabled)
#[cfg(feature = "chromium-browser")]
pub use browser::ChromiumBrowserEngine;

// CEF-specific types (when feature enabled)
#[cfg(feature = "cef-browser")]
pub use browser::{
    CefBrowserEngine, CefBrowserEventSender, CefEventSender, CefInputConfig, CefInputHandler,
    CefKeyEvent, CefKeyEventType, CefMouseButton, CefMouseEvent, CefRenderHandler, DirtyRect,
    OffScreenRenderHandler, ScreenInfo,
};

// Input types
pub use input::{
    BezierCurve, HumanTiming, InputError, InputResult, KeyboardEvent, KeyboardSimulator, Modifier,
    MouseButton, MouseEvent, MouseSimulator, Point,
};

// Stealth types
pub use stealth::{
    BrowserFingerprint, FingerprintGenerator, FingerprintProfile, MimeTypeInfo, NavigatorOverrides,
    PluginInfo, StealthConfig, WebGLConfig, WebGLProfile,
};

// API types
pub use api::{
    ApiServer, AppState, BrowserCommandHandler, BrowserEngineWrapper, BrowserEvent, IpcChannel,
    IpcCommand, IpcMessage, IpcProcessor, IpcResponse, WebSocketHandler,
};

// Config types
pub use config::{BrowserSettings, CliArgs, ConfigError, ProxyConfig, ProxyType};

// ============================================================================
// Prelude Module
// ============================================================================

/// Prelude module for convenient imports.
///
/// ```rust
/// use ki_browser_standalone::prelude::*;
/// ```
pub mod prelude {
    pub use crate::api::{ApiServer, AppState};
    pub use crate::browser::{BrowserEngine, DomElement, Tab, TabManager};
    pub use crate::config::{BrowserSettings, CliArgs};
    pub use crate::input::{KeyboardSimulator, MouseButton, MouseSimulator};
    pub use crate::stealth::{FingerprintProfile, StealthConfig};
    pub use crate::{FULL_VERSION, NAME, VERSION};
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_version_constants() {
        assert!(!VERSION.is_empty());
        assert!(!NAME.is_empty());
        assert!(FULL_VERSION.contains(VERSION));
        assert!(FULL_VERSION.contains(NAME));
    }

    #[test]
    fn test_prelude_imports() {
        // Verify prelude types are accessible
        use crate::prelude::*;
        let _ = VERSION;
        let _ = NAME;
    }
}
