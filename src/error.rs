//! Unified error handling for ki-browser-standalone.
//!
//! This module provides a single [`BrowserError`] type that unifies all error
//! handling across the crate. Instead of scattering multiple ad-hoc error enums
//! throughout the codebase, every subsystem can convert its errors into
//! `BrowserError` via the provided `From` implementations.
//!
//! # Design Goals
//!
//! - **No panics in production code.** Every error path should return a
//!   `Result<T, BrowserError>` instead of calling `panic!()`, `unwrap()`, or
//!   `expect()`.
//! - **Ergonomic conversions.** Common foreign error types (`anyhow::Error`,
//!   `serde_json::Error`, `std::io::Error`, etc.) implement `From` so the `?`
//!   operator works seamlessly.
//! - **Actionable messages.** Each variant carries enough context (tab ID,
//!   selector, URL, ...) to produce a useful log line or API response.
//!
//! # Example
//!
//! ```rust
//! use ki_browser_standalone::error::{BrowserError, BrowserResult};
//!
//! fn get_tab(tab_id: &str) -> BrowserResult<String> {
//!     if tab_id.is_empty() {
//!         return Err(BrowserError::TabNotFound {
//!             tab_id: tab_id.to_string(),
//!         });
//!     }
//!     Ok(format!("tab-{}", tab_id))
//! }
//! ```

use thiserror::Error;

// ---------------------------------------------------------------------------
// Core error type
// ---------------------------------------------------------------------------

/// Unified error type for ki-browser-standalone.
///
/// Every fallible operation across the crate should ultimately return
/// `Result<T, BrowserError>`. Subsystem-specific error types (`IpcError`,
/// `ConfigError`, `InputError`) can be converted via the `From` impls at the
/// bottom of this module.
#[derive(Error, Debug)]
pub enum BrowserError {
    /// The requested tab does not exist.
    #[error("Tab not found: {tab_id}")]
    TabNotFound {
        /// Identifier of the missing tab.
        tab_id: String,
    },

    /// A page navigation failed.
    #[error("Navigation failed for {url}: {reason}")]
    NavigationFailed {
        /// Target URL that could not be loaded.
        url: String,
        /// Human-readable reason for the failure.
        reason: String,
    },

    /// A DOM query (CSS selector, XPath, ...) failed.
    #[error("DOM query failed for selector '{selector}': {reason}")]
    DomQueryFailed {
        /// The selector that was used.
        selector: String,
        /// Human-readable reason for the failure.
        reason: String,
    },

    /// JavaScript evaluation failed inside the browser context.
    #[error("Script evaluation failed: {reason}")]
    ScriptEvaluationFailed {
        /// Human-readable reason for the failure.
        reason: String,
    },

    /// Capturing a screenshot failed.
    #[error("Screenshot capture failed: {reason}")]
    ScreenshotFailed {
        /// Human-readable reason for the failure.
        reason: String,
    },

    /// An IPC (inter-process communication) error.
    #[error("IPC error: {0}")]
    IpcError(String),

    /// A configuration loading or validation error.
    #[error("Configuration error: {0}")]
    ConfigError(String),

    /// An operation exceeded its deadline.
    #[error("Timeout after {duration_ms}ms: {operation}")]
    Timeout {
        /// Description of the operation that timed out.
        operation: String,
        /// Deadline in milliseconds.
        duration_ms: u64,
    },

    /// A low-level browser engine error.
    #[error("Browser engine error: {0}")]
    EngineError(String),

    /// A stealth / anti-detection configuration error.
    #[error("Stealth configuration error: {0}")]
    StealthError(String),

    /// A session-level error (e.g. browser process died).
    #[error("Session error: {0}")]
    SessionError(String),

    /// An error that occurred during a batch operation.
    #[error("Batch operation error: {0}")]
    BatchError(String),

    /// A form-interaction error (fill, submit, ...).
    #[error("Form handling error: {0}")]
    FormError(String),

    /// A WebSocket transport error.
    #[error("WebSocket error: {0}")]
    WebSocketError(String),

    /// The incoming request is malformed or missing required fields.
    #[error("Invalid request: {0}")]
    InvalidRequest(String),

    /// Catch-all for internal / unexpected errors.
    #[error("Internal error: {0}")]
    Internal(String),
}

// ---------------------------------------------------------------------------
// Result alias
// ---------------------------------------------------------------------------

/// Convenience alias used throughout the crate.
pub type BrowserResult<T> = Result<T, BrowserError>;

// ---------------------------------------------------------------------------
// From conversions for foreign error types
// ---------------------------------------------------------------------------

impl From<anyhow::Error> for BrowserError {
    fn from(err: anyhow::Error) -> Self {
        BrowserError::Internal(format!("{:#}", err))
    }
}

impl From<serde_json::Error> for BrowserError {
    fn from(err: serde_json::Error) -> Self {
        BrowserError::Internal(format!("JSON error: {}", err))
    }
}

impl From<std::io::Error> for BrowserError {
    fn from(err: std::io::Error) -> Self {
        BrowserError::Internal(format!("IO error: {}", err))
    }
}

impl From<tokio::sync::oneshot::error::RecvError> for BrowserError {
    fn from(err: tokio::sync::oneshot::error::RecvError) -> Self {
        BrowserError::IpcError(format!("Channel receive error: {}", err))
    }
}

