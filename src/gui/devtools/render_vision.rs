//! Renders the KI Vision section with tactic selection, analysis trigger button,
//! and annotated screenshot or JSON/text result display.
//!
//! Image results use a texture cache (`Arc<Mutex<Option<TextureHandle>>>`) to avoid
//! re-decoding the PNG on every frame. The cache must be reset to `None` by the
//! caller whenever a new analysis is triggered (i.e. when image_state is set back
//! to `Loading`), so the next `Loaded` state causes a fresh decode.

use std::sync::{Arc, Mutex};

use egui::{Color32, RichText, ScrollArea, Vec2};
use uuid::Uuid;

use super::types::{
    DevToolsAction, ImageState, OcrConfig, OcrDisplayResult, PageInfo, SharedImage, SharedText,
    TextState, VisionTactic,
};

/// Parameters for `render_ki_vision`, bundled to stay below the 7-argument Clippy limit.
///
/// Groups all shared-state references required to render the KI-Vision section:
/// vision image/text/texture for screenshot display, OCR config/results/image/texture
/// for the OCR sub-section, and page info for the API port label.
pub(super) struct KiVisionParams<'a> {
    /// Currently selected vision tactic (mutated on user selection change).
    pub tactic: &'a mut VisionTactic,
    /// Shared text result state for non-image tactics (DOM snapshot, labels, etc.).
    pub vision_text: &'a SharedText,
    /// Shared image result state for annotated screenshot tactics.
    pub vision_image: &'a SharedImage,
    /// Cached egui texture handle for the vision annotated screenshot.
    pub vision_texture: &'a Arc<Mutex<Option<egui::TextureHandle>>>,
    /// Current page metadata (URL, title, API port) shown in the run button row.
    pub page_info: &'a PageInfo,
    /// Shared action queue — OCR and vision actions are pushed here.
    pub shared_actions: &'a Arc<Mutex<Vec<DevToolsAction>>>,
    /// OCR engine selection config (Tesseract, PaddleOCR, Surya toggles).
    pub shared_ocr_config: &'a Arc<Mutex<OcrConfig>>,
    /// OCR results from the last OCR run, displayed in the results table.
    pub shared_ocr_results: &'a Arc<Mutex<Vec<OcrDisplayResult>>>,
    /// Shared image state for the OCR-annotated screenshot.
    pub ocr_image: &'a SharedImage,
    /// Cached egui texture handle for the OCR-annotated screenshot.
    pub ocr_texture: &'a Arc<Mutex<Option<egui::TextureHandle>>>,
}

