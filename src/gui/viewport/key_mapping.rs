//! Mapping from egui key codes to Windows virtual key codes (VK_*) for CEF.
//!
//! CEF's key event API expects Windows-style VK_* codes regardless of the
//! host platform. This module translates egui's `Key` enum into the
//! corresponding VK constants for navigation, editing, function keys,
//! the full A-Z alphabet, and the number row.

/// Maps egui key to Windows virtual key code (VK_*) used by CEF's key event API.
///
/// Covers navigation, editing, function keys, and the full A-Z alphabet
/// so that keyboard shortcuts (Ctrl+S, Ctrl+F, etc.) work correctly.
pub(crate) fn egui_key_to_vk(key: egui::Key) -> i32 {
    match key {
        // Navigation & editing
        egui::Key::Enter => 0x0D,
        egui::Key::Tab => 0x09,
        egui::Key::Backspace => 0x08,
        egui::Key::Escape => 0x1B,
        egui::Key::Space => 0x20,
        egui::Key::Delete => 0x2E,
        egui::Key::Insert => 0x2D,
        egui::Key::Home => 0x24,
        egui::Key::End => 0x23,
        egui::Key::PageUp => 0x21,
        egui::Key::PageDown => 0x22,
        egui::Key::ArrowLeft => 0x25,
        egui::Key::ArrowUp => 0x26,
        egui::Key::ArrowRight => 0x27,
        egui::Key::ArrowDown => 0x28,
        // Function keys (F1-F12)
        egui::Key::F1 => 0x70,
        egui::Key::F2 => 0x71,
        egui::Key::F3 => 0x72,
        egui::Key::F4 => 0x73,
        egui::Key::F5 => 0x74,
        egui::Key::F6 => 0x75,
        egui::Key::F7 => 0x76,
        egui::Key::F8 => 0x77,
        egui::Key::F9 => 0x78,
        egui::Key::F10 => 0x79,
        egui::Key::F11 => 0x7A,
        egui::Key::F12 => 0x7B,
        // Full A-Z alphabet (VK_A = 0x41 .. VK_Z = 0x5A)
        egui::Key::A => 0x41,
        egui::Key::B => 0x42,
        egui::Key::C => 0x43,
        egui::Key::D => 0x44,
        egui::Key::E => 0x45,
        egui::Key::F => 0x46,
        egui::Key::G => 0x47,
        egui::Key::H => 0x48,
        egui::Key::I => 0x49,
        egui::Key::J => 0x4A,
        egui::Key::K => 0x4B,
        egui::Key::L => 0x4C,
        egui::Key::M => 0x4D,
        egui::Key::N => 0x4E,
        egui::Key::O => 0x4F,
        egui::Key::P => 0x50,
        egui::Key::Q => 0x51,
        egui::Key::R => 0x52,
        egui::Key::S => 0x53,
        egui::Key::T => 0x54,
        egui::Key::U => 0x55,
        egui::Key::V => 0x56,
        egui::Key::W => 0x57,
        egui::Key::X => 0x58,
        egui::Key::Y => 0x59,
        egui::Key::Z => 0x5A,
        // Number row (VK_0 = 0x30 .. VK_9 = 0x39)
        egui::Key::Num0 => 0x30,
        egui::Key::Num1 => 0x31,
        egui::Key::Num2 => 0x32,
        egui::Key::Num3 => 0x33,
        egui::Key::Num4 => 0x34,
        egui::Key::Num5 => 0x35,
        egui::Key::Num6 => 0x36,
        egui::Key::Num7 => 0x37,
        egui::Key::Num8 => 0x38,
        egui::Key::Num9 => 0x39,
        _ => 0,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_egui_key_to_vk_navigation_keys() {
        assert_eq!(egui_key_to_vk(egui::Key::Enter), 0x0D);
        assert_eq!(egui_key_to_vk(egui::Key::Tab), 0x09);
        assert_eq!(egui_key_to_vk(egui::Key::Backspace), 0x08);
        assert_eq!(egui_key_to_vk(egui::Key::Escape), 0x1B);
        assert_eq!(egui_key_to_vk(egui::Key::Space), 0x20);
        assert_eq!(egui_key_to_vk(egui::Key::Delete), 0x2E);
        assert_eq!(egui_key_to_vk(egui::Key::Insert), 0x2D);
        assert_eq!(egui_key_to_vk(egui::Key::Home), 0x24);
        assert_eq!(egui_key_to_vk(egui::Key::End), 0x23);
        assert_eq!(egui_key_to_vk(egui::Key::PageUp), 0x21);
        assert_eq!(egui_key_to_vk(egui::Key::PageDown), 0x22);
    }

    #[test]
    fn test_egui_key_to_vk_arrow_keys() {
        assert_eq!(egui_key_to_vk(egui::Key::ArrowLeft), 0x25);
        assert_eq!(egui_key_to_vk(egui::Key::ArrowUp), 0x26);
        assert_eq!(egui_key_to_vk(egui::Key::ArrowRight), 0x27);
        assert_eq!(egui_key_to_vk(egui::Key::ArrowDown), 0x28);
    }

    #[test]
    fn test_egui_key_to_vk_function_keys() {
        assert_eq!(egui_key_to_vk(egui::Key::F1), 0x70);
        assert_eq!(egui_key_to_vk(egui::Key::F5), 0x74);
        assert_eq!(egui_key_to_vk(egui::Key::F12), 0x7B);
    }

    #[test]
    fn test_egui_key_to_vk_alphabet_complete() {
        // Verify all 26 letters map to VK_A (0x41) through VK_Z (0x5A)
        let keys = [
            egui::Key::A, egui::Key::B, egui::Key::C, egui::Key::D,
            egui::Key::E, egui::Key::F, egui::Key::G, egui::Key::H,
            egui::Key::I, egui::Key::J, egui::Key::K, egui::Key::L,
            egui::Key::M, egui::Key::N, egui::Key::O, egui::Key::P,
            egui::Key::Q, egui::Key::R, egui::Key::S, egui::Key::T,
            egui::Key::U, egui::Key::V, egui::Key::W, egui::Key::X,
            egui::Key::Y, egui::Key::Z,
        ];
        for (i, key) in keys.iter().enumerate() {
            assert_eq!(egui_key_to_vk(*key), 0x41 + i as i32, "Key {:?} should map to VK {:#X}", key, 0x41 + i as i32);
        }
    }

    #[test]
    fn test_egui_key_to_vk_number_row() {
        assert_eq!(egui_key_to_vk(egui::Key::Num0), 0x30);
        assert_eq!(egui_key_to_vk(egui::Key::Num5), 0x35);
        assert_eq!(egui_key_to_vk(egui::Key::Num9), 0x39);
    }

    #[test]
    fn test_egui_key_to_vk_unmapped_returns_zero() {
        // Minus key is not mapped and should return 0
        assert_eq!(egui_key_to_vk(egui::Key::Minus), 0);
    }
}