// ---------------------------------------------------------------------------
// From conversions for crate-internal error types
// ---------------------------------------------------------------------------

impl From<crate::api::ipc::IpcError> for BrowserError {
    fn from(err: crate::api::ipc::IpcError) -> Self {
        match err {
            crate::api::ipc::IpcError::ChannelClosed => {
                BrowserError::IpcError("IPC channel closed".to_string())
            }
            crate::api::ipc::IpcError::Timeout => BrowserError::Timeout {
                operation: "IPC command".to_string(),
                duration_ms: 0,
            },
            crate::api::ipc::IpcError::InvalidMessage(msg) => {
                BrowserError::IpcError(format!("Invalid message: {}", msg))
            }
            crate::api::ipc::IpcError::CommandFailed(msg) => {
                BrowserError::IpcError(format!("Command failed: {}", msg))
            }
        }
    }
}

impl From<crate::config::ConfigError> for BrowserError {
    fn from(err: crate::config::ConfigError) -> Self {
        BrowserError::ConfigError(err.to_string())
    }
}

impl From<crate::input::InputError> for BrowserError {
    fn from(err: crate::input::InputError) -> Self {
        BrowserError::Internal(format!("Input error: {}", err))
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_tab_not_found_display() {
        let err = BrowserError::TabNotFound {
            tab_id: "abc-123".to_string(),
        };
        assert_eq!(err.to_string(), "Tab not found: abc-123");
    }

    #[test]
    fn test_navigation_failed_display() {
        let err = BrowserError::NavigationFailed {
            url: "https://example.com".to_string(),
            reason: "DNS resolution failed".to_string(),
        };
        assert_eq!(
            err.to_string(),
            "Navigation failed for https://example.com: DNS resolution failed"
        );
    }

    #[test]
    fn test_timeout_display() {
        let err = BrowserError::Timeout {
            operation: "page load".to_string(),
            duration_ms: 30000,
        };
        assert_eq!(err.to_string(), "Timeout after 30000ms: page load");
    }

    #[test]
    fn test_from_io_error() {
        let io_err = std::io::Error::new(std::io::ErrorKind::NotFound, "file missing");
        let browser_err: BrowserError = io_err.into();
        assert!(browser_err.to_string().contains("IO error"));
        assert!(browser_err.to_string().contains("file missing"));
    }

    #[test]
    fn test_from_serde_json_error() {
        let json_err = serde_json::from_str::<serde_json::Value>("not json").unwrap_err();
        let browser_err: BrowserError = json_err.into();
        assert!(browser_err.to_string().contains("JSON error"));
    }

    #[test]
    fn test_from_anyhow_error() {
        let anyhow_err = anyhow::anyhow!("something went wrong");
        let browser_err: BrowserError = anyhow_err.into();
        assert!(browser_err.to_string().contains("something went wrong"));
    }

    #[test]
    fn test_from_config_error() {
        let config_err =
            crate::config::ConfigError::ValidationError("bad value".to_string());
        let browser_err: BrowserError = config_err.into();
        assert!(browser_err.to_string().contains("bad value"));
    }

    #[test]
    fn test_from_ipc_error() {
        let ipc_err = crate::api::ipc::IpcError::ChannelClosed;
        let browser_err: BrowserError = ipc_err.into();
        assert!(browser_err.to_string().contains("IPC channel closed"));
    }

    #[test]
    fn test_from_input_error() {
        let input_err = crate::input::InputError::Timeout {
            operation: "click".to_string(),
        };
        let browser_err: BrowserError = input_err.into();
        assert!(browser_err.to_string().contains("click"));
    }

    #[test]
    fn test_browser_result_ok() {
        let result: BrowserResult<i32> = Ok(42);
        assert_eq!(result.unwrap(), 42);
    }

    #[test]
    fn test_browser_result_err() {
        let result: BrowserResult<i32> = Err(BrowserError::Internal("oops".to_string()));
        assert!(result.is_err());
    }

    #[test]
    fn test_debug_format() {
        let err = BrowserError::WebSocketError("connection reset".to_string());
        let debug = format!("{:?}", err);
        assert!(debug.contains("WebSocketError"));
        assert!(debug.contains("connection reset"));
    }

    #[test]
    fn test_all_variants_display() {
        // Ensure every variant produces a non-empty Display string.
        let variants: Vec<BrowserError> = vec![
            BrowserError::TabNotFound { tab_id: "t".into() },
            BrowserError::NavigationFailed { url: "u".into(), reason: "r".into() },
            BrowserError::DomQueryFailed { selector: "s".into(), reason: "r".into() },
            BrowserError::ScriptEvaluationFailed { reason: "r".into() },
            BrowserError::ScreenshotFailed { reason: "r".into() },
            BrowserError::IpcError("e".into()),
            BrowserError::ConfigError("e".into()),
            BrowserError::Timeout { operation: "o".into(), duration_ms: 1 },
            BrowserError::EngineError("e".into()),
            BrowserError::StealthError("e".into()),
            BrowserError::SessionError("e".into()),
            BrowserError::BatchError("e".into()),
            BrowserError::FormError("e".into()),
            BrowserError::WebSocketError("e".into()),
            BrowserError::InvalidRequest("e".into()),
            BrowserError::Internal("e".into()),
        ];
        for v in &variants {
            assert!(!v.to_string().is_empty(), "Display must not be empty for {:?}", v);
        }
    }
}
