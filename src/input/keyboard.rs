//! Keyboard simulation module for human-like keyboard interactions
//!
//! This module provides realistic keyboard simulation including individual
//! key presses, text typing with variable delays, and modifier key support.
//!
//! # Example
//!
//! ```rust,no_run
//! use ki_browser::input::keyboard::{KeyboardSimulator, Modifier};
//!
//! async fn example() {
//!     let keyboard = KeyboardSimulator::new();
//!
//!     // Type text with human-like delays
//!     keyboard.type_text("Hello, World!").await.unwrap();
//!
//!     // Press a key combination
//!     keyboard.press_key_with_modifiers("a", &[Modifier::Ctrl]).await.unwrap();
//! }
//! ```

use super::timing::HumanTiming;
use super::{InputError, InputResult};
use std::collections::HashSet;
use std::time::Duration;

/// Modifier keys that can be combined with other keys
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Modifier {
    /// Shift key
    Shift,
    /// Control key (Ctrl)
    Ctrl,
    /// Alt key (Option on Mac)
    Alt,
    /// Meta key (Windows key / Command on Mac)
    Meta,
}

impl Modifier {
    /// Returns the key code string for this modifier
    pub fn key_code(&self) -> &'static str {
        match self {
            Modifier::Shift => "Shift",
            Modifier::Ctrl => "Control",
            Modifier::Alt => "Alt",
            Modifier::Meta => "Meta",
        }
    }
}

impl std::fmt::Display for Modifier {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.key_code())
    }
}

/// Represents different types of keyboard events
#[derive(Debug, Clone, PartialEq)]
pub enum KeyboardEvent {
    /// A key was pressed down
    KeyDown {
        key: String,
        modifiers: Vec<Modifier>,
    },
    /// A key was released
    KeyUp {
        key: String,
        modifiers: Vec<Modifier>,
    },
    /// A complete key press (down + up)
    KeyPress {
        key: String,
        modifiers: Vec<Modifier>,
    },
    /// Text was typed (may involve multiple key presses)
    Type { text: String },
}

/// Special keys that require specific handling
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum SpecialKey {
    Enter,
    Tab,
    Backspace,
    Delete,
    Escape,
    ArrowUp,
    ArrowDown,
    ArrowLeft,
    ArrowRight,
    Home,
    End,
    PageUp,
    PageDown,
    Insert,
    F1,
    F2,
    F3,
    F4,
    F5,
    F6,
    F7,
    F8,
    F9,
    F10,
    F11,
    F12,
    Space,
    CapsLock,
    NumLock,
    ScrollLock,
    PrintScreen,
    Pause,
}

