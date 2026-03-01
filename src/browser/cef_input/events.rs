//! CEF event type definitions for mouse and keyboard input simulation.
//!
//! This module provides the core data structures for CEF native input events,
//! including mouse button types, mouse event structures, key event types, and
//! key event structures. These types map directly to CEF's internal event
//! representation and the EVENTFLAG_* constants from cef_types.h.

// ============================================================================
// CEF Event Flags (from cef_types.h)
// ============================================================================

pub(crate) const EVENTFLAG_NONE: u32 = 0;
pub(crate) const EVENTFLAG_CAPS_LOCK_ON: u32 = 1 << 0;
pub(crate) const EVENTFLAG_SHIFT_DOWN: u32 = 1 << 1;
pub(crate) const EVENTFLAG_CONTROL_DOWN: u32 = 1 << 2;
pub(crate) const EVENTFLAG_ALT_DOWN: u32 = 1 << 3;
pub(crate) const EVENTFLAG_LEFT_MOUSE_BUTTON: u32 = 1 << 4;
pub(crate) const EVENTFLAG_MIDDLE_MOUSE_BUTTON: u32 = 1 << 5;
pub(crate) const EVENTFLAG_RIGHT_MOUSE_BUTTON: u32 = 1 << 6;
/// Command key on Mac, Windows key on Windows.
pub(crate) const EVENTFLAG_COMMAND_DOWN: u32 = 1 << 7;
pub(crate) const EVENTFLAG_NUM_LOCK_ON: u32 = 1 << 8;
pub(crate) const EVENTFLAG_IS_KEY_PAD: u32 = 1 << 9;
pub(crate) const EVENTFLAG_IS_LEFT: u32 = 1 << 10;
pub(crate) const EVENTFLAG_IS_RIGHT: u32 = 1 << 11;
pub(crate) const EVENTFLAG_ALTGR_DOWN: u32 = 1 << 12;
pub(crate) const EVENTFLAG_IS_REPEAT: u32 = 1 << 13;

// ============================================================================
// Mouse Event Types
// ============================================================================

/// Mouse button types for CEF mouse click events.
///
/// Maps to CEF's cef_mouse_button_type_t enum (MBT_LEFT, MBT_MIDDLE, MBT_RIGHT).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum CefMouseButton {
    /// Left mouse button (primary click, MBT_LEFT = 0).
    Left,
    /// Middle mouse button (scroll wheel click, MBT_MIDDLE = 1).
    Middle,
    /// Right mouse button (context menu trigger, MBT_RIGHT = 2).
    Right,
}

impl CefMouseButton {
    /// Returns the CEF-specific button type integer constant for FFI calls.
    ///
    /// CEF uses MBT_LEFT=0, MBT_MIDDLE=1, MBT_RIGHT=2.
    pub fn to_cef_type(&self) -> i32 {
        match self {
            CefMouseButton::Left => 0,   // MBT_LEFT
            CefMouseButton::Middle => 1, // MBT_MIDDLE
            CefMouseButton::Right => 2,  // MBT_RIGHT
        }
    }

    /// Returns the EVENTFLAG bitmask for this button being pressed.
    ///
    /// Used in `CefMouseEvent::modifiers` when this button is held down.
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

/// CEF mouse event structure carrying position and modifier state.
///
/// Passed to CEF for all mouse events (move, click, wheel). The `modifiers`
/// field is a combination of EVENTFLAG_* constants indicating which keys
/// and buttons are currently held.
#[derive(Debug, Clone, Copy)]
pub struct CefMouseEvent {
    /// X coordinate in view coordinates (pixels from left edge).
    pub x: i32,
    /// Y coordinate in view coordinates (pixels from top edge).
    pub y: i32,
    /// Bitmask of active EVENTFLAG_* constants (keyboard + mouse modifiers).
    pub modifiers: u32,
}

impl CefMouseEvent {
    /// Creates a new mouse event at the specified coordinates with no modifiers.
    pub fn new(x: i32, y: i32) -> Self {
        Self {
            x,
            y,
            modifiers: EVENTFLAG_NONE,
        }
    }

    /// Creates a mouse event with pre-set modifier flags.
    pub fn with_modifiers(x: i32, y: i32, modifiers: u32) -> Self {
        Self { x, y, modifiers }
    }

