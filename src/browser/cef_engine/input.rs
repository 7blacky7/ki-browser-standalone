//! Mouse, keyboard, and text input methods for the CEF thread.
//!
//! Contains internal synchronous methods that send native CEF input events
//! (mouse move, click, wheel, key, text, drag) to browser instances on the CEF
//! thread, as well as public async convenience methods on CefBrowserEngine
//! that dispatch through the command channel.

use anyhow::{anyhow, Context, Result};
use cef::{ImplBrowser, ImplBrowserHost};
use parking_lot::RwLock;
use std::collections::HashMap;
use std::sync::atomic::Ordering;
use std::sync::Arc;
use tokio::sync::oneshot;
use tracing::{debug, info, trace};
use uuid::Uuid;

use super::CefCommand;
use super::engine::CefBrowserEngine;
use super::tab::CefTab;
use crate::input::bezier::{generate_human_path, Point};
use crate::input::timing::HumanTiming;

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
    // Clone browser ref and release read lock BEFORE calling CEF methods
    // (CEF callbacks may need write lock on same thread -> deadlock prevention)
    let browser = {
        let tabs_guard = tabs.read();
        let tab = tabs_guard
            .get(&tab_id)
            .ok_or_else(|| anyhow!("Tab not found: {}", tab_id))?;
        tab.browser.clone()
            .ok_or_else(|| anyhow!("Browser not initialized for tab: {}", tab_id))?
    }; // Read lock released here.

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
    // Clone browser ref and release read lock BEFORE calling CEF methods
    let browser = {
        let tabs_guard = tabs.read();
        let tab = tabs_guard
            .get(&tab_id)
            .ok_or_else(|| anyhow!("Tab not found: {}", tab_id))?;
        tab.browser.clone()
            .ok_or_else(|| anyhow!("Browser not initialized for tab: {}", tab_id))?
    }; // Read lock released here.

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
    // Clone browser ref and release read lock BEFORE calling CEF methods
    let browser = {
        let tabs_guard = tabs.read();
        let tab = tabs_guard
            .get(&tab_id)
            .ok_or_else(|| anyhow!("Tab not found: {}", tab_id))?;
        tab.browser.clone()
            .ok_or_else(|| anyhow!("Browser not initialized for tab: {}", tab_id))?
    }; // Read lock released here.

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