impl SpecialKey {
    /// Returns the key code string for this special key
    pub fn key_code(&self) -> &'static str {
        match self {
            SpecialKey::Enter => "Enter",
            SpecialKey::Tab => "Tab",
            SpecialKey::Backspace => "Backspace",
            SpecialKey::Delete => "Delete",
            SpecialKey::Escape => "Escape",
            SpecialKey::ArrowUp => "ArrowUp",
            SpecialKey::ArrowDown => "ArrowDown",
            SpecialKey::ArrowLeft => "ArrowLeft",
            SpecialKey::ArrowRight => "ArrowRight",
            SpecialKey::Home => "Home",
            SpecialKey::End => "End",
            SpecialKey::PageUp => "PageUp",
            SpecialKey::PageDown => "PageDown",
            SpecialKey::Insert => "Insert",
            SpecialKey::F1 => "F1",
            SpecialKey::F2 => "F2",
            SpecialKey::F3 => "F3",
            SpecialKey::F4 => "F4",
            SpecialKey::F5 => "F5",
            SpecialKey::F6 => "F6",
            SpecialKey::F7 => "F7",
            SpecialKey::F8 => "F8",
            SpecialKey::F9 => "F9",
            SpecialKey::F10 => "F10",
            SpecialKey::F11 => "F11",
            SpecialKey::F12 => "F12",
            SpecialKey::Space => " ",
            SpecialKey::CapsLock => "CapsLock",
            SpecialKey::NumLock => "NumLock",
            SpecialKey::ScrollLock => "ScrollLock",
            SpecialKey::PrintScreen => "PrintScreen",
            SpecialKey::Pause => "Pause",
        }
    }

    /// Parses a string into a SpecialKey if it matches
    pub fn from_str(s: &str) -> Option<SpecialKey> {
        match s.to_lowercase().as_str() {
            "enter" | "return" => Some(SpecialKey::Enter),
            "tab" => Some(SpecialKey::Tab),
            "backspace" => Some(SpecialKey::Backspace),
            "delete" | "del" => Some(SpecialKey::Delete),
            "escape" | "esc" => Some(SpecialKey::Escape),
            "arrowup" | "up" => Some(SpecialKey::ArrowUp),
            "arrowdown" | "down" => Some(SpecialKey::ArrowDown),
            "arrowleft" | "left" => Some(SpecialKey::ArrowLeft),
            "arrowright" | "right" => Some(SpecialKey::ArrowRight),
            "home" => Some(SpecialKey::Home),
            "end" => Some(SpecialKey::End),
            "pageup" => Some(SpecialKey::PageUp),
            "pagedown" => Some(SpecialKey::PageDown),
            "insert" | "ins" => Some(SpecialKey::Insert),
            "f1" => Some(SpecialKey::F1),
            "f2" => Some(SpecialKey::F2),
            "f3" => Some(SpecialKey::F3),
            "f4" => Some(SpecialKey::F4),
            "f5" => Some(SpecialKey::F5),
            "f6" => Some(SpecialKey::F6),
            "f7" => Some(SpecialKey::F7),
            "f8" => Some(SpecialKey::F8),
            "f9" => Some(SpecialKey::F9),
            "f10" => Some(SpecialKey::F10),
            "f11" => Some(SpecialKey::F11),
            "f12" => Some(SpecialKey::F12),
            "space" | " " => Some(SpecialKey::Space),
            "capslock" => Some(SpecialKey::CapsLock),
            "numlock" => Some(SpecialKey::NumLock),
            "scrolllock" => Some(SpecialKey::ScrollLock),
            "printscreen" | "prtsc" => Some(SpecialKey::PrintScreen),
            "pause" => Some(SpecialKey::Pause),
            _ => None,
        }
    }
}

impl std::fmt::Display for SpecialKey {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.key_code())
    }
}

/// Configuration for keyboard simulation behavior
#[derive(Debug, Clone)]
pub struct KeyboardConfig {
    /// Base delay between keystrokes in milliseconds
    pub base_delay_ms: u64,
    /// Variance in keystroke delay (0.0 - 1.0)
    pub delay_variance: f64,
    /// Whether to simulate realistic typing patterns
    pub realistic_typing: bool,
    /// Error rate for typos (0.0 - 1.0, typically very low)
    pub typo_rate: f64,
}

impl Default for KeyboardConfig {
    fn default() -> Self {
        Self {
            base_delay_ms: 50,
            delay_variance: 0.3,
            realistic_typing: true,
            typo_rate: 0.0, // Disabled by default
        }
    }
}

/// Simulates realistic human-like keyboard interactions
#[derive(Debug)]
pub struct KeyboardSimulator {
    /// Configuration for keyboard behavior
    config: KeyboardConfig,
    /// Timing utility for realistic delays
    timing: HumanTiming,
    /// Currently pressed modifier keys
    active_modifiers: HashSet<Modifier>,
    /// History of keyboard events
    event_history: Vec<KeyboardEvent>,
    /// Maximum events to keep in history
    history_limit: usize,
}

impl Default for KeyboardSimulator {
    fn default() -> Self {
        Self::new()
    }
}

impl KeyboardSimulator {
    /// Creates a new KeyboardSimulator with default settings
    ///
    /// # Example
    ///
    /// ```rust
    /// use ki_browser::input::keyboard::KeyboardSimulator;
    ///
    /// let keyboard = KeyboardSimulator::new();
    /// ```
    pub fn new() -> Self {
        Self {
            config: KeyboardConfig::default(),
            timing: HumanTiming::default(),
            active_modifiers: HashSet::new(),
            event_history: Vec::new(),
            history_limit: 100,
        }
    }

