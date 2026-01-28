//! Input simulation module for ki-browser-standalone
//!
//! This module provides human-like input simulation for browser automation,
//! including mouse movements, keyboard input, and realistic timing patterns.
//!
//! # Submodules
//!
//! - [`mouse`] - Mouse event simulation with realistic movement patterns
//! - [`keyboard`] - Keyboard input simulation with modifier key support
//! - [`bezier`] - Bézier curve implementation for natural mouse paths
//! - [`timing`] - Human-like timing utilities based on behavioral studies
//!
//! # Example
//!
//! ```rust,no_run
//! use ki_browser::input::{MouseSimulator, KeyboardSimulator, HumanTiming, MouseButton};
//!
//! async fn example() {
//!     let mut mouse = MouseSimulator::new();
//!     let keyboard = KeyboardSimulator::new();
//!     let _timing = HumanTiming::default();
//!
//!     // Move mouse with human-like path
//!     mouse.move_to(500.0, 300.0).await.unwrap();
//!
//!     // Click and type
//!     mouse.click(MouseButton::Left).await.unwrap();
//!     keyboard.type_text("Hello, World!").await.unwrap();
//! }
//! ```

pub mod bezier;
pub mod keyboard;
pub mod mouse;
pub mod timing;

// Re-export commonly used types for convenience
pub use bezier::{BezierCurve, Point};
pub use keyboard::{KeyboardEvent, KeyboardSimulator, Modifier};
pub use mouse::{MouseButton, MouseEvent, MouseSimulator};
pub use timing::HumanTiming;

/// Result type for input operations
pub type InputResult<T> = Result<T, InputError>;

/// Errors that can occur during input simulation
#[derive(Debug, Clone, PartialEq)]
pub enum InputError {
    /// The specified coordinates are out of bounds
    OutOfBounds { x: f64, y: f64 },
    /// The input operation timed out
    Timeout { operation: String },
    /// An invalid key was specified
    InvalidKey { key: String },
    /// The input device is not available
    DeviceUnavailable { device: String },
    /// A platform-specific error occurred
    PlatformError { message: String },
    /// The operation was cancelled
    Cancelled,
}

impl std::fmt::Display for InputError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            InputError::OutOfBounds { x, y } => {
                write!(f, "Coordinates ({}, {}) are out of bounds", x, y)
            }
            InputError::Timeout { operation } => {
                write!(f, "Operation '{}' timed out", operation)
            }
            InputError::InvalidKey { key } => {
                write!(f, "Invalid key: '{}'", key)
            }
            InputError::DeviceUnavailable { device } => {
                write!(f, "Device '{}' is not available", device)
            }
            InputError::PlatformError { message } => {
                write!(f, "Platform error: {}", message)
            }
            InputError::Cancelled => {
                write!(f, "Operation was cancelled")
            }
        }
    }
}

impl std::error::Error for InputError {}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_error_display() {
        let err = InputError::OutOfBounds { x: 100.0, y: 200.0 };
        assert!(err.to_string().contains("100"));
        assert!(err.to_string().contains("200"));

        let err = InputError::InvalidKey {
            key: "INVALID".to_string(),
        };
        assert!(err.to_string().contains("INVALID"));
    }
}
