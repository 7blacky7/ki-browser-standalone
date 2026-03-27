//! Types for the DOM element inspector: property definitions, element details, and shared state.
//!
//! Defines `ElementProperty` (all inspectable DOM attributes), `ElementDetails`
//! (populated from CEF overlay data and JavaScript queries), `InspectorConfig`
//! (persisted checkbox visibility state), and `ElementInspectorState` (shared
//! across threads via `Arc` for the deferred OS viewport).

use std::collections::HashMap;
use std::sync::atomic::AtomicBool;
use std::sync::{Arc, Mutex};

use serde::{Deserialize, Serialize};

// ---------------------------------------------------------------------------
// ElementProperty enum
// ---------------------------------------------------------------------------

/// All possible properties that can be displayed for an inspected DOM element.
///
/// Each variant maps to one row in the inspector property grid. OCR variants
/// (`OcrTesseract`, `OcrPaddleOcr`, `OcrSurya`) are always shown even when
/// the value is empty, to indicate OCR status.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum ElementProperty {
    Tag,
    Type,
    Title,
    TextValue,
    XPath,
    FullXPath,
    CoordX,
    CoordY,
    Width,
    Height,
    Role,
    Id,
    Classes,
    Href,
    Src,
    Placeholder,
    CssSelector,
    IsVisible,
    IsInteractive,
    OcrTesseract,
    OcrPaddleOcr,
    OcrSurya,
}

impl ElementProperty {
    /// German display label for the property shown in the inspector grid.
    pub fn label(&self) -> &'static str {
        match self {
            Self::Tag => "Tag",
            Self::Type => "Typ",
            Self::Title => "Title",
            Self::TextValue => "Text-Inhalt",
            Self::XPath => "XPath",
            Self::FullXPath => "Full XPath",
            Self::CoordX => "X-Koordinate",
            Self::CoordY => "Y-Koordinate",
            Self::Width => "Breite",
            Self::Height => "Hoehe",
            Self::Role => "ARIA-Rolle",
            Self::Id => "ID",
            Self::Classes => "CSS-Klassen",
            Self::Href => "Href",
            Self::Src => "Src",
            Self::Placeholder => "Placeholder",
            Self::CssSelector => "CSS-Selektor",
            Self::IsVisible => "Sichtbar",
            Self::IsInteractive => "Interaktiv",
            Self::OcrTesseract => "OCR Tesseract",
            Self::OcrPaddleOcr => "OCR PaddleOCR",
            Self::OcrSurya => "OCR Surya",
        }
    }

    /// Whether this property is enabled (visible) by default in the inspector.
    ///
    /// Geometric and primary content properties are enabled; ARIA, OCR, and
    /// advanced selector properties are disabled by default to reduce noise.
    pub fn default_enabled(&self) -> bool {
        matches!(
            self,
            Self::Tag
                | Self::Type
                | Self::Title
                | Self::TextValue
                | Self::XPath
                | Self::FullXPath
                | Self::CoordX
                | Self::CoordY
                | Self::Width
                | Self::Height
        )
    }

    /// All property variants in fixed display order for consistent grid layout.
    pub fn all() -> &'static [ElementProperty] {
        &[
            Self::Tag,
            Self::Type,
            Self::Title,
            Self::TextValue,
            Self::XPath,
            Self::FullXPath,
            Self::CoordX,
            Self::CoordY,
            Self::Width,
            Self::Height,
            Self::Role,
            Self::Id,
            Self::Classes,
            Self::Href,
            Self::Src,
            Self::Placeholder,
            Self::CssSelector,
            Self::IsVisible,
            Self::IsInteractive,
            Self::OcrTesseract,
            Self::OcrPaddleOcr,
            Self::OcrSurya,
        ]
    }
}

// ---------------------------------------------------------------------------
// ElementDetails struct
// ---------------------------------------------------------------------------

/// All details for an inspected DOM element.
///
/// Populated from CEF vision overlay data and supplemented by JavaScript
/// queries (XPath, CSS selector, ARIA role). OCR fields are filled
/// asynchronously by the OCR layer if enabled.
#[derive(Clone, Debug, Default)]
pub struct ElementDetails {
    pub tag: String,
    pub element_type: String,
    pub title: String,
    pub text_value: String,
    pub xpath: String,
    pub full_xpath: String,
    pub x: f32,
    pub y: f32,
    pub w: f32,
    pub h: f32,
    pub role: String,
    pub id: String,
    pub classes: String,
    pub href: String,
    pub src: String,
    pub placeholder: String,
    pub css_selector: String,
    pub is_visible: Option<bool>,
    pub is_interactive: Option<bool>,
    pub ocr_tesseract: String,
    pub ocr_paddleocr: String,
    pub ocr_surya: String,
}

// ---------------------------------------------------------------------------
// InspectorConfig — persisted checkbox state
// ---------------------------------------------------------------------------

/// Which properties are visible (checked) in the inspector grid.
///
/// Serialized to `~/.config/ki-browser/inspector.json` so the user's
/// checkbox selections survive restarts.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct InspectorConfig {
    pub enabled: HashMap<ElementProperty, bool>,
}

impl Default for InspectorConfig {
    fn default() -> Self {
        let mut enabled = HashMap::new();
        for prop in ElementProperty::all() {
            enabled.insert(*prop, prop.default_enabled());
        }
        Self { enabled }
    }
}

const CONFIG_DIR: &str = ".config/ki-browser";
const CONFIG_FILE: &str = "inspector.json";