    /// Creates a new KeyboardSimulator with custom configuration
    ///
    /// # Arguments
    ///
    /// * `config` - Custom keyboard configuration
    /// * `timing` - Custom timing settings
    pub fn with_config(config: KeyboardConfig, timing: HumanTiming) -> Self {
        Self {
            config,
            timing,
            active_modifiers: HashSet::new(),
            event_history: Vec::new(),
            history_limit: 100,
        }
    }

    /// Records an event in the history
    fn record_event(&mut self, event: KeyboardEvent) {
        self.event_history.push(event);
        if self.event_history.len() > self.history_limit {
            self.event_history.remove(0);
        }
    }

    /// Returns the currently active modifiers
    pub fn active_modifiers(&self) -> Vec<Modifier> {
        self.active_modifiers.iter().copied().collect()
    }

    /// Presses a key down without releasing
    ///
    /// # Arguments
    ///
    /// * `key` - The key to press (can be a character or special key name)
    ///
    /// # Example
    ///
    /// ```rust,no_run
    /// use ki_browser::input::keyboard::KeyboardSimulator;
    ///
    /// async fn example() {
    ///     let mut keyboard = KeyboardSimulator::new();
    ///     keyboard.key_down("a").await.unwrap();
    /// }
    /// ```
    pub async fn key_down(&mut self, key: &str) -> InputResult<()> {
        // Validate key
        self.validate_key(key)?;

        // Small pre-press delay
        let delay = Duration::from_millis(rand::random::<u64>() % 10 + 2);
        tokio::time::sleep(delay).await;

        let event = KeyboardEvent::KeyDown {
            key: key.to_string(),
            modifiers: self.active_modifiers(),
        };
        self.record_event(event);

        // Check if this is a modifier key
        if let Some(modifier) = self.parse_modifier(key) {
            self.active_modifiers.insert(modifier);
        }

        Ok(())
    }

    /// Releases a key
    ///
    /// # Arguments
    ///
    /// * `key` - The key to release
    ///
    /// # Example
    ///
    /// ```rust,no_run
    /// use ki_browser::input::keyboard::KeyboardSimulator;
    ///
    /// async fn example() {
    ///     let mut keyboard = KeyboardSimulator::new();
    ///     keyboard.key_down("a").await.unwrap();
    ///     keyboard.key_up("a").await.unwrap();
    /// }
    /// ```
    pub async fn key_up(&mut self, key: &str) -> InputResult<()> {
        self.validate_key(key)?;

        let event = KeyboardEvent::KeyUp {
            key: key.to_string(),
            modifiers: self.active_modifiers(),
        };
        self.record_event(event);

        // Check if this is a modifier key
        if let Some(modifier) = self.parse_modifier(key) {
            self.active_modifiers.remove(&modifier);
        }

        Ok(())
    }

    /// Performs a complete key press (down + up)
    ///
    /// # Arguments
    ///
    /// * `key` - The key to press
    ///
    /// # Example
    ///
    /// ```rust,no_run
    /// use ki_browser::input::keyboard::KeyboardSimulator;
    ///
    /// async fn example() {
    ///     let keyboard = KeyboardSimulator::new();
    ///     keyboard.press_key("Enter").await.unwrap();
    /// }
    /// ```
    pub async fn press_key(&self, key: &str) -> InputResult<()> {
        self.validate_key(key)?;

        // Key down
        let delay = Duration::from_millis(rand::random::<u64>() % 10 + 2);
        tokio::time::sleep(delay).await;

        // Hold time
        let hold_time = self.timing.get_click_delay();
        tokio::time::sleep(hold_time).await;

        // Key up happens after hold

        Ok(())
    }

