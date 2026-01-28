//! CEF-specific input simulation for native browser events.
//!
//! This module provides native input simulation for CEF (Chromium Embedded Framework)
//! browsers, handling mouse and keyboard events through CEF's native event structures.
//!
//! # Features
//!
//! - Mouse input with human-like Bezier curve movement paths
//! - Keyboard input with realistic typing delays
//! - Platform-specific key code handling (Windows/Linux)
//! - Integration with the input module's timing utilities
//!
//! # Example
//!
//! ```rust,ignore
//! use ki_browser::browser::cef_input::{CefInputHandler, CefMouseButton};
//! use ki_browser::input::HumanTiming;
//!
//! async fn example() {
//!     let timing = HumanTiming::normal();
//!     let mut handler = CefInputHandler::new(timing);
//!
//!     // Move mouse with human-like path
//!     handler.send_mouse_move(500.0, 300.0).await.unwrap();
//!
//!     // Click at current position
//!     handler.send_mouse_click(500.0, 300.0, CefMouseButton::Left).await.unwrap();
//!
//!     // Type text with human-like delays
//!     handler.send_text("Hello, World!").await.unwrap();
//! }
//! ```

#![cfg(feature = "cef-browser")]

use crate::input::bezier::{generate_human_path, Point};
use crate::input::timing::HumanTiming;
use crate::input::{InputError, InputResult, Modifier};
use std::collections::HashSet;
use std::time::Duration;

// ============================================================================
// CEF Event Structures
// ============================================================================

/// Mouse button types for CEF events.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum CefMouseButton {
    /// Left mouse button (primary click)
    Left,
    /// Middle mouse button (scroll wheel click)
    Middle,
    /// Right mouse button (context menu)
    Right,
}

impl CefMouseButton {
    /// Returns the CEF-specific button type constant.
    ///
    /// CEF uses these values:
    /// - MBT_LEFT = 0
    /// - MBT_MIDDLE = 1
    /// - MBT_RIGHT = 2
    pub fn to_cef_type(&self) -> i32 {
        match self {
            CefMouseButton::Left => 0,   // MBT_LEFT
            CefMouseButton::Middle => 1, // MBT_MIDDLE
            CefMouseButton::Right => 2,  // MBT_RIGHT
        }
    }

    /// Returns the event flags for this button being pressed.
    ///
    /// Used in CefMouseEvent's modifiers field.
    pub fn to_event_flags(&self) -> u32 {
        match self {
            CefMouseButton::Left => EVENTFLAG_LEFT_MOUSE_BUTTON,
            CefMouseButton::Middle => EVENTFLAG_MIDDLE_MOUSE_BUTTON,
            CefMouseButton::Right => EVENTFLAG_RIGHT_MOUSE_BUTTON,
        }
    }
}

impl std::fmt::Display for CefMouseButton {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            CefMouseButton::Left => write!(f, "left"),
            CefMouseButton::Middle => write!(f, "middle"),
            CefMouseButton::Right => write!(f, "right"),
        }
    }
}

// CEF Event Flags (from cef_types.h)
const EVENTFLAG_NONE: u32 = 0;
const EVENTFLAG_CAPS_LOCK_ON: u32 = 1 << 0;
const EVENTFLAG_SHIFT_DOWN: u32 = 1 << 1;
const EVENTFLAG_CONTROL_DOWN: u32 = 1 << 2;
const EVENTFLAG_ALT_DOWN: u32 = 1 << 3;
const EVENTFLAG_LEFT_MOUSE_BUTTON: u32 = 1 << 4;
const EVENTFLAG_MIDDLE_MOUSE_BUTTON: u32 = 1 << 5;
const EVENTFLAG_RIGHT_MOUSE_BUTTON: u32 = 1 << 6;
// Command key on Mac, Windows key on Windows
const EVENTFLAG_COMMAND_DOWN: u32 = 1 << 7;
const EVENTFLAG_NUM_LOCK_ON: u32 = 1 << 8;
const EVENTFLAG_IS_KEY_PAD: u32 = 1 << 9;
const EVENTFLAG_IS_LEFT: u32 = 1 << 10;
const EVENTFLAG_IS_RIGHT: u32 = 1 << 11;
const EVENTFLAG_ALTGR_DOWN: u32 = 1 << 12;
const EVENTFLAG_IS_REPEAT: u32 = 1 << 13;

/// CEF mouse event structure.
///
/// Represents the data passed to CEF for mouse events including
/// position and modifier state.
#[derive(Debug, Clone, Copy)]
pub struct CefMouseEvent {
    /// X coordinate in view coordinates
    pub x: i32,
    /// Y coordinate in view coordinates
    pub y: i32,
    /// Combination of EVENTFLAG_* constants
    pub modifiers: u32,
}

impl CefMouseEvent {
    /// Creates a new mouse event at the specified coordinates.
    pub fn new(x: i32, y: i32) -> Self {
        Self {
            x,
            y,
            modifiers: EVENTFLAG_NONE,
        }
    }

    /// Creates a mouse event with modifier flags.
    pub fn with_modifiers(x: i32, y: i32, modifiers: u32) -> Self {
        Self { x, y, modifiers }
    }

    /// Adds a modifier flag to this event.
    pub fn add_modifier(&mut self, flag: u32) {
        self.modifiers |= flag;
    }

    /// Removes a modifier flag from this event.
    pub fn remove_modifier(&mut self, flag: u32) {
        self.modifiers &= !flag;
    }
}

impl Default for CefMouseEvent {
    fn default() -> Self {
        Self::new(0, 0)
    }
}

/// CEF key event types.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CefKeyEventType {
    /// Key was pressed down (raw key down)
    RawKeyDown,
    /// Key was pressed (translated key down)
    KeyDown,
    /// Key was released
    KeyUp,
    /// Character input event
    Char,
}

impl CefKeyEventType {
    /// Returns the CEF-specific key event type constant.
    pub fn to_cef_type(&self) -> i32 {
        match self {
            CefKeyEventType::RawKeyDown => 0, // KEYEVENT_RAWKEYDOWN
            CefKeyEventType::KeyDown => 1,    // KEYEVENT_KEYDOWN
            CefKeyEventType::KeyUp => 2,      // KEYEVENT_KEYUP
            CefKeyEventType::Char => 3,       // KEYEVENT_CHAR
        }
    }
}

/// CEF key event structure.
///
/// Represents keyboard input data for CEF including key codes,
/// character information, and modifier state.
#[derive(Debug, Clone)]
pub struct CefKeyEvent {
    /// The type of key event
    pub event_type: CefKeyEventType,
    /// Combination of EVENTFLAG_* constants
    pub modifiers: u32,
    /// Windows virtual key code
    pub windows_key_code: i32,
    /// Native (platform-specific) key code
    pub native_key_code: i32,
    /// Whether this is a system key (Alt combinations on Windows)
    pub is_system_key: bool,
    /// The character generated by the keystroke
    pub character: u16,
    /// Same as character but unmodified (ignoring Shift, etc.)
    pub unmodified_character: u16,
    /// True if the focus is currently on an editable field
    pub focus_on_editable_field: bool,
}

