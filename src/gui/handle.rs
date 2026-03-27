//! Shared GUI handle for cross-thread window visibility control and shutdown signaling.
//!
//! The `GuiHandle` is created before the GUI event loop starts and shared with
//! the API server via `AppState`. REST endpoints use it to show/hide the window
//! and to request a graceful shutdown. The `GuiVisibility` enum tracks the
//! current window state (visible, hidden, or disabled in headless mode).

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use parking_lot::RwLock;

/// Current visibility state of the GUI window.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "lowercase")]
pub enum GuiVisibility {
    /// Window is visible on screen.
    Visible,
    /// Window is hidden (minimized to tray / not shown).
    Hidden,
    /// GUI was never started (headless mode).
    Disabled,
}

/// Shared handle for controlling the GUI window from outside (e.g. REST API).
///
/// Thread-safe: all fields are atomic or behind `parking_lot::RwLock`.
/// The `egui_ctx` is stored so that API endpoints can request a repaint
/// after changing visibility, which wakes the event loop.
pub struct GuiHandle {
    /// Current visibility state of the GUI window.
    visibility: RwLock<GuiVisibility>,
    /// Signals that a shutdown has been requested (close button, SIGTERM, etc.).
    shutdown_requested: AtomicBool,
    /// Signals that the GUI event loop has fully exited.
    shutdown_complete: AtomicBool,
    /// egui context for requesting repaints from outside the event loop.
    egui_ctx: RwLock<Option<egui::Context>>,
}

impl GuiHandle {
    /// Creates a new handle for a running GUI (visibility starts as `Visible`).
    pub fn new() -> Arc<Self> {
        Arc::new(Self {
            visibility: RwLock::new(GuiVisibility::Visible),
            shutdown_requested: AtomicBool::new(false),
            shutdown_complete: AtomicBool::new(false),
            egui_ctx: RwLock::new(None),
        })
    }

    /// Creates a handle for headless mode (no GUI window exists).
    pub fn disabled() -> Arc<Self> {
        Arc::new(Self {
            visibility: RwLock::new(GuiVisibility::Disabled),
            shutdown_requested: AtomicBool::new(false),
            shutdown_complete: AtomicBool::new(false),
            egui_ctx: RwLock::new(None),
        })
    }

    /// Returns the current GUI visibility state.
    pub fn visibility(&self) -> GuiVisibility {
        *self.visibility.read()
    }

    /// Show the GUI window (sets visibility to `Visible` and requests repaint).
    pub fn show(&self) {
        let mut vis = self.visibility.write();
        if *vis != GuiVisibility::Disabled {
            *vis = GuiVisibility::Visible;
            drop(vis);
            self.request_repaint();
        }
    }

    /// Hide the GUI window (sets visibility to `Hidden` and requests repaint).
    pub fn hide(&self) {
        let mut vis = self.visibility.write();
        if *vis != GuiVisibility::Disabled {
            *vis = GuiVisibility::Hidden;
            drop(vis);
            self.request_repaint();
        }
    }

    /// Toggle between `Visible` and `Hidden`. No-op if `Disabled`.
    pub fn toggle(&self) -> GuiVisibility {
        let mut vis = self.visibility.write();
        match *vis {
            GuiVisibility::Visible => *vis = GuiVisibility::Hidden,
            GuiVisibility::Hidden => *vis = GuiVisibility::Visible,
            GuiVisibility::Disabled => {}
        }
        let result = *vis;
        drop(vis);
        self.request_repaint();
        result
    }

    /// Request a graceful shutdown from outside the GUI event loop.
    pub fn request_shutdown(&self) {
        self.shutdown_requested.store(true, Ordering::SeqCst);
        self.request_repaint();
    }

    /// Check if shutdown has been requested.
    pub fn is_shutdown_requested(&self) -> bool {
        self.shutdown_requested.load(Ordering::SeqCst)
    }

    /// Check if the GUI event loop has fully exited.
    pub fn is_shutdown_complete(&self) -> bool {
        self.shutdown_complete.load(Ordering::SeqCst)
    }

    /// Store the egui context so external callers can trigger repaints.
    pub(crate) fn set_egui_ctx(&self, ctx: egui::Context) {
        *self.egui_ctx.write() = Some(ctx);
    }

    /// Wake the event loop so it picks up visibility/shutdown changes.
    fn request_repaint(&self) {
        if let Some(ctx) = self.egui_ctx.read().as_ref() {
            ctx.request_repaint();
        }
    }

    /// Mark the GUI as fully shut down. Called once after cleanup.
    pub(crate) fn mark_shutdown_complete(&self) {
        self.shutdown_complete.store(true, Ordering::SeqCst);
        *self.visibility.write() = GuiVisibility::Disabled;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_gui_handle_new_starts_visible() {
        let handle = GuiHandle::new();
        assert_eq!(handle.visibility(), GuiVisibility::Visible);
        assert!(!handle.is_shutdown_requested());
        assert!(!handle.is_shutdown_complete());
    }

    #[test]
    fn test_gui_handle_disabled_state() {
        let handle = GuiHandle::disabled();
        assert_eq!(handle.visibility(), GuiVisibility::Disabled);
    }

    #[test]
    fn test_gui_handle_toggle_visible_to_hidden() {
        let handle = GuiHandle::new();
        let new_vis = handle.toggle();
        assert_eq!(new_vis, GuiVisibility::Hidden);
        assert_eq!(handle.visibility(), GuiVisibility::Hidden);
    }

    #[test]
    fn test_gui_handle_toggle_hidden_to_visible() {
        let handle = GuiHandle::new();
        handle.hide();
        let new_vis = handle.toggle();
        assert_eq!(new_vis, GuiVisibility::Visible);
        assert_eq!(handle.visibility(), GuiVisibility::Visible);
    }

    #[test]
    fn test_gui_handle_toggle_disabled_stays_disabled() {
        let handle = GuiHandle::disabled();
        let new_vis = handle.toggle();
        assert_eq!(new_vis, GuiVisibility::Disabled);
    }

    #[test]
    fn test_gui_handle_show_hide() {
        let handle = GuiHandle::new();
        handle.hide();
        assert_eq!(handle.visibility(), GuiVisibility::Hidden);
        handle.show();
        assert_eq!(handle.visibility(), GuiVisibility::Visible);
    }

    #[test]
    fn test_gui_handle_show_disabled_noop() {
        let handle = GuiHandle::disabled();
        handle.show();
        assert_eq!(handle.visibility(), GuiVisibility::Disabled);
    }

    #[test]
    fn test_gui_handle_shutdown_request() {
        let handle = GuiHandle::new();
        assert!(!handle.is_shutdown_requested());
        handle.request_shutdown();
        assert!(handle.is_shutdown_requested());
    }

    #[test]
    fn test_gui_handle_shutdown_complete() {
        let handle = GuiHandle::new();
        assert!(!handle.is_shutdown_complete());
        handle.mark_shutdown_complete();
        assert!(handle.is_shutdown_complete());
        assert_eq!(handle.visibility(), GuiVisibility::Disabled);
    }

    #[test]
    fn test_gui_visibility_serialization() {
        let json = serde_json::to_string(&GuiVisibility::Visible).unwrap();
        assert_eq!(json, "\"visible\"");

        let json = serde_json::to_string(&GuiVisibility::Hidden).unwrap();
        assert_eq!(json, "\"hidden\"");

        let json = serde_json::to_string(&GuiVisibility::Disabled).unwrap();
        assert_eq!(json, "\"disabled\"");
    }
}
