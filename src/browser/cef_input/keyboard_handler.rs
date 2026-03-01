//! CEF keyboard input methods on `CefInputHandler`.
//!
//! This module extends `CefInputHandler` with keyboard-specific behaviour:
//! single key events, character input (KEYEVENT_CHAR), full text typing with
//! human-like per-character delays, key combos (Ctrl+C, Alt+F4, etc.), and
//! modifier key state tracking.
//!
//! All methods are async and include randomised delays to avoid bot detection
//! via keystroke timing fingerprinting.

use std::time::Duration;

use crate::input::{InputError, InputResult, Modifier};

use super::events::{CefKeyEvent, CefKeyEventType};
use super::keyboard::{get_key_for_char, is_shifted_character, key_name_to_code,
    modifier_to_key_name, modifiers_to_event_flags};
use super::mouse::{CefEventSender, CefInputHandler};

impl<S: CefEventSender> CefInputHandler<S> {
    // ========================================================================
    // Keyboard Input Methods
    // ========================================================================

    /// Sends a single key down or key up event with optional modifier flags.
    ///
    /// Translates `key` to a platform-specific virtual key code via
    /// `key_name_to_code`, updates the internal modifier state if `key` is
    /// itself a modifier key, and delivers the event through the sender.
    ///
    /// # Arguments
    ///
    /// * `key` - Key name or single character (e.g. `"Enter"`, `"a"`, `"F1"`).
    /// * `modifiers` - Additional modifiers active during this event.
    /// * `is_down` - `true` for key down, `false` for key up.
    ///
    /// # Errors
    ///
    /// Returns `InputError::InvalidKey` if `key` is not recognised.
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

        // Set character data for printable single-character keys
        if key.len() == 1 {
            let c = key.chars().next().unwrap();
            event.character = c as u16;
            event.unmodified_character = c.to_ascii_lowercase() as u16;
        }

        // Track modifier key state for subsequent event flags
        if is_down {
            if let Some(modifier) = self.parse_modifier(key) {
                self.active_modifiers.insert(modifier);
            }
        } else if let Some(modifier) = self.parse_modifier(key) {
            self.active_modifiers.remove(&modifier);
        }

        // Brief randomised delay for realistic keystroke timing
        let delay = Duration::from_millis(rand::random::<u64>() % 10 + 2);
        tokio::time::sleep(delay).await;

        self.sender.send_key_event(&event);