impl CefKeyEvent {
    /// Creates a new key event.
    pub fn new(event_type: CefKeyEventType, windows_key_code: i32) -> Self {
        Self {
            event_type,
            modifiers: EVENTFLAG_NONE,
            windows_key_code,
            native_key_code: 0,
            is_system_key: false,
            character: 0,
            unmodified_character: 0,
            focus_on_editable_field: false,
        }
    }

    /// Creates a character input event.
    pub fn char_event(character: char) -> Self {
        let char_code = character as u16;
        Self {
            event_type: CefKeyEventType::Char,
            modifiers: EVENTFLAG_NONE,
            windows_key_code: char_code as i32,
            native_key_code: 0,
            is_system_key: false,
            character: char_code,
            unmodified_character: char_code,
            focus_on_editable_field: false,
        }
    }

    /// Sets the native key code for this event.
    pub fn with_native_key_code(mut self, code: i32) -> Self {
        self.native_key_code = code;
        self
    }

    /// Sets the character for this event.
    pub fn with_character(mut self, character: char) -> Self {
        self.character = character as u16;
        self.unmodified_character = character as u16;
        self
    }

    /// Adds a modifier flag.
    pub fn with_modifier(mut self, flag: u32) -> Self {
        self.modifiers |= flag;
        self
    }

    /// Marks this as a system key event.
    pub fn as_system_key(mut self) -> Self {
        self.is_system_key = true;
        self
    }
}

impl Default for CefKeyEvent {
    fn default() -> Self {
        Self::new(CefKeyEventType::KeyDown, 0)
    }
}

// ============================================================================
// Platform-Specific Key Codes
// ============================================================================

/// Windows virtual key codes (VK_* constants).
#[cfg(target_os = "windows")]
pub mod key_codes {
    pub const VK_BACK: i32 = 0x08;
    pub const VK_TAB: i32 = 0x09;
    pub const VK_CLEAR: i32 = 0x0C;
    pub const VK_RETURN: i32 = 0x0D;
    pub const VK_SHIFT: i32 = 0x10;
    pub const VK_CONTROL: i32 = 0x11;
    pub const VK_MENU: i32 = 0x12; // Alt key
    pub const VK_PAUSE: i32 = 0x13;
    pub const VK_CAPITAL: i32 = 0x14; // Caps Lock
    pub const VK_ESCAPE: i32 = 0x1B;
    pub const VK_SPACE: i32 = 0x20;
    pub const VK_PRIOR: i32 = 0x21; // Page Up
    pub const VK_NEXT: i32 = 0x22;  // Page Down
    pub const VK_END: i32 = 0x23;
    pub const VK_HOME: i32 = 0x24;
    pub const VK_LEFT: i32 = 0x25;
    pub const VK_UP: i32 = 0x26;
    pub const VK_RIGHT: i32 = 0x27;
    pub const VK_DOWN: i32 = 0x28;
    pub const VK_SELECT: i32 = 0x29;
    pub const VK_PRINT: i32 = 0x2A;
    pub const VK_EXECUTE: i32 = 0x2B;
    pub const VK_SNAPSHOT: i32 = 0x2C; // Print Screen
    pub const VK_INSERT: i32 = 0x2D;
    pub const VK_DELETE: i32 = 0x2E;
    pub const VK_HELP: i32 = 0x2F;
    // 0-9 are the same as ASCII '0'-'9' (0x30-0x39)
    // A-Z are the same as ASCII 'A'-'Z' (0x41-0x5A)
    pub const VK_LWIN: i32 = 0x5B;
    pub const VK_RWIN: i32 = 0x5C;
    pub const VK_APPS: i32 = 0x5D;
    pub const VK_SLEEP: i32 = 0x5F;
    pub const VK_NUMPAD0: i32 = 0x60;
    pub const VK_NUMPAD1: i32 = 0x61;
    pub const VK_NUMPAD2: i32 = 0x62;
    pub const VK_NUMPAD3: i32 = 0x63;
    pub const VK_NUMPAD4: i32 = 0x64;
    pub const VK_NUMPAD5: i32 = 0x65;
    pub const VK_NUMPAD6: i32 = 0x66;
    pub const VK_NUMPAD7: i32 = 0x67;
    pub const VK_NUMPAD8: i32 = 0x68;
    pub const VK_NUMPAD9: i32 = 0x69;
    pub const VK_MULTIPLY: i32 = 0x6A;
    pub const VK_ADD: i32 = 0x6B;
    pub const VK_SEPARATOR: i32 = 0x6C;
    pub const VK_SUBTRACT: i32 = 0x6D;
    pub const VK_DECIMAL: i32 = 0x6E;
    pub const VK_DIVIDE: i32 = 0x6F;
    pub const VK_F1: i32 = 0x70;
    pub const VK_F2: i32 = 0x71;
    pub const VK_F3: i32 = 0x72;
    pub const VK_F4: i32 = 0x73;
    pub const VK_F5: i32 = 0x74;
    pub const VK_F6: i32 = 0x75;
    pub const VK_F7: i32 = 0x76;
    pub const VK_F8: i32 = 0x77;
    pub const VK_F9: i32 = 0x78;
    pub const VK_F10: i32 = 0x79;
    pub const VK_F11: i32 = 0x7A;
    pub const VK_F12: i32 = 0x7B;
    pub const VK_NUMLOCK: i32 = 0x90;
    pub const VK_SCROLL: i32 = 0x91;
    pub const VK_LSHIFT: i32 = 0xA0;
    pub const VK_RSHIFT: i32 = 0xA1;
    pub const VK_LCONTROL: i32 = 0xA2;
    pub const VK_RCONTROL: i32 = 0xA3;
    pub const VK_LMENU: i32 = 0xA4;
    pub const VK_RMENU: i32 = 0xA5;
    pub const VK_OEM_1: i32 = 0xBA; // ;:
    pub const VK_OEM_PLUS: i32 = 0xBB; // =+
    pub const VK_OEM_COMMA: i32 = 0xBC; // ,<
    pub const VK_OEM_MINUS: i32 = 0xBD; // -_
    pub const VK_OEM_PERIOD: i32 = 0xBE; // .>
    pub const VK_OEM_2: i32 = 0xBF; // /?
    pub const VK_OEM_3: i32 = 0xC0; // `~
    pub const VK_OEM_4: i32 = 0xDB; // [{
    pub const VK_OEM_5: i32 = 0xDC; // \|
    pub const VK_OEM_6: i32 = 0xDD; // ]}
    pub const VK_OEM_7: i32 = 0xDE; // '"
}