    /// Adds a modifier flag bitmask to this event (OR operation).
    pub fn add_modifier(&mut self, flag: u32) {
        self.modifiers |= flag;
    }

    /// Removes a modifier flag bitmask from this event (AND NOT operation).
    pub fn remove_modifier(&mut self, flag: u32) {
        self.modifiers &= !flag;
    }
}

impl Default for CefMouseEvent {
    fn default() -> Self {
        Self::new(0, 0)
    }
}

// ============================================================================
// Keyboard Event Types
// ============================================================================

/// CEF key event type discriminant, mapping to KEYEVENT_* constants.
///
/// Controls whether CEF processes the event as a raw key, translated key,
/// character input, or key release.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CefKeyEventType {
    /// Raw key down (KEYEVENT_RAWKEYDOWN = 0), no character translation.
    RawKeyDown,
    /// Translated key down (KEYEVENT_KEYDOWN = 1).
    KeyDown,
    /// Key released (KEYEVENT_KEYUP = 2).
    KeyUp,
    /// Character input event (KEYEVENT_CHAR = 3), triggers text input.
    Char,
}

impl CefKeyEventType {
    /// Returns the CEF integer constant for this key event type (KEYEVENT_*).
    pub fn to_cef_type(&self) -> i32 {
        match self {
            CefKeyEventType::RawKeyDown => 0, // KEYEVENT_RAWKEYDOWN
            CefKeyEventType::KeyDown => 1,    // KEYEVENT_KEYDOWN
            CefKeyEventType::KeyUp => 2,      // KEYEVENT_KEYUP
            CefKeyEventType::Char => 3,       // KEYEVENT_CHAR
        }
    }
}

/// CEF key event structure for keyboard input simulation.
///
/// Represents a single keyboard event with Windows virtual key code,
/// platform-specific native code, character data, and modifier state.
/// Both `windows_key_code` and `native_key_code` must be set correctly
/// for cross-platform CEF compatibility.
#[derive(Debug, Clone)]
pub struct CefKeyEvent {
    /// The type of key event (raw down, translated down, up, or char).
    pub event_type: CefKeyEventType,
    /// Bitmask of active EVENTFLAG_* constants.
    pub modifiers: u32,
    /// Windows virtual key code (VK_* constant).
    pub windows_key_code: i32,
    /// Native platform-specific key code (XKB keysym on Linux).
    pub native_key_code: i32,
    /// Whether this is a system key event (Alt combinations on Windows).
    pub is_system_key: bool,
    /// The Unicode character generated by the keystroke.
    pub character: u16,
    /// Same character ignoring Shift and similar modifiers.
    pub unmodified_character: u16,
    /// True when the browser focus is on an editable text field.
    pub focus_on_editable_field: bool,
}

impl CefKeyEvent {
    /// Creates a new key event with the given type and Windows virtual key code.
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

    /// Creates a KEYEVENT_CHAR event for direct Unicode character input.
    ///
    /// Used for text typing where the character value matters more than
    /// the physical key position.
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

    /// Builder: sets the native (platform-specific) key code.
    pub fn with_native_key_code(mut self, code: i32) -> Self {
        self.native_key_code = code;
        self
    }

    /// Builder: sets the generated character for this event.
    pub fn with_character(mut self, character: char) -> Self {
        self.character = character as u16;
        self.unmodified_character = character as u16;
        self
    }

    /// Builder: adds an EVENTFLAG_* modifier bitmask to this event.
    pub fn with_modifier(mut self, flag: u32) -> Self {
        self.modifiers |= flag;
        self
    }

    /// Builder: marks this event as a system key (Alt combinations on Windows).
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
    fn test_cef_mouse_event_modifier_operations() {
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
    fn test_cef_key_event_type_constants() {
        assert_eq!(CefKeyEventType::RawKeyDown.to_cef_type(), 0);
        assert_eq!(CefKeyEventType::KeyDown.to_cef_type(), 1);
        assert_eq!(CefKeyEventType::KeyUp.to_cef_type(), 2);
        assert_eq!(CefKeyEventType::Char.to_cef_type(), 3);
    }

    #[test]
    fn test_cef_key_event_char_event_encoding() {
        let event = CefKeyEvent::char_event('A');
        assert_eq!(event.event_type, CefKeyEventType::Char);
        assert_eq!(event.character, 'A' as u16);
        assert_eq!(event.unmodified_character, 'A' as u16);
    }
}
