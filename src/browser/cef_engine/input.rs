//! Mouse, keyboard, and text input methods for the CEF thread.
//!
//! Contains internal synchronous methods that send native CEF input events
//! (mouse move, click, wheel, key, text) to browser instances on the CEF
//! thread, as well as public async convenience methods on CefBrowserEngine
//! that dispatch through the command channel.

use anyhow::{anyhow, Context, Result};
use parking_lot::RwLock;
use std::collections::HashMap;
use std::sync::atomic::Ordering;
use std::sync::Arc;
use tracing::{debug, trace};
use uuid::Uuid;

use super::CefCommand;
use super::engine::CefBrowserEngine;
use super::tab::CefTab;

// ============================================================================
// Internal methods (called on the CEF thread)
// ============================================================================

/// Sends a mouse move event internally on the CEF thread.
pub(crate) fn mouse_move_internal(
    tab_id: Uuid,
    x: i32,
    y: i32,
    tabs: Arc<RwLock<HashMap<Uuid, CefTab>>>,
) -> Result<()> {
    let tabs_guard = tabs.read();
    let tab = tabs_guard
        .get(&tab_id)
        .ok_or_else(|| anyhow!("Tab not found: {}", tab_id))?;

    let browser = tab
        .browser
        .as_ref()
        .ok_or_else(|| anyhow!("Browser not initialized for tab: {}", tab_id))?;

    if let Some(host) = browser.host() {
        let event = cef::MouseEvent {
            x,
            y,
            modifiers: 0u32,
        };
        host.send_mouse_move_event(Some(&event), 0);
        trace!("Mouse move sent to tab {}: ({}, {})", tab_id, x, y);
        Ok(())
    } else {
        Err(anyhow!("No browser host for tab: {}", tab_id))
    }
}

/// Sends a mouse click event internally on the CEF thread.
///
/// The `click_count` encoding: positive values indicate mouse-down,
/// negative values indicate mouse-up. The absolute value is the actual
/// click count.
pub(crate) fn mouse_click_internal(
    tab_id: Uuid,
    x: i32,
    y: i32,
    button: i32,
    click_count: i32,
    tabs: Arc<RwLock<HashMap<Uuid, CefTab>>>,
) -> Result<()> {
    let tabs_guard = tabs.read();
    let tab = tabs_guard
        .get(&tab_id)
        .ok_or_else(|| anyhow!("Tab not found: {}", tab_id))?;

    let browser = tab
        .browser
        .as_ref()
        .ok_or_else(|| anyhow!("Browser not initialized for tab: {}", tab_id))?;

    if let Some(host) = browser.host() {
        let event = cef::MouseEvent {
            x,
            y,
            modifiers: 0u32,
        };

        // Decode click_count: positive = down, negative = up
        let mouse_up = if click_count < 0 { 1 } else { 0 };
        let actual_count = click_count.abs();

        let button_type = match button {
            0 => cef::MouseButtonType::LEFT,
            1 => cef::MouseButtonType::MIDDLE,
            2 => cef::MouseButtonType::RIGHT,
            _ => cef::MouseButtonType::LEFT,
        };

        host.send_mouse_click_event(Some(&event), button_type, mouse_up, actual_count);
        trace!(
            "Mouse click sent to tab {}: ({}, {}), button={}, up={}, count={}",
            tab_id, x, y, button, mouse_up, actual_count
        );
        Ok(())
    } else {
        Err(anyhow!("No browser host for tab: {}", tab_id))
    }
}

/// Sends a mouse wheel event internally on the CEF thread.
pub(crate) fn mouse_wheel_internal(
    tab_id: Uuid,
    x: i32,
    y: i32,
    delta_x: i32,
    delta_y: i32,
    tabs: Arc<RwLock<HashMap<Uuid, CefTab>>>,
) -> Result<()> {
    let tabs_guard = tabs.read();
    let tab = tabs_guard
        .get(&tab_id)
        .ok_or_else(|| anyhow!("Tab not found: {}", tab_id))?;

    let browser = tab
        .browser
        .as_ref()
        .ok_or_else(|| anyhow!("Browser not initialized for tab: {}", tab_id))?;

    if let Some(host) = browser.host() {
        let event = cef::MouseEvent {
            x,
            y,
            modifiers: 0u32,
        };
        host.send_mouse_wheel_event(Some(&event), delta_x, delta_y);
        trace!(
            "Mouse wheel sent to tab {}: ({}, {}), delta=({}, {})",
            tab_id, x, y, delta_x, delta_y
        );
        Ok(())
    } else {
        Err(anyhow!("No browser host for tab: {}", tab_id))
    }
}