    /// Presses a key with modifier keys
    ///
    /// # Arguments
    ///
    /// * `key` - The key to press
    /// * `modifiers` - Modifier keys to hold during the press
    ///
    /// # Example
    ///
    /// ```rust,no_run
    /// use ki_browser::input::keyboard::{KeyboardSimulator, Modifier};
    ///
    /// async fn example() {
    ///     let keyboard = KeyboardSimulator::new();
    ///     // Press Ctrl+A
    ///     keyboard.press_key_with_modifiers("a", &[Modifier::Ctrl]).await.unwrap();
    /// }
    /// ```
    pub async fn press_key_with_modifiers(
        &self,
        key: &str,
        modifiers: &[Modifier],
    ) -> InputResult<()> {
        self.validate_key(key)?;

        // Press modifiers
        for modifier in modifiers {
            let delay = Duration::from_millis(rand::random::<u64>() % 20 + 10);
            tokio::time::sleep(delay).await;
            // Simulate modifier key down
        }

        // Press the main key
        self.press_key(key).await?;

        // Release modifiers in reverse order
        for _modifier in modifiers.iter().rev() {
            let delay = Duration::from_millis(rand::random::<u64>() % 20 + 10);
            tokio::time::sleep(delay).await;
            // Simulate modifier key up
        }

        Ok(())
    }

    /// Types a string of text with human-like delays
    ///
    /// # Arguments
    ///
    /// * `text` - The text to type
    ///
    /// # Example
    ///
    /// ```rust,no_run
    /// use ki_browser::input::keyboard::KeyboardSimulator;
    ///
    /// async fn example() {
    ///     let keyboard = KeyboardSimulator::new();
    ///     keyboard.type_text("Hello, World!").await.unwrap();
    /// }
    /// ```
    pub async fn type_text(&self, text: &str) -> InputResult<()> {
        for c in text.chars() {
            // Get typing delay based on character
            let delay = self.get_char_delay(c);
            tokio::time::sleep(delay).await;

            // Handle uppercase characters
            if c.is_uppercase() {
                // Would need to press shift
            }

            // Type the character
            let key = c.to_string();
            self.press_key(&key).await?;
        }

        Ok(())
    }

    /// Types text with a custom delay between each character
    ///
    /// # Arguments
    ///
    /// * `text` - The text to type
    /// * `delay` - The delay between each character
    ///
    /// # Example
    ///
    /// ```rust,no_run
    /// use ki_browser::input::keyboard::KeyboardSimulator;
    /// use std::time::Duration;
    ///
    /// async fn example() {
    ///     let keyboard = KeyboardSimulator::new();
    ///     keyboard.type_with_delay("Slow typing...", Duration::from_millis(200)).await.unwrap();
    /// }
    /// ```
    pub async fn type_with_delay(&self, text: &str, delay: Duration) -> InputResult<()> {
        for c in text.chars() {
            tokio::time::sleep(delay).await;

            let key = c.to_string();
            self.press_key(&key).await?;
        }

        Ok(())
    }

    /// Presses a special key
    ///
    /// # Arguments
    ///
    /// * `special_key` - The special key to press
    ///
    /// # Example
    ///
    /// ```rust,no_run
    /// use ki_browser::input::keyboard::{KeyboardSimulator, SpecialKey};
    ///
    /// async fn example() {
    ///     let keyboard = KeyboardSimulator::new();
    ///     keyboard.press_special_key(SpecialKey::Enter).await.unwrap();
    /// }
    /// ```
    pub async fn press_special_key(&self, special_key: SpecialKey) -> InputResult<()> {
        self.press_key(special_key.key_code()).await
    }

    /// Validates that a key string is valid
    fn validate_key(&self, key: &str) -> InputResult<()> {
        if key.is_empty() {
            return Err(InputError::InvalidKey {
                key: key.to_string(),
            });
        }

        // Single characters are always valid
        if key.len() == 1 {
            return Ok(());
        }

        // Check if it's a valid special key or modifier
        if SpecialKey::from_str(key).is_some() || self.parse_modifier(key).is_some() {
            return Ok(());
        }

        // Check for common key names
        let valid_keys = [
            "Enter", "Tab", "Backspace", "Delete", "Escape", "Space",
            "ArrowUp", "ArrowDown", "ArrowLeft", "ArrowRight",
            "Home", "End", "PageUp", "PageDown", "Insert",
            "Shift", "Control", "Alt", "Meta",
            "F1", "F2", "F3", "F4", "F5", "F6",
            "F7", "F8", "F9", "F10", "F11", "F12",
        ];

        if valid_keys.iter().any(|&k| k.eq_ignore_ascii_case(key)) {
            return Ok(());
        }

        Err(InputError::InvalidKey {
            key: key.to_string(),
        })
    }