        Ok(())
    }

    /// Sends a `KEYEVENT_CHAR` event for direct Unicode character input.
    ///
    /// Used for text input where the Unicode value matters, not the physical
    /// key position. Inherits the current modifier flags.
    ///
    /// # Arguments
    ///
    /// * `c` - The Unicode character to inject.
    pub async fn send_char(&mut self, c: char) -> InputResult<()> {
        let event = CefKeyEvent::char_event(c).with_modifier(self.current_modifier_flags());
        self.sender.send_key_event(&event);
        Ok(())
    }

    /// Types a string with human-like per-character delays and Shift handling.
    ///
    /// For each character: presses Shift if needed (uppercase or shifted symbol),
    /// sends a key-down + KEYEVENT_CHAR + key-up sequence, then waits a
    /// character-frequency-weighted delay before the next character.
    ///
    /// # Arguments
    ///
    /// * `text` - The text to type into the CEF browser.
    ///
    /// # Errors
    ///
    /// Returns `InputError::InvalidKey` if a character cannot be mapped to a key.
    ///
    /// # Example
    ///
    /// ```rust,ignore
    /// handler.send_text("Hello, World!").await?;
    /// ```
    pub async fn send_text(&mut self, text: &str) -> InputResult<()> {
        for c in text.chars() {
            let needs_shift = c.is_uppercase() || is_shifted_character(c);

            if needs_shift {
                self.send_key_event("Shift", &[], true).await?;
            }

            let key = get_key_for_char(c);

            self.send_key_event(&key, &[], true).await?;
            self.send_char(c).await?;

            let hold = self.timing.get_click_delay();
            tokio::time::sleep(hold).await;

            self.send_key_event(&key, &[], false).await?;

            if needs_shift {
                self.send_key_event("Shift", &[], false).await?;
            }

            // Inter-keystroke delay weighted by character typing frequency
            let delay = self.get_char_delay(c);
            tokio::time::sleep(delay).await;
        }

        Ok(())
    }

    /// Sends a key combination such as Ctrl+C, Alt+F4, or Shift+Tab.
    ///
    /// Presses modifiers in order, presses the main key, then releases
    /// everything in reverse order with small inter-event delays.
    ///
    /// # Arguments
    ///
    /// * `key` - The main key name (e.g. `"c"`, `"F4"`, `"Tab"`).
    /// * `modifiers` - Modifier keys to hold (e.g. `&[Modifier::Ctrl]`).
    ///
    /// # Errors
    ///
    /// Returns `InputError::InvalidKey` if `key` is not recognised.
    pub async fn send_key_combo(&mut self, key: &str, modifiers: &[Modifier]) -> InputResult<()> {
        // Press modifiers
        for modifier in modifiers {
            let mod_key = modifier_to_key_name(modifier);
            self.send_key_event(mod_key, &[], true).await?;
            let delay = Duration::from_millis(rand::random::<u64>() % 20 + 10);
            tokio::time::sleep(delay).await;
        }

        // Press and release main key
        self.send_key_event(key, modifiers, true).await?;
        let hold = self.timing.get_click_delay();
        tokio::time::sleep(hold).await;
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

    // ========================================================================
    // Private Keyboard Helpers
    // ========================================================================

    /// Parses a key name into a `Modifier` variant if the key is a modifier key.
    ///
    /// Returns `None` for non-modifier keys.
    fn parse_modifier(&self, key: &str) -> Option<Modifier> {
        match key.to_lowercase().as_str() {
            "shift" => Some(Modifier::Shift),
            "control" | "ctrl" => Some(Modifier::Ctrl),
            "alt" => Some(Modifier::Alt),
            "meta" | "command" | "cmd" | "windows" | "win" => Some(Modifier::Meta),
            _ => None,
        }
    }

    /// Returns a human-like inter-keystroke delay weighted by character frequency.
    ///
    /// Common letters (e, t, a, o, i, n) use a shorter base delay, rare
    /// letters and special characters use a longer multiplier to reflect
    /// realistic typing patterns.
    fn get_char_delay(&self, c: char) -> Duration {
        let base = self.timing.get_type_delay();

        let multiplier = match c {
            // High-frequency letters — fastest
            'e' | 't' | 'a' | 'o' | 'i' | 'n' | 's' | 'h' | 'r' => 0.8,
            // Mid-frequency letters
            'l' | 'd' | 'c' | 'u' | 'm' | 'w' | 'f' | 'g' | 'y' | 'p' | 'b' => 1.0,
            // Low-frequency letters — slower
            'v' | 'k' | 'j' | 'x' | 'q' | 'z' => 1.2,
            // Digit row
            '0'..='9' => 1.1,
            // Space bar (thumb key) — fast
            ' ' => 0.7,
            // Common punctuation
            '.' | ',' => 1.0,
            // Less common punctuation
            '!' | '?' | ':' | ';' => 1.3,
            // Shifted special characters — slowest
            '@' | '#' | '$' | '%' | '^' | '&' | '*' => 1.5,
            // Uppercase adds Shift key overhead
            _ if c.is_uppercase() => 1.2,
            _ => 1.0,
        };

        Duration::from_millis((base.as_millis() as f64 * multiplier) as u64)
    }
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use crate::browser::cef_input::events::{CefKeyEvent, CefMouseButton, CefMouseEvent};
    use crate::browser::cef_input::mouse::{CefEventSender, CefInputHandler};
    use crate::input::timing::HumanTiming;

    struct MockSender {
        keys: std::sync::Mutex<Vec<CefKeyEvent>>,
    }

    impl MockSender {
        fn new() -> Self {
            Self { keys: std::sync::Mutex::new(Vec::new()) }
        }
    }

    impl CefEventSender for MockSender {
        fn send_mouse_move_event(&self, _: &CefMouseEvent, _: bool) {}
        fn send_mouse_click_event(
            &self,
            _: &CefMouseEvent,
            _: CefMouseButton,
            _: bool,
            _: i32,
        ) {}
        fn send_mouse_wheel_event(&self, _: &CefMouseEvent, _: i32, _: i32) {}
        fn send_key_event(&self, event: &CefKeyEvent) {
            self.keys.lock().unwrap().push(event.clone());
        }
    }

    #[tokio::test]
    async fn test_send_text_generates_key_events() {
        let mut handler = CefInputHandler::new(MockSender::new(), HumanTiming::instant());

        handler.send_text("Hi").await.unwrap();

        let events = handler.sender.keys.lock().unwrap();
        // 'H' requires Shift + key down + char + key up + shift up = multiple events
        // 'i' requires key down + char + key up
        assert!(!events.is_empty());
    }

    #[tokio::test]
    async fn test_send_key_event_invalid_key_returns_error() {
        let mut handler = CefInputHandler::new(MockSender::new(), HumanTiming::instant());

        let result = handler.send_key_event("XF86InvalidKey", &[], true).await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_send_key_event_valid_key() {
        let mut handler = CefInputHandler::new(MockSender::new(), HumanTiming::instant());

        handler.send_key_event("Enter", &[], true).await.unwrap();

        let events = handler.sender.keys.lock().unwrap();
        assert_eq!(events.len(), 1);
    }
}
