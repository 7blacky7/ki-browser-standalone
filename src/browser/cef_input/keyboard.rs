//! CEF keyboard input handling: key code tables and conversion utilities.
//!
//! This module provides platform-specific virtual key code constants (Windows VK_*
//! and Linux XKB keysyms), key name to code conversion, modifier flag mapping,
//! and character-level helpers for shifted characters and key-for-char lookup.
//!
//! Key code tables vary by platform:
//! - Windows: Windows virtual key codes (VK_* from winuser.h)
//! - Linux: X11 XKB keysyms (XK_* from X11/keysymdef.h)

use crate::input::Modifier;

use super::events::{
    EVENTFLAG_ALT_DOWN, EVENTFLAG_COMMAND_DOWN, EVENTFLAG_CONTROL_DOWN, EVENTFLAG_NONE,
    EVENTFLAG_SHIFT_DOWN,
};

// ============================================================================
// Platform-Specific Key Code Tables
// ============================================================================

/// Windows virtual key codes (VK_* constants from winuser.h).
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

/// Linux/X11 key codes using XKB keysyms (XK_* from X11/keysymdef.h).
#[cfg(target_os = "linux")]
pub mod key_codes {
    // X11 uses XKB keysyms instead of Windows virtual key codes
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

/// Fallback key codes for platforms other than Windows and Linux.
///
/// Uses Windows-style virtual key codes as the baseline.
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

/// Converts a key name string to a platform-specific virtual key code.
///
/// Handles single-character keys (letters, digits, punctuation) and named
/// special keys ("Enter", "Tab", "F1", "ArrowUp", etc.). Returns `None`
/// for unrecognized key names.
///
/// # Arguments
///
/// * `key` - The key name (e.g., "Enter", "Tab", "a", "F1", "ArrowLeft")
///
/// # Returns
///
/// The platform-specific virtual key code, or `None` if not recognized.
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

    // Named special keys (case-insensitive)
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

/// Converts a `Modifier` enum value to the corresponding CEF EVENTFLAG_* bitmask.
pub fn modifier_to_event_flag(modifier: &Modifier) -> u32 {
    match modifier {
        Modifier::Shift => EVENTFLAG_SHIFT_DOWN,
        Modifier::Ctrl => EVENTFLAG_CONTROL_DOWN,
        Modifier::Alt => EVENTFLAG_ALT_DOWN,
        Modifier::Meta => EVENTFLAG_COMMAND_DOWN,
    }
}

/// Combines a slice of modifiers into a single EVENTFLAG bitmask for CEF events.
pub fn modifiers_to_event_flags(modifiers: &[Modifier]) -> u32 {
    modifiers.iter().fold(EVENTFLAG_NONE, |acc, m| acc | modifier_to_event_flag(m))
}

/// Converts a `Modifier` to the canonical key name string used in key events.
pub fn modifier_to_key_name(modifier: &Modifier) -> &'static str {
    match modifier {
        Modifier::Shift => "Shift",
        Modifier::Ctrl => "Control",
        Modifier::Alt => "Alt",
        Modifier::Meta => "Meta",
    }
}

/// Checks whether a character requires the Shift key to be typed on a standard keyboard.
///
/// Returns `true` for shifted symbols like `!`, `@`, `#`, `?`, `_`, `+`, etc.
pub fn is_shifted_character(c: char) -> bool {
    matches!(c,
        '!' | '@' | '#' | '$' | '%' | '^' | '&' | '*' | '(' | ')' |
        '_' | '+' | '{' | '}' | '|' | ':' | '"' | '<' | '>' | '?' |
        '~'
    )
}

/// Returns the unshifted base key string for a character.
///
/// For shifted characters (e.g., `!` → `"1"`, `@` → `"2"`), returns the
/// base key that must be pressed with Shift. For regular characters, returns
/// the lowercase form.
pub fn get_key_for_char(c: char) -> String {
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

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_key_name_to_code_single_chars() {
        // Alphabetic keys map to their uppercase ASCII codes
        assert_eq!(key_name_to_code("a"), Some('A' as i32));
        assert_eq!(key_name_to_code("A"), Some('A' as i32));
        assert_eq!(key_name_to_code("1"), Some('1' as i32));
    }

    #[test]
    fn test_key_name_to_code_special_keys() {
        assert!(key_name_to_code("Enter").is_some());
        assert!(key_name_to_code("Tab").is_some());
        assert!(key_name_to_code("F1").is_some());
        assert!(key_name_to_code("ArrowUp").is_some());
        assert!(key_name_to_code("Escape").is_some());
    }

    #[test]
    fn test_key_name_to_code_unknown_key() {
        assert!(key_name_to_code("invalid_key").is_none());
        assert!(key_name_to_code("XF86AudioPlay").is_none());
    }

    #[test]
    fn test_modifier_to_event_flag_mapping() {
        assert_eq!(modifier_to_event_flag(&Modifier::Shift), EVENTFLAG_SHIFT_DOWN);
        assert_eq!(modifier_to_event_flag(&Modifier::Ctrl), EVENTFLAG_CONTROL_DOWN);
        assert_eq!(modifier_to_event_flag(&Modifier::Alt), EVENTFLAG_ALT_DOWN);
        assert_eq!(modifier_to_event_flag(&Modifier::Meta), EVENTFLAG_COMMAND_DOWN);
    }

    #[test]
    fn test_modifiers_to_event_flags_combination() {
        let mods = vec![Modifier::Shift, Modifier::Ctrl];
        let flags = modifiers_to_event_flags(&mods);
        assert_eq!(flags, EVENTFLAG_SHIFT_DOWN | EVENTFLAG_CONTROL_DOWN);
    }

    #[test]
    fn test_is_shifted_character_detection() {
        assert!(is_shifted_character('!'));
        assert!(is_shifted_character('@'));
        assert!(is_shifted_character('?'));
        assert!(!is_shifted_character('a'));
        assert!(!is_shifted_character('1'));
    }

    #[test]
    fn test_get_key_for_char_shifted_symbols() {
        assert_eq!(get_key_for_char('!'), "1");
        assert_eq!(get_key_for_char('@'), "2");
        assert_eq!(get_key_for_char('?'), "/");
    }

    #[test]
    fn test_get_key_for_char_regular_chars() {
        assert_eq!(get_key_for_char('a'), "a");
        assert_eq!(get_key_for_char('A'), "a");
        assert_eq!(get_key_for_char('z'), "z");
    }
}
