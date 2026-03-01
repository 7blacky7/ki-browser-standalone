//! Element-Inspector as a separate OS window via egui `show_viewport_deferred`.
//!
//! Shows detailed properties of a right-clicked overlay element with
//! configurable checkbox visibility. The configuration is persisted to
//! `~/.config/ki-browser/inspector.json`.

use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};

use egui::{Color32, RichText, ScrollArea};
use serde::{Deserialize, Serialize};

// ---------------------------------------------------------------------------
// ElementProperty enum
// ---------------------------------------------------------------------------

/// All possible properties that can be displayed for an inspected element.
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
    /// German display label for the property.
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

    /// Whether this property is enabled by default.
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

    /// All property variants in display order.
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

/// All details for an inspected element. Populated from overlay data and
/// (later) JavaScript queries.
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

/// Which properties are visible in the inspector. Persisted to disk.
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
    /// Load config from `~/.config/ki-browser/inspector.json`.
    /// Returns default if file does not exist or is invalid.
    pub fn load() -> Self {
        let path = Self::config_path();
        match std::fs::read_to_string(&path) {
            Ok(content) => serde_json::from_str(&content).unwrap_or_default(),
            Err(_) => Self::default(),
        }
    }

    /// Save config to `~/.config/ki-browser/inspector.json`.
    pub fn save(&self) {
        let path = Self::config_path();
        if let Some(parent) = std::path::Path::new(&path).parent() {
            let _ = std::fs::create_dir_all(parent);
        }
        if let Ok(json) = serde_json::to_string_pretty(self) {
            let _ = std::fs::write(&path, json);
        }
    }

    /// Reset to default settings and persist.
    pub fn reset_to_default(&mut self) {
        *self = Self::default();
        self.save();
    }

    fn config_path() -> String {
        let home = std::env::var("HOME").unwrap_or_else(|_| "/tmp".to_string());
        format!("{}/{}/{}", home, CONFIG_DIR, CONFIG_FILE)
    }
}

// ---------------------------------------------------------------------------
// ElementInspectorState — shared across threads via Arc
// ---------------------------------------------------------------------------