/// Sends a keyboard event internally on the CEF thread.
///
/// Maps integer event types to CEF key event types:
/// 0 = RAWKEYDOWN, 1 = KEYDOWN, 2 = KEYUP, 3 = CHAR.
pub(crate) fn key_event_internal(
    tab_id: Uuid,
    event_type: i32,
    modifiers: u32,
    windows_key_code: i32,
    character: u16,
    tabs: Arc<RwLock<HashMap<Uuid, CefTab>>>,
) -> Result<()> {
    let tabs_guard = tabs.read();
    let tab = tabs_guard
        .get(&tab_id)
        .ok_or_else(|| anyhow!("Tab not found: {}", tab_id))?;

    let browser = tab
        .browser
        .as_ref()
        .ok_or_else(|| anyhow!("Browser not initialized for tab: {}", tab_id))?;

    if let Some(host) = browser.host() {
        let key_event_type = match event_type {
            0 => cef::KeyEventType::RAWKEYDOWN,
            1 => cef::KeyEventType::KEYDOWN,
            2 => cef::KeyEventType::KEYUP,
            3 => cef::KeyEventType::CHAR,
            _ => cef::KeyEventType::KEYDOWN,
        };

        // Use modifiers directly as u32
        let key_modifiers = modifiers;

        let event = cef::KeyEvent {
            size: std::mem::size_of::<cef::KeyEvent>(),
            type_: key_event_type,
            modifiers: key_modifiers,
            windows_key_code,
            native_key_code: 0,
            is_system_key: 0,
            character,
            unmodified_character: character,
            focus_on_editable_field: 0,
        };

        host.send_key_event(Some(&event));
        trace!(
            "Key event sent to tab {}: type={}, code={}, char={}",
            tab_id, event_type, windows_key_code, character
        );
        Ok(())
    } else {
        Err(anyhow!("No browser host for tab: {}", tab_id))
    }
}

/// Types text by sending character events internally on the CEF thread.
///
/// For each character, sends a KEYDOWN, CHAR, and KEYUP event sequence
/// to simulate realistic keyboard input.
pub(crate) fn type_text_internal(
    tab_id: Uuid,
    text: &str,
    tabs: Arc<RwLock<HashMap<Uuid, CefTab>>>,
) -> Result<()> {
    let tabs_guard = tabs.read();
    let tab = tabs_guard
        .get(&tab_id)
        .ok_or_else(|| anyhow!("Tab not found: {}", tab_id))?;

    let browser = tab
        .browser
        .as_ref()
        .ok_or_else(|| anyhow!("Browser not initialized for tab: {}", tab_id))?;

    if let Some(host) = browser.host() {
        for c in text.chars() {
            let char_code = c as u16;

            // Send KeyDown
            let key_down = cef::KeyEvent {
                size: std::mem::size_of::<cef::KeyEvent>(),
                type_: cef::KeyEventType::KEYDOWN,
                modifiers: 0u32,
                windows_key_code: char_code as i32,
                native_key_code: 0,
                is_system_key: 0,
                character: char_code,
                unmodified_character: char_code,
                focus_on_editable_field: 0,
            };
            host.send_key_event(Some(&key_down));

            // Send Char event
            let char_event = cef::KeyEvent {
                size: std::mem::size_of::<cef::KeyEvent>(),
                type_: cef::KeyEventType::CHAR,
                modifiers: 0u32,
                windows_key_code: char_code as i32,
                native_key_code: 0,
                is_system_key: 0,
                character: char_code,
                unmodified_character: char_code,
                focus_on_editable_field: 0,
            };
            host.send_key_event(Some(&char_event));

            // Send KeyUp
            let key_up = cef::KeyEvent {
                size: std::mem::size_of::<cef::KeyEvent>(),
                type_: cef::KeyEventType::KEYUP,
                modifiers: 0u32,
                windows_key_code: char_code as i32,
                native_key_code: 0,
                is_system_key: 0,
                character: char_code,
                unmodified_character: char_code,
                focus_on_editable_field: 0,
            };
            host.send_key_event(Some(&key_up));
        }

        debug!("Typed text on tab {}: {} chars", tab_id, text.len());
        Ok(())
    } else {
        Err(anyhow!("No browser host for tab: {}", tab_id))
    }
}