/// Linux/X11 key codes (using XKB keysyms).
#[cfg(target_os = "linux")]
pub mod key_codes {
    // X11 uses different key codes - these map to XKB keysyms
    pub const VK_BACK: i32 = 0xFF08; // XK_BackSpace
    pub const VK_TAB: i32 = 0xFF09; // XK_Tab
    pub const VK_CLEAR: i32 = 0xFF0B; // XK_Clear
    pub const VK_RETURN: i32 = 0xFF0D; // XK_Return
    pub const VK_SHIFT: i32 = 0xFFE1; // XK_Shift_L
    pub const VK_CONTROL: i32 = 0xFFE3; // XK_Control_L
    pub const VK_MENU: i32 = 0xFFE9; // XK_Alt_L
    pub const VK_PAUSE: i32 = 0xFF13; // XK_Pause
    pub const VK_CAPITAL: i32 = 0xFFE5; // XK_Caps_Lock
    pub const VK_ESCAPE: i32 = 0xFF1B; // XK_Escape
    pub const VK_SPACE: i32 = 0x0020; // XK_space
    pub const VK_PRIOR: i32 = 0xFF55; // XK_Page_Up
    pub const VK_NEXT: i32 = 0xFF56; // XK_Page_Down
    pub const VK_END: i32 = 0xFF57; // XK_End
    pub const VK_HOME: i32 = 0xFF50; // XK_Home
    pub const VK_LEFT: i32 = 0xFF51; // XK_Left
    pub const VK_UP: i32 = 0xFF52; // XK_Up
    pub const VK_RIGHT: i32 = 0xFF53; // XK_Right
    pub const VK_DOWN: i32 = 0xFF54; // XK_Down
    pub const VK_SELECT: i32 = 0xFF60; // XK_Select
    pub const VK_PRINT: i32 = 0xFF61; // XK_Print
    pub const VK_EXECUTE: i32 = 0xFF62; // XK_Execute
    pub const VK_SNAPSHOT: i32 = 0xFF61; // Same as Print
    pub const VK_INSERT: i32 = 0xFF63; // XK_Insert
    pub const VK_DELETE: i32 = 0xFFFF; // XK_Delete
    pub const VK_HELP: i32 = 0xFF6A; // XK_Help
    pub const VK_LWIN: i32 = 0xFFEB; // XK_Super_L
    pub const VK_RWIN: i32 = 0xFFEC; // XK_Super_R
    pub const VK_APPS: i32 = 0xFF67; // XK_Menu
    pub const VK_SLEEP: i32 = 0x1008FF2F; // XF86XK_Sleep
    pub const VK_NUMPAD0: i32 = 0xFFB0; // XK_KP_0
    pub const VK_NUMPAD1: i32 = 0xFFB1;
    pub const VK_NUMPAD2: i32 = 0xFFB2;
    pub const VK_NUMPAD3: i32 = 0xFFB3;
    pub const VK_NUMPAD4: i32 = 0xFFB4;
    pub const VK_NUMPAD5: i32 = 0xFFB5;
    pub const VK_NUMPAD6: i32 = 0xFFB6;
    pub const VK_NUMPAD7: i32 = 0xFFB7;
    pub const VK_NUMPAD8: i32 = 0xFFB8;
    pub const VK_NUMPAD9: i32 = 0xFFB9;
    pub const VK_MULTIPLY: i32 = 0xFFAA; // XK_KP_Multiply
    pub const VK_ADD: i32 = 0xFFAB; // XK_KP_Add
    pub const VK_SEPARATOR: i32 = 0xFFAC; // XK_KP_Separator
    pub const VK_SUBTRACT: i32 = 0xFFAD; // XK_KP_Subtract
    pub const VK_DECIMAL: i32 = 0xFFAE; // XK_KP_Decimal
    pub const VK_DIVIDE: i32 = 0xFFAF; // XK_KP_Divide
    pub const VK_F1: i32 = 0xFFBE; // XK_F1
    pub const VK_F2: i32 = 0xFFBF;
    pub const VK_F3: i32 = 0xFFC0;
    pub const VK_F4: i32 = 0xFFC1;
    pub const VK_F5: i32 = 0xFFC2;
    pub const VK_F6: i32 = 0xFFC3;
    pub const VK_F7: i32 = 0xFFC4;
    pub const VK_F8: i32 = 0xFFC5;
    pub const VK_F9: i32 = 0xFFC6;
    pub const VK_F10: i32 = 0xFFC7;
    pub const VK_F11: i32 = 0xFFC8;
    pub const VK_F12: i32 = 0xFFC9;
    pub const VK_NUMLOCK: i32 = 0xFF7F; // XK_Num_Lock
    pub const VK_SCROLL: i32 = 0xFF14; // XK_Scroll_Lock
    pub const VK_LSHIFT: i32 = 0xFFE1;
    pub const VK_RSHIFT: i32 = 0xFFE2;
    pub const VK_LCONTROL: i32 = 0xFFE3;
    pub const VK_RCONTROL: i32 = 0xFFE4;
    pub const VK_LMENU: i32 = 0xFFE9;
    pub const VK_RMENU: i32 = 0xFFEA;
    // OEM keys - using ASCII values as Linux doesn't have direct equivalents
    pub const VK_OEM_1: i32 = 0x003B; // ;
    pub const VK_OEM_PLUS: i32 = 0x003D; // =
    pub const VK_OEM_COMMA: i32 = 0x002C; // ,
    pub const VK_OEM_MINUS: i32 = 0x002D; // -
    pub const VK_OEM_PERIOD: i32 = 0x002E; // .
    pub const VK_OEM_2: i32 = 0x002F; // /
    pub const VK_OEM_3: i32 = 0x0060; // `
    pub const VK_OEM_4: i32 = 0x005B; // [
    pub const VK_OEM_5: i32 = 0x005C; // \
    pub const VK_OEM_6: i32 = 0x005D; // ]
    pub const VK_OEM_7: i32 = 0x0027; // '
}