/// Shared state for the Element-Inspector OS window.
///
/// All fields are `Arc`-wrapped so the `show_viewport_deferred` closure
/// (which must be `Send + Sync + 'static`) can safely reference them.
pub struct ElementInspectorState {
    /// Whether the inspector window should be displayed.
    pub open: Arc<AtomicBool>,
    /// The element currently being inspected.
    pub element: Arc<Mutex<Option<ElementDetails>>>,
    /// Checkbox visibility configuration (persisted).
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
// Helper functions
// ---------------------------------------------------------------------------

/// Returns true if the property is an OCR property.
fn is_ocr_property(prop: ElementProperty) -> bool {
    matches!(
        prop,
        ElementProperty::OcrTesseract | ElementProperty::OcrPaddleOcr | ElementProperty::OcrSurya
    )
}

/// Extracts the string value of a property from an `ElementDetails`.
fn get_property_value(details: &ElementDetails, prop: ElementProperty) -> String {
    match prop {
        ElementProperty::Tag => details.tag.clone(),
        ElementProperty::Type => details.element_type.clone(),
        ElementProperty::Title => details.title.clone(),
        ElementProperty::TextValue => details.text_value.clone(),
        ElementProperty::XPath => details.xpath.clone(),
        ElementProperty::FullXPath => details.full_xpath.clone(),
        ElementProperty::CoordX => format!("{:.0}", details.x),
        ElementProperty::CoordY => format!("{:.0}", details.y),
        ElementProperty::Width => format!("{:.0}", details.w),
        ElementProperty::Height => format!("{:.0}", details.h),
        ElementProperty::Role => details.role.clone(),
        ElementProperty::Id => details.id.clone(),
        ElementProperty::Classes => details.classes.clone(),
        ElementProperty::Href => details.href.clone(),
        ElementProperty::Src => details.src.clone(),
        ElementProperty::Placeholder => details.placeholder.clone(),
        ElementProperty::CssSelector => details.css_selector.clone(),
        ElementProperty::IsVisible => match details.is_visible {
            Some(true) => "Ja".to_string(),
            Some(false) => "Nein".to_string(),
            None => String::new(),
        },
        ElementProperty::IsInteractive => match details.is_interactive {
            Some(true) => "Ja".to_string(),
            Some(false) => "Nein".to_string(),
            None => String::new(),
        },
        ElementProperty::OcrTesseract => details.ocr_tesseract.clone(),
        ElementProperty::OcrPaddleOcr => details.ocr_paddleocr.clone(),
        ElementProperty::OcrSurya => details.ocr_surya.clone(),
    }
}

// ---------------------------------------------------------------------------
// Standalone render function for the deferred OS viewport
// ---------------------------------------------------------------------------

/// Renders the Element-Inspector UI inside a deferred viewport (separate OS window).
///
/// Called by the closure passed to `ctx.show_viewport_deferred()`.
pub fn render_standalone(ctx: &egui::Context, state: &ElementInspectorState) {
    // Handle window close request (user clicks X on the OS window)
    if ctx.input(|i| i.viewport().close_requested()) {
        state.open.store(false, Ordering::Relaxed);
        ctx.send_viewport_cmd(egui::ViewportCommand::Close);
        return;
    }

    // Dark theme for the standalone window
    ctx.set_visuals(egui::Visuals::dark());

    // Read current element (clone to release lock quickly)
    let element = state.element.lock().ok().and_then(|e| e.clone());

    egui::CentralPanel::default().show(ctx, |ui| {
        ui.heading(RichText::new("Element-Details").color(Color32::WHITE).strong());
        ui.separator();

        match element {
            None => {
                ui.add_space(20.0);
                ui.centered_and_justified(|ui| {
                    ui.label(
                        RichText::new(
                            "Kein Element ausgewaehlt.\n\
                             Rechtsklick auf ein Overlay-Element und\n\
                             'Element-Details oeffnen' waehlen.",
                        )
                        .color(Color32::from_rgb(100, 100, 115))
                        .italics(),
                    );
                });
            }
            Some(details) => {
                render_element_properties(ui, state, &details);
            }
        }
    });
}

/// Renders the property list with checkboxes for visibility control.
fn render_element_properties(
    ui: &mut egui::Ui,
    state: &ElementInspectorState,
    details: &ElementDetails,
) {
    let mut config = state
        .config
        .lock()
        .map(|c| c.clone())
        .unwrap_or_default();
    let mut config_changed = false;

    ScrollArea::vertical()
        .auto_shrink([false, false])
        .show(ui, |ui| {
            egui::Grid::new("inspector_props_grid")
                .num_columns(3)
                .spacing([8.0, 4.0])
                .striped(true)
                .show(ui, |ui| {
                    for prop in ElementProperty::all() {
                        let value = get_property_value(details, *prop);

                        // Skip empty non-OCR properties
                        if value.is_empty() && !is_ocr_property(*prop) {
                            continue;
                        }

                        let is_enabled = config.enabled.get(prop).copied().unwrap_or(false);

                        // Checkbox
                        let mut checked = is_enabled;
                        if ui.checkbox(&mut checked, "").changed() {
                            config.enabled.insert(*prop, checked);
                            config_changed = true;
                        }

                        // Label
                        ui.label(
                            RichText::new(prop.label())
                                .color(Color32::GRAY)
                                .strong()
                                .size(12.0),
                        );

                        // Value (only show if enabled)
                        if checked {
                            if value.is_empty() {
                                ui.label(
                                    RichText::new("—")
                                        .color(Color32::from_rgb(80, 80, 90))
                                        .italics(),
                                );
                            } else {
                                ui.label(
                                    RichText::new(&value)
                                        .color(Color32::WHITE)
                                        .monospace()
                                        .size(12.0),
                                );
                            }
                        } else {
                            ui.label(
                                RichText::new("(ausgeblendet)")
                                    .color(Color32::from_rgb(60, 60, 70))
                                    .italics()
                                    .size(11.0),
                            );
                        }

                        ui.end_row();
                    }
                });

            ui.add_space(12.0);
            ui.separator();
            ui.add_space(4.0);

            if ui
                .button(
                    RichText::new("Auf Standard zuruecksetzen")
                        .color(Color32::from_rgb(180, 180, 200)),
                )
                .clicked()
            {
                config.reset_to_default();
                if let Ok(mut c) = state.config.lock() {
                    *c = config.clone();
                }
                // Already saved by reset_to_default
                return;
            }
        });

    // Persist config changes
    if config_changed {
        config.save();
        if let Ok(mut c) = state.config.lock() {
            *c = config;
        }
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

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
    fn test_get_property_value() {
        let details = ElementDetails {
            tag: "div".to_string(),
            x: 42.0,
            y: 84.0,
            w: 100.0,
            h: 50.0,
            is_visible: Some(true),
            is_interactive: Some(false),
            ..Default::default()
        };
        assert_eq!(get_property_value(&details, ElementProperty::Tag), "div");
        assert_eq!(get_property_value(&details, ElementProperty::CoordX), "42");
        assert_eq!(
            get_property_value(&details, ElementProperty::IsVisible),
            "Ja"
        );
        assert_eq!(
            get_property_value(&details, ElementProperty::IsInteractive),
            "Nein"
        );
        assert_eq!(
            get_property_value(&details, ElementProperty::Role),
            ""
        );
    }

    #[test]
    fn test_is_ocr_property() {
        assert!(is_ocr_property(ElementProperty::OcrTesseract));
        assert!(is_ocr_property(ElementProperty::OcrPaddleOcr));
        assert!(is_ocr_property(ElementProperty::OcrSurya));
        assert!(!is_ocr_property(ElementProperty::Tag));
        assert!(!is_ocr_property(ElementProperty::Width));
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
}
