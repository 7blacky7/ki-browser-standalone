//! CEF-specific input simulation for native browser events.
//!
//! This module provides native input simulation for CEF (Chromium Embedded Framework)
//! browsers, handling mouse and keyboard events through CEF's native event structures.
//!
//! # Submodules
//!
//! - [`events`] - CEF event type definitions (CefMouseEvent, CefKeyEvent, EVENTFLAG_* constants)
//! - [`keyboard`] - Platform key code tables (Windows VK_*, Linux XKB) and conversion utilities
//! - [`mouse`] - CefInputHandler struct, CefEventSender trait, and all mouse input methods
//! - [`keyboard_handler`] - Keyboard input methods on CefInputHandler (text, combos, key events)
//!
//! # Features
//!
//! - Mouse input with human-like Bezier curve movement paths and micro-jitter
//! - Keyboard input with realistic per-character typing delays
//! - Platform-specific key code handling (Windows VK_* / Linux XKB keysyms)
//! - Integration with the input module's timing and Bezier utilities
//!
//! # Example
//!
//! ```rust,ignore
//! use ki_browser::browser::cef_input::{CefInputHandler, CefMouseButton};
//! use ki_browser::input::HumanTiming;
//!
//! async fn example(sender: impl CefEventSender) {
//!     let timing = HumanTiming::normal();
//!     let mut handler = CefInputHandler::new(sender, timing);
//!
//!     // Move mouse with human-like Bezier path
//!     handler.send_mouse_move(500.0, 300.0).await.unwrap();
//!
//!     // Click at position
//!     handler.send_mouse_click(500.0, 300.0, CefMouseButton::Left).await.unwrap();
//!
//!     // Type text with human-like delays
//!     handler.send_text("Hello, World!").await.unwrap();
//! }
//! ```

pub mod events;
pub mod keyboard;
pub mod keyboard_handler;
pub mod mouse;

// Re-export all public types for backward-compatible access via `cef_input::*`
pub use events::{CefKeyEvent, CefKeyEventType, CefMouseButton, CefMouseEvent};
pub use mouse::{CefEventSender, CefInputConfig, CefInputHandler};

// Re-export key code tables so downstream code can use `cef_input::key_codes::VK_*`
pub use keyboard::key_codes;

// ============================================================================
// Mock Event Sender (test utility, available in test builds)
// ============================================================================

/// Mock `CefEventSender` that records all delivered events for assertion in tests.
///
/// Used in unit tests for `CefInputHandler` to verify correct event sequencing
/// without requiring a live CEF browser instance.
#[cfg(test)]
pub struct MockCefEventSender {
    pub mouse_moves: std::sync::Mutex<Vec<CefMouseEvent>>,
    pub mouse_clicks: std::sync::Mutex<Vec<(CefMouseEvent, CefMouseButton, bool, i32)>>,
    pub mouse_wheels: std::sync::Mutex<Vec<(CefMouseEvent, i32, i32)>>,
    pub key_events: std::sync::Mutex<Vec<CefKeyEvent>>,
}

#[cfg(test)]
impl MockCefEventSender {
    /// Creates a new empty mock sender.
    pub fn new() -> Self {
        Self {
            mouse_moves: std::sync::Mutex::new(Vec::new()),
            mouse_clicks: std::sync::Mutex::new(Vec::new()),
            mouse_wheels: std::sync::Mutex::new(Vec::new()),
            key_events: std::sync::Mutex::new(Vec::new()),
        }
    }
}

#[cfg(test)]
impl CefEventSender for MockCefEventSender {
    fn send_mouse_move_event(&self, event: &CefMouseEvent, _mouse_leave: bool) {
        self.mouse_moves.lock().unwrap().push(*event);
    }

    fn send_mouse_click_event(
        &self,
        event: &CefMouseEvent,
        button: CefMouseButton,
        mouse_up: bool,
        click_count: i32,
    ) {
        self.mouse_clicks
            .lock()
            .unwrap()
            .push((*event, button, mouse_up, click_count));
    }

    fn send_mouse_wheel_event(&self, event: &CefMouseEvent, delta_x: i32, delta_y: i32) {
        self.mouse_wheels.lock().unwrap().push((*event, delta_x, delta_y));
    }

    fn send_key_event(&self, event: &CefKeyEvent) {
        self.key_events.lock().unwrap().push(event.clone());
    }
}