/// Fallback key codes for other platforms (using Windows-style codes).
#[cfg(not(any(target_os = "windows", target_os = "linux")))]
pub mod key_codes {
    pub const VK_BACK: i32 = 0x08;
    pub const VK_TAB: i32 = 0x09;
    pub const VK_CLEAR: i32 = 0x0C;
    pub const VK_RETURN: i32 = 0x0D;
    pub const VK_SHIFT: i32 = 0x10;
    pub const VK_CONTROL: i32 = 0x11;
    pub const VK_MENU: i32 = 0x12;
    pub const VK_PAUSE: i32 = 0x13;
    pub const VK_CAPITAL: i32 = 0x14;
    pub const VK_ESCAPE: i32 = 0x1B;
    pub const VK_SPACE: i32 = 0x20;
    pub const VK_PRIOR: i32 = 0x21;
    pub const VK_NEXT: i32 = 0x22;
    pub const VK_END: i32 = 0x23;
    pub const VK_HOME: i32 = 0x24;
    pub const VK_LEFT: i32 = 0x25;
    pub const VK_UP: i32 = 0x26;
    pub const VK_RIGHT: i32 = 0x27;
    pub const VK_DOWN: i32 = 0x28;
    pub const VK_SELECT: i32 = 0x29;
    pub const VK_PRINT: i32 = 0x2A;
    pub const VK_EXECUTE: i32 = 0x2B;
    pub const VK_SNAPSHOT: i32 = 0x2C;
    pub const VK_INSERT: i32 = 0x2D;
    pub const VK_DELETE: i32 = 0x2E;
    pub const VK_HELP: i32 = 0x2F;
    pub const VK_LWIN: i32 = 0x5B;
    pub const VK_RWIN: i32 = 0x5C;
    pub const VK_APPS: i32 = 0x5D;
    pub const VK_SLEEP: i32 = 0x5F;
    pub const VK_NUMPAD0: i32 = 0x60;
    pub const VK_NUMPAD1: i32 = 0x61;
    pub const VK_NUMPAD2: i32 = 0x62;
    pub const VK_NUMPAD3: i32 = 0x63;
    pub const VK_NUMPAD4: i32 = 0x64;
    pub const VK_NUMPAD5: i32 = 0x65;
    pub const VK_NUMPAD6: i32 = 0x66;
    pub const VK_NUMPAD7: i32 = 0x67;
    pub const VK_NUMPAD8: i32 = 0x68;
    pub const VK_NUMPAD9: i32 = 0x69;
    pub const VK_MULTIPLY: i32 = 0x6A;
    pub const VK_ADD: i32 = 0x6B;
    pub const VK_SEPARATOR: i32 = 0x6C;
    pub const VK_SUBTRACT: i32 = 0x6D;
    pub const VK_DECIMAL: i32 = 0x6E;
    pub const VK_DIVIDE: i32 = 0x6F;
    pub const VK_F1: i32 = 0x70;
    pub const VK_F2: i32 = 0x71;
    pub const VK_F3: i32 = 0x72;
    pub const VK_F4: i32 = 0x73;
    pub const VK_F5: i32 = 0x74;
    pub const VK_F6: i32 = 0x75;
    pub const VK_F7: i32 = 0x76;
    pub const VK_F8: i32 = 0x77;
    pub const VK_F9: i32 = 0x78;
    pub const VK_F10: i32 = 0x79;
    pub const VK_F11: i32 = 0x7A;
    pub const VK_F12: i32 = 0x7B;
    pub const VK_NUMLOCK: i32 = 0x90;
    pub const VK_SCROLL: i32 = 0x91;
    pub const VK_LSHIFT: i32 = 0xA0;
    pub const VK_RSHIFT: i32 = 0xA1;
    pub const VK_LCONTROL: i32 = 0xA2;
    pub const VK_RCONTROL: i32 = 0xA3;
    pub const VK_LMENU: i32 = 0xA4;
    pub const VK_RMENU: i32 = 0xA5;
    pub const VK_OEM_1: i32 = 0xBA;
    pub const VK_OEM_PLUS: i32 = 0xBB;
    pub const VK_OEM_COMMA: i32 = 0xBC;
    pub const VK_OEM_MINUS: i32 = 0xBD;
    pub const VK_OEM_PERIOD: i32 = 0xBE;
    pub const VK_OEM_2: i32 = 0xBF;
    pub const VK_OEM_3: i32 = 0xC0;
    pub const VK_OEM_4: i32 = 0xDB;
    pub const VK_OEM_5: i32 = 0xDC;
    pub const VK_OEM_6: i32 = 0xDD;
    pub const VK_OEM_7: i32 = 0xDE;
}

// ============================================================================
// Key Code Conversion Utilities
// ============================================================================

/// Converts a key name string to a virtual key code.
///
/// # Arguments
///
/// * `key` - The key name (e.g., "Enter", "Tab", "a", "F1")
///
/// # Returns
///
/// The platform-specific virtual key code, or None if not recognized.
pub fn key_name_to_code(key: &str) -> Option<i32> {
    use key_codes::*;

    // Single character keys
    if key.len() == 1 {
        let c = key.chars().next().unwrap();
        return match c {
            'a'..='z' => Some((c.to_ascii_uppercase() as u8) as i32),
            'A'..='Z' => Some((c as u8) as i32),
            '0'..='9' => Some((c as u8) as i32),
            ' ' => Some(VK_SPACE),
            ';' | ':' => Some(VK_OEM_1),
            '=' | '+' => Some(VK_OEM_PLUS),
            ',' | '<' => Some(VK_OEM_COMMA),
            '-' | '_' => Some(VK_OEM_MINUS),
            '.' | '>' => Some(VK_OEM_PERIOD),
            '/' | '?' => Some(VK_OEM_2),
            '`' | '~' => Some(VK_OEM_3),
            '[' | '{' => Some(VK_OEM_4),
            '\\' | '|' => Some(VK_OEM_5),
            ']' | '}' => Some(VK_OEM_6),
            '\'' | '"' => Some(VK_OEM_7),
            _ => None,
        };
    }

    // Special keys
    match key.to_lowercase().as_str() {
        "enter" | "return" => Some(VK_RETURN),
        "tab" => Some(VK_TAB),
        "backspace" => Some(VK_BACK),
        "delete" | "del" => Some(VK_DELETE),
        "insert" | "ins" => Some(VK_INSERT),
        "escape" | "esc" => Some(VK_ESCAPE),
        "space" => Some(VK_SPACE),
        "home" => Some(VK_HOME),
        "end" => Some(VK_END),
        "pageup" | "pgup" => Some(VK_PRIOR),
        "pagedown" | "pgdn" => Some(VK_NEXT),
        "arrowup" | "up" => Some(VK_UP),
        "arrowdown" | "down" => Some(VK_DOWN),
        "arrowleft" | "left" => Some(VK_LEFT),
        "arrowright" | "right" => Some(VK_RIGHT),
        "shift" => Some(VK_SHIFT),
        "control" | "ctrl" => Some(VK_CONTROL),
        "alt" => Some(VK_MENU),
        "meta" | "command" | "cmd" | "windows" | "win" => Some(VK_LWIN),
        "capslock" => Some(VK_CAPITAL),
        "numlock" => Some(VK_NUMLOCK),
        "scrolllock" => Some(VK_SCROLL),
        "pause" => Some(VK_PAUSE),
        "printscreen" | "prtsc" => Some(VK_SNAPSHOT),
        "f1" => Some(VK_F1),
        "f2" => Some(VK_F2),
        "f3" => Some(VK_F3),
        "f4" => Some(VK_F4),
        "f5" => Some(VK_F5),
        "f6" => Some(VK_F6),
        "f7" => Some(VK_F7),
        "f8" => Some(VK_F8),
        "f9" => Some(VK_F9),
        "f10" => Some(VK_F10),
        "f11" => Some(VK_F11),
        "f12" => Some(VK_F12),
        _ => None,
    }
}