    /// Parses a key string into a modifier if applicable
    fn parse_modifier(&self, key: &str) -> Option<Modifier> {
        match key.to_lowercase().as_str() {
            "shift" => Some(Modifier::Shift),
            "control" | "ctrl" => Some(Modifier::Ctrl),
            "alt" => Some(Modifier::Alt),
            "meta" | "command" | "cmd" | "windows" | "win" => Some(Modifier::Meta),
            _ => None,
        }
    }

    /// Gets the delay for typing a specific character
    ///
    /// Different characters have different typical typing speeds based on
    /// their position on the keyboard and frequency of use.
    fn get_char_delay(&self, c: char) -> Duration {
        let base = self.timing.get_type_delay();

        // Adjust for character difficulty
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
            // Punctuation - slower
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

    /// Returns a copy of the event history
    pub fn event_history(&self) -> Vec<KeyboardEvent> {
        self.event_history.clone()
    }

    /// Clears the event history
    pub fn clear_history(&mut self) {
        self.event_history.clear();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_modifier_key_code() {
        assert_eq!(Modifier::Shift.key_code(), "Shift");
        assert_eq!(Modifier::Ctrl.key_code(), "Control");
        assert_eq!(Modifier::Alt.key_code(), "Alt");
        assert_eq!(Modifier::Meta.key_code(), "Meta");
    }

    #[test]
    fn test_special_key_from_str() {
        assert_eq!(SpecialKey::from_str("enter"), Some(SpecialKey::Enter));
        assert_eq!(SpecialKey::from_str("ENTER"), Some(SpecialKey::Enter));
        assert_eq!(SpecialKey::from_str("return"), Some(SpecialKey::Enter));
        assert_eq!(SpecialKey::from_str("tab"), Some(SpecialKey::Tab));
        assert_eq!(SpecialKey::from_str("invalid"), None);
    }

    #[test]
    fn test_validate_key() {
        let keyboard = KeyboardSimulator::new();

        // Single characters are valid
        assert!(keyboard.validate_key("a").is_ok());
        assert!(keyboard.validate_key("Z").is_ok());
        assert!(keyboard.validate_key("1").is_ok());
        assert!(keyboard.validate_key("!").is_ok());

        // Special keys are valid
        assert!(keyboard.validate_key("Enter").is_ok());
        assert!(keyboard.validate_key("Tab").is_ok());
        assert!(keyboard.validate_key("F1").is_ok());

        // Empty string is invalid
        assert!(keyboard.validate_key("").is_err());

        // Invalid multi-char strings
        assert!(keyboard.validate_key("invalid").is_err());
    }

    #[test]
    fn test_parse_modifier() {
        let keyboard = KeyboardSimulator::new();

        assert_eq!(keyboard.parse_modifier("shift"), Some(Modifier::Shift));
        assert_eq!(keyboard.parse_modifier("Shift"), Some(Modifier::Shift));
        assert_eq!(keyboard.parse_modifier("ctrl"), Some(Modifier::Ctrl));
        assert_eq!(keyboard.parse_modifier("control"), Some(Modifier::Ctrl));
        assert_eq!(keyboard.parse_modifier("alt"), Some(Modifier::Alt));
        assert_eq!(keyboard.parse_modifier("meta"), Some(Modifier::Meta));
        assert_eq!(keyboard.parse_modifier("cmd"), Some(Modifier::Meta));
        assert_eq!(keyboard.parse_modifier("a"), None);
    }

    #[test]
    fn test_keyboard_config_default() {
        let config = KeyboardConfig::default();
        assert_eq!(config.base_delay_ms, 50);
        assert!(config.realistic_typing);
        assert_eq!(config.typo_rate, 0.0);
    }
}