impl InspectorConfig {
    /// Load inspector config from `~/.config/ki-browser/inspector.json`.
    ///
    /// Returns `InspectorConfig::default()` if the file does not exist or
    /// cannot be deserialized (e.g. after a schema change).
    pub fn load() -> Self {
        let path = Self::config_path();
        match std::fs::read_to_string(&path) {
            Ok(content) => serde_json::from_str(&content).unwrap_or_default(),
            Err(_) => Self::default(),
        }
    }

    /// Persist current config to `~/.config/ki-browser/inspector.json`.
    ///
    /// Creates parent directories if they do not exist. Silently ignores
    /// I/O errors to avoid crashing the GUI on read-only filesystems.
    pub fn save(&self) {
        let path = Self::config_path();
        if let Some(parent) = std::path::Path::new(&path).parent() {
            let _ = std::fs::create_dir_all(parent);
        }
        if let Ok(json) = serde_json::to_string_pretty(self) {
            let _ = std::fs::write(&path, json);
        }
    }

    /// Reset all checkbox states to their defaults and persist immediately.
    pub fn reset_to_default(&mut self) {
        *self = Self::default();
        self.save();
    }

    /// Returns the absolute path to the inspector config file.
    fn config_path() -> String {
        let home = std::env::var("HOME").unwrap_or_else(|_| "/tmp".to_string());
        format!("{}/{}/{}", home, CONFIG_DIR, CONFIG_FILE)
    }
}

// ---------------------------------------------------------------------------
// ElementInspectorState — shared across threads via Arc
// ---------------------------------------------------------------------------

/// Shared state for the Element-Inspector OS window rendered via `show_viewport_deferred`.
///
/// All fields are `Arc`-wrapped so the deferred viewport closure
/// (`Send + Sync + 'static`) can safely access them from any thread.
pub struct ElementInspectorState {
    /// Whether the inspector OS window is currently visible.
    pub open: Arc<AtomicBool>,
    /// The DOM element currently being inspected (set on right-click).
    pub element: Arc<Mutex<Option<ElementDetails>>>,
    /// Checkbox visibility configuration, persisted across restarts.
    pub config: Arc<Mutex<InspectorConfig>>,
}

impl Default for ElementInspectorState {
    fn default() -> Self {
        Self {
            open: Arc::new(AtomicBool::new(false)),
            element: Arc::new(Mutex::new(None)),
            config: Arc::new(Mutex::new(InspectorConfig::load())),
        }
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::Ordering;

    #[test]
    fn test_element_property_all_count() {
        assert_eq!(ElementProperty::all().len(), 22);
    }

    #[test]
    fn test_element_property_default_enabled_count() {
        let defaults: Vec<_> = ElementProperty::all()
            .iter()
            .filter(|p| p.default_enabled())
            .collect();
        assert_eq!(defaults.len(), 10);
    }

    #[test]
    fn test_element_property_labels_not_empty() {
        for prop in ElementProperty::all() {
            assert!(!prop.label().is_empty(), "Label for {:?} is empty", prop);
        }
    }

    #[test]
    fn test_inspector_config_default() {
        let config = InspectorConfig::default();
        assert_eq!(config.enabled.len(), 22);
        assert_eq!(config.enabled[&ElementProperty::Tag], true);
        assert_eq!(config.enabled[&ElementProperty::Role], false);
        assert_eq!(config.enabled[&ElementProperty::OcrTesseract], false);
    }

    #[test]
    fn test_inspector_config_serialization() {
        let config = InspectorConfig::default();
        let json = serde_json::to_string(&config).unwrap();
        let parsed: InspectorConfig = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed.enabled.len(), config.enabled.len());
        for prop in ElementProperty::all() {
            assert_eq!(parsed.enabled[prop], config.enabled[prop]);
        }
    }

    #[test]
    fn test_inspector_state_default_is_closed() {
        let state = ElementInspectorState::default();
        assert!(!state.open.load(Ordering::Relaxed));
    }

    #[test]
    fn test_inspector_state_open_toggle() {
        let state = ElementInspectorState::default();
        state.open.store(true, Ordering::Relaxed);
        assert!(state.open.load(Ordering::Relaxed));
        state.open.store(false, Ordering::Relaxed);
        assert!(!state.open.load(Ordering::Relaxed));
    }

    #[test]
    fn test_inspector_state_element_default_none() {
        let state = ElementInspectorState::default();
        let elem = state.element.lock().unwrap();
        assert!(elem.is_none());
    }

    #[test]
    fn test_inspector_state_set_element() {
        let state = ElementInspectorState::default();
        let details = ElementDetails {
            tag: "button".to_string(),
            element_type: "submit".to_string(),
            x: 100.0,
            y: 200.0,
            w: 80.0,
            h: 30.0,
            ..Default::default()
        };
        *state.element.lock().unwrap() = Some(details);
        let elem = state.element.lock().unwrap();
        assert!(elem.is_some());
        assert_eq!(elem.as_ref().unwrap().tag, "button");
    }

    #[test]
    fn test_inspector_config_reset_to_default() {
        let mut config = InspectorConfig::default();
        // Modify some settings
        config.enabled.insert(ElementProperty::Role, true);
        config.enabled.insert(ElementProperty::Tag, false);
        assert!(config.enabled[&ElementProperty::Role]);
        assert!(!config.enabled[&ElementProperty::Tag]);
        // Reset
        config.reset_to_default();
        assert!(!config.enabled[&ElementProperty::Role]);
        assert!(config.enabled[&ElementProperty::Tag]);
    }

    #[test]
    fn test_element_details_default() {
        let details = ElementDetails::default();
        assert!(details.tag.is_empty());
        assert!(details.element_type.is_empty());
        assert_eq!(details.x, 0.0);
        assert_eq!(details.y, 0.0);
        assert!(details.is_visible.is_none());
        assert!(details.is_interactive.is_none());
    }
}
