//! Renders the OCR section with engine selection checkboxes, run button,
//! annotated screenshot overlay, and a per-region confidence table.
//!
//! The annotated image (bounding boxes drawn on the screenshot PNG) is rendered
//! via `render_vision_image` from the `render_vision` sub-module.

use std::sync::{Arc, Mutex};

use egui::{Color32, RichText, ScrollArea};
use uuid::Uuid;

use super::render_vision::render_vision_image;
use super::types::{DevToolsAction, OcrConfig, OcrDisplayResult, PageInfo, SharedImage};

/// Renders the OCR section: engine checkboxes, run button, annotated image, and results table.
///
/// Actions (RunOcr) are pushed directly into `actions` rather than returned,
/// because this function is called from within `render_ki_vision` which already
/// has its own optional return action.
pub(super) fn render_ocr_section(
    ui: &mut egui::Ui,
    ctx: &egui::Context,
    actions: &Arc<Mutex<Vec<DevToolsAction>>>,
    ocr_config: &Arc<Mutex<OcrConfig>>,
    ocr_results: &Arc<Mutex<Vec<OcrDisplayResult>>>,
    _page_info: &PageInfo,
    ocr_image: &SharedImage,
    ocr_texture: &Arc<Mutex<Option<egui::TextureHandle>>>,
) {
    ui.label(RichText::new("OCR Engines").color(Color32::WHITE).strong());
    ui.add_space(4.0);

    // Engine checkboxes — read-modify-write the config atomically.
    let mut config = ocr_config.lock().ok().map(|c| c.clone()).unwrap_or_default();
    ui.horizontal(|ui| {
        ui.checkbox(&mut config.tesseract, RichText::new("Tesseract").color(Color32::from_rgb(100, 200, 255)));
        ui.checkbox(&mut config.paddleocr, RichText::new("PaddleOCR").color(Color32::from_rgb(100, 255, 100)));
        ui.checkbox(&mut config.surya, RichText::new("Surya").color(Color32::from_rgb(255, 200, 100)));
    });
    if let Ok(mut c) = ocr_config.lock() { *c = config.clone(); }

    ui.add_space(4.0);

    // Run OCR button — only enabled when at least one engine is selected.
    let any_engine = config.tesseract || config.paddleocr || config.surya;
    let results = ocr_results.lock().ok().map(|r| r.clone()).unwrap_or_default();

    if ui.add_enabled(any_engine, egui::Button::new(
        RichText::new("OCR starten").color(if any_engine { Color32::WHITE } else { Color32::GRAY })
    )).clicked() {
        let mut engines = Vec::new();
        if config.tesseract { engines.push("tesseract".to_string()); }
        if config.paddleocr { engines.push("paddleocr".to_string()); }
        if config.surya { engines.push("surya".to_string()); }
        if let Ok(mut a) = actions.lock() {
            a.push(DevToolsAction::RunOcr {
                engines,
                tab_id: Uuid::nil(), // resolved to the active tab in browser_app
            });
        }
    }
    ui.separator();

    // Display results or placeholder text.
    if results.is_empty() {
        ui.label(RichText::new("Keine OCR-Ergebnisse. Starte OCR mit den ausgewaehlten Engines.")
            .color(Color32::from_rgb(100, 100, 115)).italics());
    } else {
        ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| {
            // Render OCR annotated screenshot (bounding boxes drawn on PNG) if available.
            render_vision_image(ui, ctx, ocr_image, ocr_texture, "ocr_annotated");

            for result in &results {
                ui.group(|ui| {
                    ui.horizontal(|ui| {
                        ui.label(RichText::new(&result.engine).color(Color32::WHITE).strong());
                        ui.label(RichText::new(format!("{}ms", result.duration_ms))
                            .color(Color32::GRAY).size(11.0));
                        ui.label(RichText::new(format!("{} Regionen", result.result_count))
                            .color(Color32::GRAY).size(11.0));
                    });
                    if let Some(ref err) = result.error {
                        ui.label(RichText::new(format!("Fehler: {}", err)).color(Color32::RED));
                    } else {
                        ui.add(
                            egui::TextEdit::multiline(&mut result.full_text.as_str())
                                .code_editor()
                                .desired_width(f32::INFINITY)
                                .desired_rows(4)
                                .font(egui::TextStyle::Monospace),
                        );

                        // Show per-region bounding boxes in a striped table below the full text.
                        if !result.regions.is_empty() {
                            ui.add_space(4.0);
                            ui.label(
                                RichText::new(format!("{} Regionen:", result.regions.len()))
                                    .color(Color32::from_rgb(200, 200, 220))
                                    .size(11.0),
                            );
                            egui::Grid::new(format!("ocr_regions_{}", result.engine))
                                .striped(true)
                                .min_col_width(40.0)
                                .show(ui, |ui| {
                                    // Header row
                                    ui.label(RichText::new("#").color(Color32::GRAY).size(10.0));
                                    ui.label(RichText::new("Text").color(Color32::GRAY).size(10.0));
                                    ui.label(RichText::new("Conf").color(Color32::GRAY).size(10.0));
                                    ui.label(RichText::new("Position").color(Color32::GRAY).size(10.0));
                                    ui.end_row();

                                    for (i, region) in result.regions.iter().enumerate() {
                                        ui.label(
                                            RichText::new(format!("{}", i + 1))
                                                .color(Color32::from_rgb(255, 180, 100))
                                                .size(11.0),
                                        );
                                        // Truncate long text to keep table readable (Unicode-safe).
                                        let text_preview = if region.text.chars().count() > 40 {
                                            let truncated: String = region.text.chars().take(40).collect();
                                            format!("{}...", truncated)
                                        } else {
                                            region.text.clone()
                                        };
                                        ui.label(
                                            RichText::new(text_preview)
                                                .color(Color32::WHITE)
                                                .size(11.0),
                                        );
                                        // Color-code confidence: green > 80%, yellow > 50%, red otherwise.
                                        let conf_color = if region.confidence > 0.8 {
                                            Color32::from_rgb(100, 255, 100)
                                        } else if region.confidence > 0.5 {
                                            Color32::YELLOW
                                        } else {
                                            Color32::RED
                                        };
                                        ui.label(
                                            RichText::new(format!("{:.0}%", region.confidence * 100.0))
                                                .color(conf_color)
                                                .size(11.0),
                                        );
                                        ui.label(
                                            RichText::new(format!(
                                                "{:.0},{:.0} {:.0}x{:.0}",
                                                region.x, region.y, region.w, region.h
                                            ))
                                            .color(Color32::GRAY)
                                            .size(10.0)
                                            .monospace(),
                                        );
                                        ui.end_row();
                                    }
                                });
                        }
                    }
                });
                ui.add_space(4.0);
            }
        });
    }
}
