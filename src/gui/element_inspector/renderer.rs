//! Renders the element inspector as a standalone OS window with property grid.
//!
//! Provides `render_standalone` (entry point for `show_viewport_deferred`) and
//! the internal `render_element_properties` grid. Properties are shown in a
//! striped egui `Grid` with per-row checkboxes that persist to disk via
//! `InspectorConfig::save`.

use egui::{Color32, RichText, ScrollArea};

use super::types::{ElementDetails, ElementInspectorState, ElementProperty};

use std::sync::atomic::Ordering;

// ---------------------------------------------------------------------------
// Helper functions
// ---------------------------------------------------------------------------

/// Returns true if the given property represents an OCR engine result.
///
/// OCR properties (`OcrTesseract`, `OcrPaddleOcr`, `OcrSurya`) are always
/// rendered in the grid even when their value is empty, to show OCR status.
fn is_ocr_property(prop: ElementProperty) -> bool {
    matches!(
        prop,
        ElementProperty::OcrTesseract | ElementProperty::OcrPaddleOcr | ElementProperty::OcrSurya
    )
}

/// Extracts the display string for a given `ElementProperty` from `ElementDetails`.
///
/// Boolean fields (`is_visible`, `is_interactive`) are formatted as "Ja"/"Nein".
/// Coordinate and size fields are formatted without decimal places.
/// Returns an empty string when the field value is unset or `None`.
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
// Standalone render entry point
// ---------------------------------------------------------------------------

/// Renders the Element-Inspector UI inside a deferred egui viewport (separate OS window).
///
/// Called by the closure passed to `ctx.show_viewport_deferred()` in `browser_app`.
/// Handles the OS-level close button by setting `state.open` to false and sending
/// `ViewportCommand::Close`. Applies a dark theme independent of the main window.
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

// ---------------------------------------------------------------------------
// Property grid renderer
// ---------------------------------------------------------------------------

/// Renders the striped property grid with per-row checkboxes for visibility control.
///
/// Each row shows a checkbox, a label, and the property value. Rows with empty
/// values are skipped unless they are OCR properties. Config changes are debounced
/// and written to disk once after all rows are processed.
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

                        // Skip empty non-OCR properties to keep grid compact
                        if value.is_empty() && !is_ocr_property(*prop) {
                            continue;
                        }

                        let is_enabled = config.enabled.get(prop).copied().unwrap_or(false);

                        // Checkbox column
                        let mut checked = is_enabled;
                        if ui.checkbox(&mut checked, "").changed() {
                            config.enabled.insert(*prop, checked);
                            config_changed = true;
                        }

                        // Label column
                        ui.label(
                            RichText::new(prop.label())
                                .color(Color32::GRAY)
                                .strong()
                                .size(12.0),
                        );

                        // Value column: show value if enabled, placeholder if hidden
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
            }
        });

    // Persist config changes accumulated during grid rendering
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
    use crate::gui::element_inspector::types::ElementDetails;

    #[test]
    fn test_get_property_value_tag() {
        let details = ElementDetails {
            tag: "div".to_string(),
            ..Default::default()
        };
        assert_eq!(get_property_value(&details, ElementProperty::Tag), "div");
    }

    #[test]
    fn test_get_property_value_coordinates_formatted() {
        let details = ElementDetails {
            x: 42.0,
            y: 84.0,
            w: 100.0,
            h: 50.0,
            ..Default::default()
        };
        assert_eq!(get_property_value(&details, ElementProperty::CoordX), "42");
        assert_eq!(get_property_value(&details, ElementProperty::CoordY), "84");
        assert_eq!(get_property_value(&details, ElementProperty::Width), "100");
        assert_eq!(get_property_value(&details, ElementProperty::Height), "50");
    }

    #[test]
    fn test_get_property_value_bool_fields() {
        let details = ElementDetails {
            is_visible: Some(true),
            is_interactive: Some(false),
            ..Default::default()
        };
        assert_eq!(
            get_property_value(&details, ElementProperty::IsVisible),
            "Ja"
        );
        assert_eq!(
            get_property_value(&details, ElementProperty::IsInteractive),
            "Nein"
        );
    }

    #[test]
    fn test_get_property_value_bool_none_is_empty() {
        let details = ElementDetails::default();
        assert_eq!(
            get_property_value(&details, ElementProperty::IsVisible),
            ""
        );
        assert_eq!(
            get_property_value(&details, ElementProperty::IsInteractive),
            ""
        );
    }

    #[test]
    fn test_get_property_value_empty_string_field() {
        let details = ElementDetails::default();
        assert_eq!(get_property_value(&details, ElementProperty::Role), "");
    }

    #[test]
    fn test_is_ocr_property_true() {
        assert!(is_ocr_property(ElementProperty::OcrTesseract));
        assert!(is_ocr_property(ElementProperty::OcrPaddleOcr));
        assert!(is_ocr_property(ElementProperty::OcrSurya));
    }

    #[test]
    fn test_is_ocr_property_false() {
        assert!(!is_ocr_property(ElementProperty::Tag));
        assert!(!is_ocr_property(ElementProperty::Width));
        assert!(!is_ocr_property(ElementProperty::Role));
    }
}