/// Converts a Modifier to the corresponding event flag.
pub fn modifier_to_event_flag(modifier: &Modifier) -> u32 {
    match modifier {
        Modifier::Shift => EVENTFLAG_SHIFT_DOWN,
        Modifier::Ctrl => EVENTFLAG_CONTROL_DOWN,
        Modifier::Alt => EVENTFLAG_ALT_DOWN,
        Modifier::Meta => EVENTFLAG_COMMAND_DOWN,
    }
}

/// Combines multiple modifiers into a single event flags value.
pub fn modifiers_to_event_flags(modifiers: &[Modifier]) -> u32 {
    modifiers.iter().fold(EVENTFLAG_NONE, |acc, m| acc | modifier_to_event_flag(m))
}

// ============================================================================
// CEF Input Handler
// ============================================================================

/// Configuration for CEF input handling.
#[derive(Debug, Clone)]
pub struct CefInputConfig {
    /// Minimum number of points for mouse movement paths
    pub min_path_points: usize,
    /// Maximum number of points for mouse movement paths
    pub max_path_points: usize,
    /// Whether to add random micro-movements to mouse paths
    pub add_jitter: bool,
    /// Jitter intensity (0.0 - 1.0)
    pub jitter_intensity: f64,
    /// View bounds for coordinate validation (width, height)
    pub view_bounds: Option<(i32, i32)>,
}

impl Default for CefInputConfig {
    fn default() -> Self {
        Self {
            min_path_points: 20,
            max_path_points: 50,
            add_jitter: true,
            jitter_intensity: 0.3,
            view_bounds: None,
        }
    }
}

/// Callback trait for sending CEF events to the browser.
///
/// Implement this trait to connect the input handler to actual CEF browser
/// instances for event delivery.
pub trait CefEventSender: Send + Sync {
    /// Sends a mouse move event.
    fn send_mouse_move_event(&self, event: &CefMouseEvent, mouse_leave: bool);

    /// Sends a mouse click event.
    fn send_mouse_click_event(
        &self,
        event: &CefMouseEvent,
        button: CefMouseButton,
        mouse_up: bool,
        click_count: i32,
    );

    /// Sends a mouse wheel event.
    fn send_mouse_wheel_event(&self, event: &CefMouseEvent, delta_x: i32, delta_y: i32);

    /// Sends a keyboard event.
    fn send_key_event(&self, event: &CefKeyEvent);
}

/// Handles native input simulation for CEF browsers.
///
/// This struct provides methods for simulating mouse and keyboard input
/// with human-like timing and movement patterns.
pub struct CefInputHandler<S: CefEventSender> {
    /// Event sender for delivering events to CEF
    sender: S,
    /// Current mouse position
    current_position: Point,
    /// Configuration for input behavior
    config: CefInputConfig,
    /// Timing utility for realistic delays
    timing: HumanTiming,
    /// Currently pressed mouse buttons
    pressed_buttons: HashSet<CefMouseButton>,
    /// Currently pressed modifier keys
    active_modifiers: HashSet<Modifier>,
}

impl<S: CefEventSender> CefInputHandler<S> {
    /// Creates a new CEF input handler.
    ///
    /// # Arguments
    ///
    /// * `sender` - The event sender for delivering events to CEF
    /// * `timing` - Human timing configuration for delays
    pub fn new(sender: S, timing: HumanTiming) -> Self {
        Self {
            sender,
            current_position: Point::new(0.0, 0.0),
            config: CefInputConfig::default(),
            timing,
            pressed_buttons: HashSet::new(),
            active_modifiers: HashSet::new(),
        }
    }

    /// Creates a new handler with custom configuration.
    pub fn with_config(sender: S, timing: HumanTiming, config: CefInputConfig) -> Self {
        Self {
            sender,
            current_position: Point::new(0.0, 0.0),
            config,
            timing,
            pressed_buttons: HashSet::new(),
            active_modifiers: HashSet::new(),
        }
    }

    /// Returns the current mouse position.
    pub fn position(&self) -> Point {
        self.current_position
    }

    /// Sets the mouse position without animation.
    pub fn set_position(&mut self, x: f64, y: f64) {
        self.current_position = Point::new(x, y);
    }

    /// Gets the currently active modifiers.
    pub fn active_modifiers(&self) -> Vec<Modifier> {
        self.active_modifiers.iter().copied().collect()
    }

    /// Builds the current modifier flags.
    fn current_modifier_flags(&self) -> u32 {
        let mut flags = EVENTFLAG_NONE;
        for modifier in &self.active_modifiers {
            flags |= modifier_to_event_flag(modifier);
        }
        for button in &self.pressed_buttons {
            flags |= button.to_event_flags();
        }
        flags
    }

    /// Validates coordinates against view bounds.
    fn validate_position(&self, x: f64, y: f64) -> InputResult<()> {
        if x < 0.0 || y < 0.0 {
            return Err(InputError::OutOfBounds { x, y });
        }

        if let Some((max_x, max_y)) = self.config.view_bounds {
            if x > max_x as f64 || y > max_y as f64 {
                return Err(InputError::OutOfBounds { x, y });
            }
        }

        Ok(())
    }

    /// Creates a CefMouseEvent at the specified coordinates with current modifiers.
    fn create_mouse_event(&self, x: i32, y: i32) -> CefMouseEvent {
        CefMouseEvent::with_modifiers(x, y, self.current_modifier_flags())
    }

    // ========================================================================
    // Mouse Input Methods
    // ========================================================================

    /// Moves the mouse to the specified position using a human-like Bezier path.
    ///
    /// # Arguments
    ///
    /// * `x` - Target X coordinate
    /// * `y` - Target Y coordinate
    ///
    /// # Returns
    ///
    /// A vector of points representing the path taken.
    ///
    /// # Example
    ///
    /// ```rust,ignore
    /// handler.send_mouse_move(500.0, 300.0).await?;
    /// ```
    pub async fn send_mouse_move(&mut self, x: f64, y: f64) -> InputResult<Vec<Point>> {
        self.validate_position(x, y)?;

        let target = Point::new(x, y);
        let distance = self.current_position.distance_to(&target);

        // Calculate number of path points based on distance
        let num_points = calculate_path_points(
            distance,
            self.config.min_path_points,
            self.config.max_path_points,
        );

        // Generate human-like path using Bezier curves
        let mut path = generate_human_path(self.current_position, target, num_points);

        // Add micro-jitter if enabled
        if self.config.add_jitter {
            add_jitter_to_path(&mut path, self.config.jitter_intensity);
        }

        // Move along the path with delays
        for point in &path {
            let delay = self.timing.get_move_delay();
            tokio::time::sleep(delay).await;

            self.current_position = *point;

            let event = self.create_mouse_event(point.x.round() as i32, point.y.round() as i32);
            self.sender.send_mouse_move_event(&event, false);
        }

        Ok(path)
    }