// ============================================================================
// Public async API on CefBrowserEngine
// ============================================================================

impl CefBrowserEngine {
    /// Clicks at the specified coordinates in a tab.
    ///
    /// Sends a mouse-down event followed by a 50ms delay and a mouse-up event
    /// to simulate a realistic click at the given position.
    pub async fn click(&self, tab_id: Uuid, x: i32, y: i32, button: i32) -> Result<()> {
        if !self.is_running.load(Ordering::SeqCst) {
            return Err(anyhow!("Browser engine is not running"));
        }

        // Mouse down
        let (response_tx, response_rx) = tokio::sync::oneshot::channel();
        self.command_tx
            .send(CefCommand::MouseClick {
                tab_id,
                x,
                y,
                button,
                click_count: 1, // Positive = down
                response: response_tx,
            })
            .await
            .context("Failed to send mouse down command")?;
        response_rx
            .await
            .context("Failed to receive mouse down response")??;

        // Small delay between down and up
        tokio::time::sleep(tokio::time::Duration::from_millis(50)).await;

        // Mouse up
        let (response_tx, response_rx) = tokio::sync::oneshot::channel();
        self.command_tx
            .send(CefCommand::MouseClick {
                tab_id,
                x,
                y,
                button,
                click_count: -1, // Negative = up
                response: response_tx,
            })
            .await
            .context("Failed to send mouse up command")?;
        response_rx
            .await
            .context("Failed to receive mouse up response")?
    }

    /// Types text in the currently focused element of a tab.
    ///
    /// Sends the text as a sequence of key events to the CEF thread.
    pub async fn type_text(&self, tab_id: Uuid, text: &str) -> Result<()> {
        if !self.is_running.load(Ordering::SeqCst) {
            return Err(anyhow!("Browser engine is not running"));
        }

        let (response_tx, response_rx) = tokio::sync::oneshot::channel();
        self.command_tx
            .send(CefCommand::TypeText {
                tab_id,
                text: text.to_string(),
                response: response_tx,
            })
            .await
            .context("Failed to send type text command")?;

        response_rx
            .await
            .context("Failed to receive type text response")?
    }

    /// Scrolls at the specified position in a tab.
    ///
    /// Sends a mouse wheel event with the given deltas to the CEF thread.
    pub async fn scroll(
        &self,
        tab_id: Uuid,
        x: i32,
        y: i32,
        delta_x: i32,
        delta_y: i32,
    ) -> Result<()> {
        if !self.is_running.load(Ordering::SeqCst) {
            return Err(anyhow!("Browser engine is not running"));
        }

        let (response_tx, response_rx) = tokio::sync::oneshot::channel();
        self.command_tx
            .send(CefCommand::MouseWheel {
                tab_id,
                x,
                y,
                delta_x,
                delta_y,
                response: response_tx,
            })
            .await
            .context("Failed to send scroll command")?;

        response_rx
            .await
            .context("Failed to receive scroll response")?
    }

    /// Moves the mouse to the specified coordinates in a tab.
    ///
    /// Sends a mouse move event to the CEF thread.
    pub async fn mouse_move(&self, tab_id: Uuid, x: i32, y: i32) -> Result<()> {
        if !self.is_running.load(Ordering::SeqCst) {
            return Err(anyhow!("Browser engine is not running"));
        }

        let (response_tx, response_rx) = tokio::sync::oneshot::channel();
        self.command_tx
            .send(CefCommand::MouseMove {
                tab_id,
                x,
                y,
                response: response_tx,
            })
            .await
            .context("Failed to send mouse move command")?;

        response_rx
            .await
            .context("Failed to receive mouse move response")?
    }
}