/// Renders the KI-Vision section with tactic selector grid, run button, and results.
///
/// Returns an optional `RunVisionTactic` action when the user clicks "Analyse starten".
/// OCR results are delegated to `render_ocr_section` from the `render_ocr` sub-module.
pub(super) fn render_ki_vision(
    ui: &mut egui::Ui,
    ctx: &egui::Context,
    params: &mut KiVisionParams<'_>,
) -> Option<DevToolsAction> {
    let KiVisionParams {
        tactic,
        vision_text,
        vision_image,
        vision_texture,
        page_info,
        shared_actions,
        shared_ocr_config,
        shared_ocr_results,
        ocr_image,
        ocr_texture,
    } = params;
    let mut action = None;

    // Header with description
    ui.label(
        RichText::new("KI-Vision Taktiken")
            .color(Color32::WHITE)
            .strong()
            .size(14.0),
    );
    ui.label(
        RichText::new("Zeigt was die KI bei verschiedenen Analyse-Methoden sieht")
            .color(Color32::from_rgb(140, 140, 160))
            .size(11.0),
    );
    ui.add_space(4.0);

    // Tactic selector grid (2 columns)
    egui::Grid::new("vision_tactic_grid")
        .num_columns(2)
        .spacing([6.0, 4.0])
        .show(ui, |ui| {
            for (i, t) in VisionTactic::all().iter().enumerate() {
                let is_selected = **tactic == *t;
                let bg = if is_selected {
                    Color32::from_rgb(45, 55, 75)
                } else {
                    Color32::from_rgb(32, 32, 40)
                };
                let text_color = if is_selected { t.color() } else { Color32::GRAY };

                egui::Frame::NONE
                    .fill(bg)
                    .corner_radius(4.0)
                    .inner_margin(6.0)
                    .show(ui, |ui| {
                        let resp = ui.selectable_label(
                            is_selected,
                            RichText::new(t.label()).color(text_color).size(11.0),
                        );
                        if resp.clicked() {
                            **tactic = *t;
                        }
                    });

                if i % 2 == 1 {
                    ui.end_row();
                }
            }
        });

    ui.add_space(4.0);

    // Description of selected tactic
    ui.horizontal(|ui| {
        ui.label(
            RichText::new(">>")
                .color(tactic.color())
                .strong(),
        );
        ui.label(
            RichText::new(tactic.description())
                .color(Color32::LIGHT_GRAY)
                .size(11.0),
        );
    });
    ui.add_space(4.0);

    // Run button (not shown for Ocr — it has its own section via render_ocr_section)
    let is_image_tactic = matches!(*tactic, VisionTactic::Annotated | VisionTactic::DomAnnotate);
    let is_ocr_tactic = **tactic == VisionTactic::Ocr;

    if !is_ocr_tactic {
        let is_loading = if is_image_tactic {
            vision_image.lock().ok()
                .map(|s| matches!(*s, ImageState::Loading))
                .unwrap_or(false)
        } else {
            vision_text.lock().ok()
                .map(|s| matches!(*s, TextState::Loading))
                .unwrap_or(false)
        };

        ui.horizontal(|ui| {
            let btn_text = if is_loading {
                "Analysiere..."
            } else {
                "Analyse starten"
            };
            let btn = egui::Button::new(
                RichText::new(btn_text).color(if is_loading { Color32::GRAY } else { Color32::WHITE }),
            );
            if ui.add_enabled(!is_loading, btn).clicked() {
                action = Some(DevToolsAction::RunVisionTactic {
                    tactic: match *tactic {
                        VisionTactic::Annotated => "annotated",
                        VisionTactic::Labels => "labels",
                        VisionTactic::DomSnapshot => "dom_snapshot",
                        VisionTactic::DomAnnotate => "dom_annotate",
                        VisionTactic::StructuredData => "structured_data",
                        VisionTactic::ContentExtract => "content_extract",
                        VisionTactic::StructureAnalysis => "structure_analysis",
                        VisionTactic::Forms => "forms",
                        VisionTactic::Ocr => "ocr",
                    },
                    tab_id: Uuid::nil(), // resolved to the active tab in browser_app
                });
            }

            ui.label(
                RichText::new(format!("Port :{}", page_info.api_port))
                    .color(Color32::from_rgb(80, 80, 100))
                    .monospace()
                    .size(10.0),
            );
        });
        ui.separator();
    }

    // Result display: OCR section, annotated image, or text/JSON
    if is_ocr_tactic {
        super::render_ocr::render_ocr_section(
            ui,
            ctx,
            &super::render_ocr::OcrSectionParams {
                actions: shared_actions,
                ocr_config: shared_ocr_config,
                ocr_results: shared_ocr_results,
                _page_info: page_info,
                ocr_image,
                ocr_texture,
            },
        );
    } else if is_image_tactic {
        render_vision_image(ui, ctx, vision_image, vision_texture, "vision_annotated");
    } else {
        render_vision_text(ui, vision_text);
    }

    action
}