    /// Performs a mouse click at the specified position.
    ///
    /// This is a combined move + click operation with human-like timing.
    ///
    /// # Arguments
    ///
    /// * `x` - X coordinate to click
    /// * `y` - Y coordinate to click
    /// * `button` - Which mouse button to click
    pub async fn send_mouse_click(
        &mut self,
        x: f64,
        y: f64,
        button: CefMouseButton,
    ) -> InputResult<()> {
        // Move to position first
        self.send_mouse_move(x, y).await?;

        // Small pause before clicking (natural hesitation)
        let pause = Duration::from_millis(rand::random::<u64>() % 50 + 20);
        tokio::time::sleep(pause).await;

        // Mouse down
        self.send_mouse_down(x, y, button).await?;

        // Hold duration
        let hold_delay = self.timing.get_click_delay();
        tokio::time::sleep(hold_delay).await;

        // Mouse up
        self.send_mouse_up(x, y, button).await?;

        Ok(())
    }

    /// Sends a mouse button down event at the specified position.
    ///
    /// # Arguments
    ///
    /// * `x` - X coordinate
    /// * `y` - Y coordinate
    /// * `button` - Which mouse button to press
    pub async fn send_mouse_down(
        &mut self,
        x: f64,
        y: f64,
        button: CefMouseButton,
    ) -> InputResult<()> {
        self.validate_position(x, y)?;

        // Small pre-press delay
        let delay = Duration::from_millis(rand::random::<u64>() % 10 + 2);
        tokio::time::sleep(delay).await;

        self.pressed_buttons.insert(button);

        let event = self.create_mouse_event(x.round() as i32, y.round() as i32);
        self.sender.send_mouse_click_event(&event, button, false, 1);

        Ok(())
    }

    /// Sends a mouse button up event at the specified position.
    ///
    /// # Arguments
    ///
    /// * `x` - X coordinate
    /// * `y` - Y coordinate
    /// * `button` - Which mouse button to release
    pub async fn send_mouse_up(
        &mut self,
        x: f64,
        y: f64,
        button: CefMouseButton,
    ) -> InputResult<()> {
        self.validate_position(x, y)?;

        self.pressed_buttons.remove(&button);

        let event = self.create_mouse_event(x.round() as i32, y.round() as i32);
        self.sender.send_mouse_click_event(&event, button, true, 1);

        Ok(())
    }

    /// Sends a scroll wheel event at the specified position.
    ///
    /// # Arguments
    ///
    /// * `x` - X coordinate for scroll location
    /// * `y` - Y coordinate for scroll location
    /// * `delta_x` - Horizontal scroll amount (positive = right)
    /// * `delta_y` - Vertical scroll amount (positive = down)
    pub async fn send_scroll(
        &mut self,
        x: f64,
        y: f64,
        delta_x: f64,
        delta_y: f64,
    ) -> InputResult<()> {
        self.validate_position(x, y)?;

        // Convert to pixel units (CEF uses pixels for scroll)
        // Typical scroll step is around 40-120 pixels
        let scroll_multiplier = 40.0;
        let total_dx = (delta_x * scroll_multiplier).round() as i32;
        let total_dy = (delta_y * scroll_multiplier).round() as i32;

        // Calculate number of scroll steps for smooth scrolling
        let steps = ((delta_x.abs() + delta_y.abs()).ceil() as usize).max(1);
        let step_dx = total_dx / steps as i32;
        let step_dy = total_dy / steps as i32;

        for i in 0..steps {
            // Delay between scroll steps
            let delay = Duration::from_millis(rand::random::<u64>() % 30 + 10);
            tokio::time::sleep(delay).await;

            // Calculate this step's delta (handle remainder on last step)
            let dx = if i == steps - 1 {
                total_dx - step_dx * (steps as i32 - 1)
            } else {
                step_dx
            };
            let dy = if i == steps - 1 {
                total_dy - step_dy * (steps as i32 - 1)
            } else {
                step_dy
            };

            let event = self.create_mouse_event(x.round() as i32, y.round() as i32);
            self.sender.send_mouse_wheel_event(&event, dx, dy);
        }

        Ok(())
    }

    /// Performs a double-click at the specified position.
    pub async fn send_double_click(
        &mut self,
        x: f64,
        y: f64,
        button: CefMouseButton,
    ) -> InputResult<()> {
        // First click
        self.send_mouse_click(x, y, button).await?;

        // Inter-click delay for double-click recognition
        let delay = self.timing.get_double_click_interval();
        tokio::time::sleep(delay).await;

        // Second click with click_count = 2
        let event = self.create_mouse_event(x.round() as i32, y.round() as i32);
        self.sender.send_mouse_click_event(&event, button, false, 2);

        let hold = self.timing.get_click_delay();
        tokio::time::sleep(hold).await;

        self.sender.send_mouse_click_event(&event, button, true, 2);

        Ok(())
    }

    /// Performs a drag operation from the current position to the target.
    pub async fn send_drag(
        &mut self,
        target_x: f64,
        target_y: f64,
        button: CefMouseButton,
    ) -> InputResult<Vec<Point>> {
        let start = self.current_position;

        // Press button down
        self.send_mouse_down(start.x, start.y, button).await?;

        // Small delay after pressing
        let delay = Duration::from_millis(rand::random::<u64>() % 50 + 30);
        tokio::time::sleep(delay).await;

        // Move to target
        let path = self.send_mouse_move(target_x, target_y).await?;

        // Small delay before releasing
        let delay = Duration::from_millis(rand::random::<u64>() % 50 + 30);
        tokio::time::sleep(delay).await;

        // Release button
        self.send_mouse_up(target_x, target_y, button).await?;

        Ok(path)
    }

    // ========================================================================
    // Keyboard Input Methods
    // ========================================================================

    /// Sends a key event (press or release).
    ///
    /// # Arguments
    ///
    /// * `key` - The key name or character
    /// * `modifiers` - Active modifier keys
    /// * `is_down` - True for key down, false for key up
    pub async fn send_key_event(
        &mut self,
        key: &str,
        modifiers: &[Modifier],
        is_down: bool,
    ) -> InputResult<()> {
        let key_code = key_name_to_code(key).ok_or_else(|| InputError::InvalidKey {
            key: key.to_string(),
        })?;

        let event_type = if is_down {
            CefKeyEventType::KeyDown
        } else {
            CefKeyEventType::KeyUp
        };

        let modifier_flags = modifiers_to_event_flags(modifiers);

        let mut event = CefKeyEvent::new(event_type, key_code);
        event.modifiers = modifier_flags;

        // Set character if it's a printable key
        if key.len() == 1 {
            let c = key.chars().next().unwrap();
            event.character = c as u16;
            event.unmodified_character = c.to_ascii_lowercase() as u16;
        }

        // Update modifier state
        if is_down {
            if let Some(modifier) = self.parse_modifier(key) {
                self.active_modifiers.insert(modifier);
            }
        } else {
            if let Some(modifier) = self.parse_modifier(key) {
                self.active_modifiers.remove(&modifier);
            }
        }

        // Small delay for realism
        let delay = Duration::from_millis(rand::random::<u64>() % 10 + 2);
        tokio::time::sleep(delay).await;

        self.sender.send_key_event(&event);

        Ok(())
    }

