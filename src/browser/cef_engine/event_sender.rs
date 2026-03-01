//! Bridge between CefInputHandler and CefBrowserEngine command channel.
//!
//! Provides `CefBrowserEventSender` which implements the `CefEventSender` trait
//! to forward mouse and keyboard events from the input handler to the CEF message
//! loop thread via the command channel.

use tokio::sync::{mpsc, oneshot};
use uuid::Uuid;

use super::CefCommand;

/// Event sender implementation for connecting CefInputHandler to CefBrowserEngine.
///
/// This struct bridges the input handler with the CEF browser by sending
/// input events through the command channel to be processed on the CEF thread.
pub struct CefBrowserEventSender {
    /// Tab ID this sender is associated with.
    tab_id: Uuid,
    /// Command sender for the CEF message loop (unbounded = never drops).
    command_tx: mpsc::UnboundedSender<CefCommand>,
}

impl CefBrowserEventSender {
    /// Creates a new event sender for a specific tab.
    pub(crate) fn new(tab_id: Uuid, command_tx: mpsc::UnboundedSender<CefCommand>) -> Self {
        Self {
            tab_id,
            command_tx,
        }
    }
}

impl crate::browser::cef_input::CefEventSender for CefBrowserEventSender {
    fn send_mouse_move_event(&self, event: &crate::browser::cef_input::CefMouseEvent, _mouse_leave: bool) {
        let (tx, _rx) = oneshot::channel();
        let cmd = CefCommand::MouseMove {
            tab_id: self.tab_id,
            x: event.x,
            y: event.y,
            response: tx,
        };
        // Fire and forget - we don't wait for response on move events
        let _ = self.command_tx.send(cmd);
    }

    fn send_mouse_click_event(
        &self,
        event: &crate::browser::cef_input::CefMouseEvent,
        button: crate::browser::cef_input::CefMouseButton,
        mouse_up: bool,
        click_count: i32,
    ) {
        let (tx, _rx) = oneshot::channel();
        // Encode mouse_up in the click_count (negative = up)
        let encoded_count = if mouse_up { -click_count } else { click_count };
        let cmd = CefCommand::MouseClick {
            tab_id: self.tab_id,
            x: event.x,
            y: event.y,
            button: button.to_cef_type(),
            click_count: encoded_count,
            response: tx,
        };
        let _ = self.command_tx.send(cmd);
    }

    fn send_mouse_wheel_event(&self, event: &crate::browser::cef_input::CefMouseEvent, delta_x: i32, delta_y: i32) {
        let (tx, _rx) = oneshot::channel();
        let cmd = CefCommand::MouseWheel {
            tab_id: self.tab_id,
            x: event.x,
            y: event.y,
            delta_x,
            delta_y,
            response: tx,
        };
        let _ = self.command_tx.send(cmd);
    }

    fn send_key_event(&self, event: &crate::browser::cef_input::CefKeyEvent) {
        let (tx, _rx) = oneshot::channel();
        let cmd = CefCommand::KeyEvent {
            tab_id: self.tab_id,
            event_type: event.event_type.to_cef_type(),
            modifiers: event.modifiers,
            windows_key_code: event.windows_key_code,
            character: event.character,
            response: tx,
        };
        let _ = self.command_tx.send(cmd);
    }
}