/// Simulates a drag-and-drop by sending mousedown, mousemoves, mouseup.
#[allow(clippy::too_many_arguments)]
pub(crate) fn drag_internal(
    tab_id: Uuid,
    from_x: i32,
    from_y: i32,
    to_x: i32,
    to_y: i32,
    steps: u32,
    duration_ms: u64,
    tabs: Arc<RwLock<HashMap<Uuid, CefTab>>>,
) -> Result<()> {
    let browser = {
        let tabs_guard = tabs.read();
        let tab = tabs_guard
            .get(&tab_id)
            .ok_or_else(|| anyhow!("Tab not found: {}", tab_id))?;
        tab.browser.clone()
            .ok_or_else(|| anyhow!("Browser not initialized for tab: {}", tab_id))?
    };

    if let Some(host) = browser.host() {
        let step_delay = if steps > 0 {
            std::time::Duration::from_millis(duration_ms / steps as u64)
        } else {
            std::time::Duration::from_millis(10)
        };

        // 1. Move to start position
        let start_event = cef::MouseEvent { x: from_x, y: from_y, modifiers: 0u32 };
        host.send_mouse_move_event(Some(&start_event), 0);
        std::thread::sleep(std::time::Duration::from_millis(50));

        // 2. Mouse down at start
        host.send_mouse_click_event(Some(&start_event), cef::MouseButtonType::LEFT, 0, 1);
        std::thread::sleep(std::time::Duration::from_millis(100));

        // 3. Intermediate moves (with left button held = modifier bit 5)
        let left_button_down: u32 = 1 << 5; // EVENTFLAG_LEFT_MOUSE_BUTTON
        let actual_steps = steps.max(1);
        for i in 1..=actual_steps {
            let t = i as f64 / actual_steps as f64;
            let cx = from_x + ((to_x - from_x) as f64 * t) as i32;
            let cy = from_y + ((to_y - from_y) as f64 * t) as i32;
            let move_event = cef::MouseEvent { x: cx, y: cy, modifiers: left_button_down };
            host.send_mouse_move_event(Some(&move_event), 0);
            std::thread::sleep(step_delay);
        }

        // 4. Mouse up at end
        std::thread::sleep(std::time::Duration::from_millis(50));
        let end_event = cef::MouseEvent { x: to_x, y: to_y, modifiers: left_button_down };
        host.send_mouse_click_event(Some(&end_event), cef::MouseButtonType::LEFT, 1, 1);

        info!("Drag on tab {}: ({},{}) -> ({},{}) in {} steps", tab_id, from_x, from_y, to_x, to_y, actual_steps);
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
    // Clone browser ref and release read lock BEFORE calling CEF methods
    let browser = {
        let tabs_guard = tabs.read();
        let tab = tabs_guard
            .get(&tab_id)
            .ok_or_else(|| anyhow!("Tab not found: {}", tab_id))?;
        tab.browser.clone()
            .ok_or_else(|| anyhow!("Browser not initialized for tab: {}", tab_id))?
    }; // Read lock released here.

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

/// Maps a character to its Windows Virtual Key code for KEYDOWN/KEYUP events.
/// Without this, characters like '.' (char code 46 = VK_DELETE!) would be
/// misinterpreted as control keys.
fn char_to_vk_code(c: char) -> (i32, u32) {
    // Returns (vk_code, modifiers). Shift = 1 << 1 = 2 (EVENTFLAG_SHIFT_DOWN).
    const SHIFT: u32 = 2;
    match c {
        'a'..='z' => ((c as u8 - b'a' + b'A') as i32, 0),
        'A'..='Z' => (c as i32, SHIFT),
        '0'..='9' => (c as i32, 0),
        ' ' => (0x20, 0),        // VK_SPACE
        '\r' | '\n' => (0x0D, 0), // VK_RETURN
        '\t' => (0x09, 0),       // VK_TAB
        '.' => (190, 0),         // VK_OEM_PERIOD  (NOT 46 = VK_DELETE!)
        ',' => (188, 0),         // VK_OEM_COMMA
        '-' => (189, 0),         // VK_OEM_MINUS
        '=' => (187, 0),         // VK_OEM_PLUS (unshifted)
        ';' => (186, 0),         // VK_OEM_1
        '\'' => (222, 0),        // VK_OEM_7
        '/' => (191, 0),         // VK_OEM_2
        '\\' => (220, 0),        // VK_OEM_5
        '[' => (219, 0),         // VK_OEM_4
        ']' => (221, 0),         // VK_OEM_6
        '`' => (192, 0),         // VK_OEM_3
        // Shifted variants
        '!' => (0x31, SHIFT),    // Shift+1
        '@' => (0x32, SHIFT),    // Shift+2
        '#' => (0x33, SHIFT),    // Shift+3
        '$' => (0x34, SHIFT),    // Shift+4
        '%' => (0x35, SHIFT),    // Shift+5
        '^' => (0x36, SHIFT),    // Shift+6
        '&' => (0x37, SHIFT),    // Shift+7
        '*' => (0x38, SHIFT),    // Shift+8
        '(' => (0x39, SHIFT),    // Shift+9
        ')' => (0x30, SHIFT),    // Shift+0
        '_' => (189, SHIFT),     // Shift+minus
        '+' => (187, SHIFT),     // Shift+=
        ':' => (186, SHIFT),     // Shift+;
        '"' => (222, SHIFT),     // Shift+'
        '?' => (191, SHIFT),     // Shift+/
        '>' => (190, SHIFT),     // Shift+.
        '<' => (188, SHIFT),     // Shift+,
        '|' => (220, SHIFT),     // Shift+backslash
        '{' => (219, SHIFT),     // Shift+[
        '}' => (221, SHIFT),     // Shift+]
        '~' => (192, SHIFT),     // Shift+`
        // Fallback: use char code directly (works for basic ASCII)
        _ => (c as i32, 0),
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
    // Clone browser ref and release read lock BEFORE calling CEF methods
    let browser = {
        let tabs_guard = tabs.read();
        let tab = tabs_guard
            .get(&tab_id)
            .ok_or_else(|| anyhow!("Tab not found: {}", tab_id))?;
        tab.browser.clone()
            .ok_or_else(|| anyhow!("Browser not initialized for tab: {}", tab_id))?
    }; // Read lock released here.

    if let Some(host) = browser.host() {
        for c in text.chars() {
            let char_code = c as u16;
            let (vk_code, modifiers) = char_to_vk_code(c);

            // Send KeyDown (uses VK code, not char code!)
            let key_down = cef::KeyEvent {
                size: std::mem::size_of::<cef::KeyEvent>(),
                type_: cef::KeyEventType::KEYDOWN,
                modifiers,
                windows_key_code: vk_code,
                native_key_code: 0,
                is_system_key: 0,
                character: char_code,
                unmodified_character: char_code,
                focus_on_editable_field: 0,
            };
            host.send_key_event(Some(&key_down));

            // Send Char event (uses char code -- this is what produces text input)
            let char_event = cef::KeyEvent {
                size: std::mem::size_of::<cef::KeyEvent>(),
                type_: cef::KeyEventType::CHAR,
                modifiers,
                windows_key_code: char_code as i32,
                native_key_code: 0,
                is_system_key: 0,
                character: char_code,
                unmodified_character: char_code,
                focus_on_editable_field: 0,
            };
            host.send_key_event(Some(&char_event));

            // Send KeyUp (uses VK code, not char code!)
            let key_up = cef::KeyEvent {
                size: std::mem::size_of::<cef::KeyEvent>(),
                type_: cef::KeyEventType::KEYUP,
                modifiers,
                windows_key_code: vk_code,
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
    /// Simulates a REAL user click (E2E-tool grade): the cursor approaches the
    /// target along a Bézier curve (no teleport), settles, presses with a
    /// human dwell time and releases — all delays jittered via `HumanTiming`.
    /// Pacing runs on the tokio side, so the CEF message loop is never blocked
    /// between events. This is the KI/API path (/click); the viewer WS path
    /// forwards the user's raw events 1:1 and stays strictly separate.
    pub async fn click(&self, tab_id: Uuid, x: i32, y: i32, button: i32) -> Result<()> {
        if !self.is_running.load(Ordering::SeqCst) {
            return Err(anyhow!("Browser engine is not running"));
        }

        let timing = HumanTiming::fast();
        let target = Point::new(x as f64, y as f64);

        // Approach start: last known cursor position for this tab, or a
        // plausible nearby point on first contact (never a 0,0 teleport).
        let start = {
            let positions = self.last_mouse_pos.lock();
            positions
                .get(&tab_id)
                .map(|&(px, py)| Point::new(px as f64, py as f64))
        }
        .unwrap_or_else(|| Point::new((x as f64 - 180.0).max(0.0), (y as f64 - 120.0).max(0.0)));

        // Bézier approach, density scaled by distance (~1 point per 25px).
        let distance = start.distance_to(&target);
        if distance >= 2.0 {
            let num_points = ((distance / 25.0) as usize).clamp(6, 30);
            for p in generate_human_path(start, target, num_points) {
                self.mouse_move_and_wait(tab_id, p.x.round() as i32, p.y.round() as i32)
                    .await?;
                tokio::time::sleep(timing.get_move_delay()).await;
            }
        }
        // Land exactly on the target (path points are jittered/rounded).
        self.mouse_move_and_wait(tab_id, x, y).await?;
        tokio::time::sleep(timing.get_move_delay()).await;

        // Press, human dwell, release.
        self.mouse_button_and_wait(tab_id, x, y, button, 1).await?;
        tokio::time::sleep(timing.get_click_delay()).await;
        self.mouse_button_and_wait(tab_id, x, y, button, -1).await?;

        self.last_mouse_pos.lock().insert(tab_id, (x, y));
        Ok(())
    }

    /// Sends a mouse move via the command channel and awaits delivery.
    async fn mouse_move_and_wait(&self, tab_id: Uuid, x: i32, y: i32) -> Result<()> {
        let (response_tx, response_rx) = oneshot::channel();
        self.command_tx
            .send(CefCommand::MouseMove {
                tab_id,
                x,
                y,
                response: response_tx,
            })
            .map_err(|_| anyhow!("Failed to send mouse move command"))?;
        response_rx.await.context("Failed to receive mouse move response")?
    }

    /// Sends one button transition (down: positive, up: negative click_count)
    /// via the command channel and awaits delivery.
    async fn mouse_button_and_wait(&self, tab_id: Uuid, x: i32, y: i32, button: i32, click_count: i32) -> Result<()> {
        let (response_tx, response_rx) = oneshot::channel();
        self.command_tx
            .send(CefCommand::MouseClick {
                tab_id,
                x,
                y,
                button,
                click_count,
                response: response_tx,
            })
            .map_err(|_| anyhow!("Failed to send mouse button command"))?;
        response_rx.await.context("Failed to receive mouse button response")?
    }

    /// Types text in the currently focused element of a tab.
    pub async fn type_text(&self, tab_id: Uuid, text: &str) -> Result<()> {
        if !self.is_running.load(Ordering::SeqCst) {
            return Err(anyhow!("Browser engine is not running"));
        }

        let (response_tx, response_rx) = oneshot::channel();
        self.command_tx
            .send(CefCommand::TypeText {
                tab_id,
                text: text.to_string(),
                response: response_tx,
            })
            .map_err(|_| anyhow!("Failed to send type text command"))?;

        response_rx.await.context("Failed to receive type text response")?
    }

    /// Scrolls at the specified position in a tab.
    pub async fn scroll(&self, tab_id: Uuid, x: i32, y: i32, delta_x: i32, delta_y: i32) -> Result<()> {
        if !self.is_running.load(Ordering::SeqCst) {
            return Err(anyhow!("Browser engine is not running"));
        }

        let (response_tx, response_rx) = oneshot::channel();
        self.command_tx
            .send(CefCommand::MouseWheel {
                tab_id,
                x,
                y,
                delta_x,
                delta_y,
                response: response_tx,
            })
            .map_err(|_| anyhow!("Failed to send scroll command"))?;

        response_rx.await.context("Failed to receive scroll response")?
    }

    /// Moves the mouse to the specified coordinates in a tab.
    pub async fn mouse_move(&self, tab_id: Uuid, x: i32, y: i32) -> Result<()> {
        if !self.is_running.load(Ordering::SeqCst) {
            return Err(anyhow!("Browser engine is not running"));
        }

        let (response_tx, response_rx) = oneshot::channel();
        self.command_tx
            .send(CefCommand::MouseMove {
                tab_id,
                x,
                y,
                response: response_tx,
            })
            .map_err(|_| anyhow!("Failed to send mouse move command"))?;

        response_rx.await.context("Failed to receive mouse move response")?
    }

    /// Performs a drag operation from one point to another.
    #[allow(clippy::too_many_arguments)]
    pub async fn drag(&self, tab_id: Uuid, from_x: i32, from_y: i32, to_x: i32, to_y: i32, steps: u32, duration_ms: u64) -> Result<()> {
        if !self.is_running.load(Ordering::SeqCst) {
            return Err(anyhow!("Browser engine is not running"));
        }

        let (response_tx, response_rx) = oneshot::channel();
        self.command_tx
            .send(CefCommand::Drag {
                tab_id,
                from_x,
                from_y,
                to_x,
                to_y,
                steps,
                duration_ms,
                response: response_tx,
            })
            .map_err(|_| anyhow!("Failed to send drag command"))?;

        response_rx.await.context("Failed to receive drag response")?
    }
}