    /// Sends a character input event.
    ///
    /// This is used for text input where the character matters,
    /// not the specific key pressed.
    ///
    /// # Arguments
    ///
    /// * `c` - The character to input
    pub async fn send_char(&mut self, c: char) -> InputResult<()> {
        let event = CefKeyEvent::char_event(c)
            .with_modifier(self.current_modifier_flags());

        self.sender.send_key_event(&event);

        Ok(())
    }

    /// Types text with human-like delays between characters.
    ///
    /// # Arguments
    ///
    /// * `text` - The text to type
    ///
    /// # Example
    ///
    /// ```rust,ignore
    /// handler.send_text("Hello, World!").await?;
    /// ```
    pub async fn send_text(&mut self, text: &str) -> InputResult<()> {
        for c in text.chars() {
            // Check if shift is needed for uppercase or special characters
            let needs_shift = c.is_uppercase() || is_shifted_character(c);

            if needs_shift {
                // Press shift
                self.send_key_event("Shift", &[], true).await?;
            }

            // Get the key for this character
            let key = get_key_for_char(c);

            // Key down
            self.send_key_event(&key, &[], true).await?;

            // Character event
            self.send_char(c).await?;

            // Hold time
            let hold = self.timing.get_click_delay();
            tokio::time::sleep(hold).await;

            // Key up
            self.send_key_event(&key, &[], false).await?;

            if needs_shift {
                // Release shift
                self.send_key_event("Shift", &[], false).await?;
            }

            // Inter-keystroke delay
            let delay = self.get_char_delay(c);
            tokio::time::sleep(delay).await;
        }

        Ok(())
    }

    /// Presses a key combination (e.g., Ctrl+C).
    ///
    /// # Arguments
    ///
    /// * `key` - The main key
    /// * `modifiers` - Modifier keys to hold
    pub async fn send_key_combo(&mut self, key: &str, modifiers: &[Modifier]) -> InputResult<()> {
        // Press modifiers
        for modifier in modifiers {
            let mod_key = modifier_to_key_name(modifier);
            self.send_key_event(mod_key, &[], true).await?;

            let delay = Duration::from_millis(rand::random::<u64>() % 20 + 10);
            tokio::time::sleep(delay).await;
        }

        // Press main key
        self.send_key_event(key, modifiers, true).await?;

        let hold = self.timing.get_click_delay();
        tokio::time::sleep(hold).await;

        // Release main key
        self.send_key_event(key, modifiers, false).await?;

        // Release modifiers in reverse order
        for modifier in modifiers.iter().rev() {
            let delay = Duration::from_millis(rand::random::<u64>() % 20 + 10);
            tokio::time::sleep(delay).await;

            let mod_key = modifier_to_key_name(modifier);
            self.send_key_event(mod_key, &[], false).await?;
        }

        Ok(())
    }

    /// Parses a key name into a Modifier if applicable.
    fn parse_modifier(&self, key: &str) -> Option<Modifier> {
        match key.to_lowercase().as_str() {
            "shift" => Some(Modifier::Shift),
            "control" | "ctrl" => Some(Modifier::Ctrl),
            "alt" => Some(Modifier::Alt),
            "meta" | "command" | "cmd" | "windows" | "win" => Some(Modifier::Meta),
            _ => None,
        }
    }

    /// Gets a human-like delay for typing a specific character.
    fn get_char_delay(&self, c: char) -> Duration {
        let base = self.timing.get_type_delay();

        // Adjust based on character type
        let multiplier = match c {
            // Common letters - fastest
            'e' | 't' | 'a' | 'o' | 'i' | 'n' | 's' | 'h' | 'r' => 0.8,
            // Less common letters
            'l' | 'd' | 'c' | 'u' | 'm' | 'w' | 'f' | 'g' | 'y' | 'p' | 'b' => 1.0,
            // Rare letters
            'v' | 'k' | 'j' | 'x' | 'q' | 'z' => 1.2,
            // Numbers
            '0'..='9' => 1.1,
            // Space is fast (thumb key)
            ' ' => 0.7,
            // Punctuation
            '.' | ',' => 1.0,
            '!' | '?' | ':' | ';' => 1.3,
            // Special characters - slowest
            '@' | '#' | '$' | '%' | '^' | '&' | '*' => 1.5,
            // Uppercase adds shift delay
            _ if c.is_uppercase() => 1.2,
            // Default
            _ => 1.0,
        };

        Duration::from_millis((base.as_millis() as f64 * multiplier) as u64)
    }
}

// ============================================================================
// Helper Functions
// ============================================================================

/// Calculates the number of path points based on distance.
fn calculate_path_points(distance: f64, min: usize, max: usize) -> usize {
    // More points for longer distances
    let points = (distance / 10.0).ceil() as usize;
    points.clamp(min, max)
}

/// Adds random micro-jitter to a path to simulate hand tremor.
fn add_jitter_to_path(path: &mut [Point], intensity: f64) {
    // Don't jitter the first and last points to ensure exact positioning
    let len = path.len();
    if len <= 2 {
        return;
    }

    for point in path[1..len - 1].iter_mut() {
        let jitter_x = (rand::random::<f64>() - 0.5) * intensity * 2.0;
        let jitter_y = (rand::random::<f64>() - 0.5) * intensity * 2.0;
        point.x += jitter_x;
        point.y += jitter_y;
    }
}

/// Checks if a character requires shift to type.
fn is_shifted_character(c: char) -> bool {
    matches!(c,
        '!' | '@' | '#' | '$' | '%' | '^' | '&' | '*' | '(' | ')' |
        '_' | '+' | '{' | '}' | '|' | ':' | '"' | '<' | '>' | '?' |
        '~'
    )
}

/// Gets the base key for a character (without shift).
fn get_key_for_char(c: char) -> String {
    let c_lower = c.to_ascii_lowercase();
    match c_lower {
        '!' => "1".to_string(),
        '@' => "2".to_string(),
        '#' => "3".to_string(),
        '$' => "4".to_string(),
        '%' => "5".to_string(),
        '^' => "6".to_string(),
        '&' => "7".to_string(),
        '*' => "8".to_string(),
        '(' => "9".to_string(),
        ')' => "0".to_string(),
        '_' => "-".to_string(),
        '+' => "=".to_string(),
        '{' => "[".to_string(),
        '}' => "]".to_string(),
        '|' => "\\".to_string(),
        ':' => ";".to_string(),
        '"' => "'".to_string(),
        '<' => ",".to_string(),
        '>' => ".".to_string(),
        '?' => "/".to_string(),
        '~' => "`".to_string(),
        _ => c_lower.to_string(),
    }
}