/// Renders an annotated PNG screenshot result with aspect-ratio-preserving scaling.
///
/// Uses a texture cache (`texture`) to avoid re-decoding the PNG on every frame.
/// The cache is invalidated (reset to `None`) externally when `image_state` transitions
/// back to `Loading`, ensuring a fresh decode on the next `Loaded` state.
pub(super) fn render_vision_image(
    ui: &mut egui::Ui,
    ctx: &egui::Context,
    image_state: &SharedImage,
    texture: &Arc<Mutex<Option<egui::TextureHandle>>>,
    texture_key: &str,
) {
    let state = {
        let guard = image_state.lock().ok();
        match guard.as_deref() {
            Some(ImageState::Empty) => None,
            Some(ImageState::Loading) => Some(Err("Laden...".to_string())),
            Some(ImageState::Loaded(bytes)) => {
                // Check the texture cache first — only decode PNG when the cache is empty.
                let already_cached = texture.lock().ok()
                    .map(|t| t.is_some())
                    .unwrap_or(false);

                if already_cached {
                    // Cache hit: skip decoding, proceed directly to render.
                    Some(Ok(()))
                } else {
                    // Cache miss: decode PNG and populate the texture cache.
                    match image::load_from_memory(bytes) {
                        Ok(img) => {
                            let rgba = img.to_rgba8();
                            let size = [rgba.width() as usize, rgba.height() as usize];
                            let pixels = rgba.into_raw();
                            let color_image = egui::ColorImage::from_rgba_unmultiplied(size, &pixels);
                            let tex = ctx.load_texture(
                                texture_key,
                                color_image,
                                egui::TextureOptions::LINEAR,
                            );
                            if let Ok(mut t) = texture.lock() {
                                *t = Some(tex);
                            }
                            Some(Ok(()))
                        }
                        Err(e) => Some(Err(format!("Bild-Dekodierung fehlgeschlagen: {}", e))),
                    }
                }
            }
            Some(ImageState::Error(e)) => Some(Err(e.clone())),
            None => None,
        }
    };

    match state {
        None => {
            ui.centered_and_justified(|ui| {
                ui.label(
                    RichText::new("Klicke 'Analyse starten' um die KI-Vision zu testen")
                        .color(Color32::from_rgb(100, 100, 115)),
                );
            });
        }
        Some(Err(msg)) => {
            ui.label(RichText::new(&msg).color(Color32::YELLOW).italics());
        }
        Some(Ok(())) => {
            let tex_opt = texture.lock().ok().and_then(|t| t.clone());
            if let Some(tex) = tex_opt {
                ScrollArea::both()
                    .auto_shrink([false, false])
                    .show(ui, |ui| {
                        let tex_size = tex.size_vec2();
                        let available = ui.available_width();
                        let scale = (available / tex_size.x).min(1.0);
                        let display_size = Vec2::new(tex_size.x * scale, tex_size.y * scale);
                        ui.image(egui::load::SizedTexture::new(tex.id(), display_size));
                    });
            }
        }
    }
}

/// Renders a text or JSON result in a scrollable monospace code editor.
pub(super) fn render_vision_text(ui: &mut egui::Ui, text_state: &SharedText) {
    let content = {
        let guard = text_state.lock().ok();
        match guard.as_deref() {
            Some(TextState::Empty) => None,
            Some(TextState::Loading) => Some(("Laden...".to_string(), false)),
            Some(TextState::Loaded(s)) => Some((s.clone(), true)),
            Some(TextState::Error(e)) => Some((format!("Fehler: {}", e), false)),
            None => None,
        }
    };

    match content {
        None => {
            ui.centered_and_justified(|ui| {
                ui.label(
                    RichText::new("Klicke 'Analyse starten' um die KI-Vision zu testen")
                        .color(Color32::from_rgb(100, 100, 115)),
                );
            });
        }
        Some((text, is_data)) => {
            ScrollArea::both()
                .auto_shrink([false, false])
                .show(ui, |ui| {
                    if is_data {
                        ui.add(
                            egui::TextEdit::multiline(&mut text.as_str())
                                .code_editor()
                                .desired_width(f32::INFINITY)
                                .font(egui::TextStyle::Monospace),
                        );
                    } else {
                        ui.label(RichText::new(&text).color(Color32::GRAY).italics());
                    }
                });
        }
    }
}