/// Converts a Modifier to its key name.
fn modifier_to_key_name(modifier: &Modifier) -> &'static str {
    match modifier {
        Modifier::Shift => "Shift",
        Modifier::Ctrl => "Control",
        Modifier::Alt => "Alt",
        Modifier::Meta => "Meta",
    }
}

// ============================================================================
// Mock Event Sender for Testing
// ============================================================================

/// Mock event sender for testing purposes.
#[cfg(test)]
pub struct MockCefEventSender {
    pub mouse_moves: std::sync::Mutex<Vec<CefMouseEvent>>,
    pub mouse_clicks: std::sync::Mutex<Vec<(CefMouseEvent, CefMouseButton, bool, i32)>>,
    pub mouse_wheels: std::sync::Mutex<Vec<(CefMouseEvent, i32, i32)>>,
    pub key_events: std::sync::Mutex<Vec<CefKeyEvent>>,
}

#[cfg(test)]
impl MockCefEventSender {
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

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_cef_mouse_button_codes() {
        assert_eq!(CefMouseButton::Left.to_cef_type(), 0);
        assert_eq!(CefMouseButton::Middle.to_cef_type(), 1);
        assert_eq!(CefMouseButton::Right.to_cef_type(), 2);
    }

    #[test]
    fn test_cef_mouse_event() {
        let mut event = CefMouseEvent::new(100, 200);
        assert_eq!(event.x, 100);
        assert_eq!(event.y, 200);
        assert_eq!(event.modifiers, EVENTFLAG_NONE);

        event.add_modifier(EVENTFLAG_SHIFT_DOWN);
        assert_eq!(event.modifiers, EVENTFLAG_SHIFT_DOWN);

        event.add_modifier(EVENTFLAG_CONTROL_DOWN);
        assert_eq!(event.modifiers, EVENTFLAG_SHIFT_DOWN | EVENTFLAG_CONTROL_DOWN);

        event.remove_modifier(EVENTFLAG_SHIFT_DOWN);
        assert_eq!(event.modifiers, EVENTFLAG_CONTROL_DOWN);
    }

    #[test]
    fn test_cef_key_event_types() {
        assert_eq!(CefKeyEventType::RawKeyDown.to_cef_type(), 0);
        assert_eq!(CefKeyEventType::KeyDown.to_cef_type(), 1);
        assert_eq!(CefKeyEventType::KeyUp.to_cef_type(), 2);
        assert_eq!(CefKeyEventType::Char.to_cef_type(), 3);
    }

    #[test]
    fn test_key_name_to_code() {
        // Single characters
        assert_eq!(key_name_to_code("a"), Some('A' as i32));
        assert_eq!(key_name_to_code("A"), Some('A' as i32));
        assert_eq!(key_name_to_code("1"), Some('1' as i32));

        // Special keys
        assert!(key_name_to_code("Enter").is_some());
        assert!(key_name_to_code("Tab").is_some());
        assert!(key_name_to_code("F1").is_some());

        // Invalid
        assert!(key_name_to_code("invalid_key").is_none());
    }

    #[test]
    fn test_modifier_to_event_flag() {
        assert_eq!(modifier_to_event_flag(&Modifier::Shift), EVENTFLAG_SHIFT_DOWN);
        assert_eq!(modifier_to_event_flag(&Modifier::Ctrl), EVENTFLAG_CONTROL_DOWN);
        assert_eq!(modifier_to_event_flag(&Modifier::Alt), EVENTFLAG_ALT_DOWN);
        assert_eq!(modifier_to_event_flag(&Modifier::Meta), EVENTFLAG_COMMAND_DOWN);
    }

    #[test]
    fn test_modifiers_to_event_flags() {
        let mods = vec![Modifier::Shift, Modifier::Ctrl];
        let flags = modifiers_to_event_flags(&mods);
        assert_eq!(flags, EVENTFLAG_SHIFT_DOWN | EVENTFLAG_CONTROL_DOWN);
    }

    #[test]
    fn test_is_shifted_character() {
        assert!(is_shifted_character('!'));
        assert!(is_shifted_character('@'));
        assert!(is_shifted_character('?'));
        assert!(!is_shifted_character('a'));
        assert!(!is_shifted_character('1'));
    }

    #[test]
    fn test_get_key_for_char() {
        assert_eq!(get_key_for_char('!'), "1");
        assert_eq!(get_key_for_char('@'), "2");
        assert_eq!(get_key_for_char('a'), "a");
        assert_eq!(get_key_for_char('A'), "a");
    }

    #[test]
    fn test_calculate_path_points() {
        assert_eq!(calculate_path_points(50.0, 10, 100), 10);
        assert_eq!(calculate_path_points(500.0, 10, 100), 50);
        assert_eq!(calculate_path_points(2000.0, 10, 100), 100);
    }

    #[tokio::test]
    async fn test_mock_event_sender() {
        let sender = MockCefEventSender::new();
        let timing = HumanTiming::instant();
        let mut handler = CefInputHandler::new(sender, timing);

        // Move mouse
        handler.send_mouse_move(100.0, 100.0).await.unwrap();

        // Check events were recorded
        let moves = handler.sender.mouse_moves.lock().unwrap();
        assert!(!moves.is_empty());
        let last_move = moves.last().unwrap();
        assert_eq!(last_move.x, 100);
        assert_eq!(last_move.y, 100);
    }

    #[tokio::test]
    async fn test_mouse_click() {
        let sender = MockCefEventSender::new();
        let timing = HumanTiming::instant();
        let mut handler = CefInputHandler::new(sender, timing);

        handler
            .send_mouse_click(200.0, 150.0, CefMouseButton::Left)
            .await
            .unwrap();

        let clicks = handler.sender.mouse_clicks.lock().unwrap();
        // Should have at least 2 events (down and up)
        assert!(clicks.len() >= 2);
    }

    #[tokio::test]
    async fn test_send_text() {
        let sender = MockCefEventSender::new();
        let timing = HumanTiming::instant();
        let mut handler = CefInputHandler::new(sender, timing);

        handler.send_text("Hi").await.unwrap();

        let events = handler.sender.key_events.lock().unwrap();
        // Should have key events for 'H' (with shift) and 'i'
        assert!(!events.is_empty());
    }

    #[test]
    fn test_position_validation() {
        let sender = MockCefEventSender::new();
        let timing = HumanTiming::instant();
        let mut handler = CefInputHandler::new(sender, timing);
        handler.config.view_bounds = Some((800, 600));

        // Valid position
        assert!(handler.validate_position(400.0, 300.0).is_ok());

        // Out of bounds
        assert!(handler.validate_position(-10.0, 100.0).is_err());
        assert!(handler.validate_position(1000.0, 100.0).is_err());
    }
}
