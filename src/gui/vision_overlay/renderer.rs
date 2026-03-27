//! Renders colored bounding-box overlays on the viewport for detected elements.
//!
//! Draws bounding boxes, numbered label badges, and OCR text indicators
//! directly on the egui viewport rect so developers can see exactly what
//! the KI sees through each vision tactic (VisionLabels, DomAnnotate, etc.).
//! Also provides hit-testing for right-click element inspection.

use egui::{Color32, Pos2, Rect, Stroke, StrokeKind, Vec2, CornerRadius};
use super::types::{OverlayElement, OverlayState, VisionMode, VisionOverlayState};

/// Finds the overlay element at the given screen position.
///
/// Scales element bounding boxes from webpage coordinates to screen coordinates
/// using `scale` and `viewport_rect`, then returns the smallest (most specific)
/// hit element when multiple bounding boxes overlap the given point.
/// Returns `None` if the overlay is off or no element is hit.
pub fn hit_test(
    state: &VisionOverlayState,
    screen_pos: egui::Pos2,
    viewport_rect: Rect,
    scale: f32,
) -> Option<OverlayElement> {
    if state.mode == VisionMode::Off {
        return None;
    }
    let elements = {
        let guard = state.overlay.lock().ok();
        match guard.as_deref() {
            Some(OverlayState::Loaded(elems)) => elems.clone(),
            _ => return None,
        }
    };

    let mut hits: Vec<&OverlayElement> = elements.iter().filter(|elem| {
        let screen_rect = Rect::from_min_size(
            Pos2::new(
                viewport_rect.min.x + elem.x * scale,
                viewport_rect.min.y + elem.y * scale,
            ),
            Vec2::new(elem.w * scale, elem.h * scale),
        );
        screen_rect.contains(screen_pos)
    }).collect();

    // Return the smallest (most specific) element
    hits.sort_by(|a, b| {
        let area_a = a.w * a.h;
        let area_b = b.w * b.h;
        area_a.partial_cmp(&area_b).unwrap_or(std::cmp::Ordering::Equal)
    });
    hits.first().map(|e| (*e).clone())
}

/// Draws the vision overlay on top of the viewport.
///
/// `viewport_rect` is the egui screen rect where the webpage texture is drawn.
/// `scale` is the ratio between webpage pixels and screen pixels.
///
/// Renders a mode indicator badge in the top-right corner, then for each loaded
/// `OverlayElement`: a colored bounding-box stroke, a semi-transparent fill, a
/// label badge, and an OCR text badge when OCR enrichment data is present.
pub fn render_overlay(
    ui: &mut egui::Ui,
    state: &VisionOverlayState,
    viewport_rect: Rect,
    scale: f32,
) {
    if state.mode == VisionMode::Off {
        return;
    }

    let elements = {
        let guard = state.overlay.lock().ok();
        match guard.as_deref() {
            Some(OverlayState::Loaded(elems)) => elems.clone(),
            Some(OverlayState::Loading) => {
                // Show loading indicator in top-left of viewport
                let loading_pos = Pos2::new(viewport_rect.min.x + 8.0, viewport_rect.min.y + 8.0);
                ui.painter().rect_filled(
                    Rect::from_min_size(loading_pos, Vec2::new(120.0, 24.0)),
                    CornerRadius::same(4),
                    Color32::from_rgba_unmultiplied(0, 0, 0, 200),
                );
                ui.painter().text(
                    Pos2::new(loading_pos.x + 8.0, loading_pos.y + 12.0),
                    egui::Align2::LEFT_CENTER,
                    "Analysiere...",
                    egui::FontId::proportional(12.0),
                    Color32::YELLOW,
                );
                return;
            }
            Some(OverlayState::Error(e)) => {
                let err_pos = Pos2::new(viewport_rect.min.x + 8.0, viewport_rect.min.y + 8.0);
                let text = format!("Fehler: {}", e);
                ui.painter().rect_filled(
                    Rect::from_min_size(err_pos, Vec2::new(300.0, 24.0)),
                    CornerRadius::same(4),
                    Color32::from_rgba_unmultiplied(180, 0, 0, 200),
                );
                ui.painter().text(
                    Pos2::new(err_pos.x + 8.0, err_pos.y + 12.0),
                    egui::Align2::LEFT_CENTER,
                    &text,
                    egui::FontId::proportional(11.0),
                    Color32::WHITE,
                );
                return;
            }
            _ => return,
        }
    };

    // Mode indicator badge in top-right corner
    let mode_text = format!("{} ({} Elemente)", state.mode.label(), elements.len());
    let badge_pos = Pos2::new(viewport_rect.max.x - 8.0, viewport_rect.min.y + 8.0);
    let badge_size = Vec2::new(mode_text.len() as f32 * 7.0 + 16.0, 22.0);
    let badge_rect = Rect::from_min_size(
        Pos2::new(badge_pos.x - badge_size.x, badge_pos.y),
        badge_size,
    );
    ui.painter().rect_filled(
        badge_rect,
        CornerRadius::same(4),
        Color32::from_rgba_unmultiplied(0, 0, 0, 200),
    );
    ui.painter().text(
        badge_rect.center(),
        egui::Align2::CENTER_CENTER,
        &mode_text,
        egui::FontId::proportional(11.0),
        Color32::from_rgb(100, 220, 100),
    );

    // Draw each element overlay
    for elem in &elements {
        let screen_rect = Rect::from_min_size(
            Pos2::new(
                viewport_rect.min.x + elem.x * scale,
                viewport_rect.min.y + elem.y * scale,
            ),
            Vec2::new(elem.w * scale, elem.h * scale),
        );

        // Skip elements outside viewport
        if !viewport_rect.intersects(screen_rect) {
            continue;
        }

        // Clip to viewport
        let clipped = screen_rect.intersect(viewport_rect);

        // Bounding box stroke
        let stroke_color = Color32::from_rgba_unmultiplied(
            elem.color.r(), elem.color.g(), elem.color.b(), 180,
        );
        ui.painter().rect_stroke(
            clipped,
            CornerRadius::same(1),
            Stroke::new(1.5, stroke_color),
            StrokeKind::Outside,
        );

        // Semi-transparent fill
        let fill = Color32::from_rgba_unmultiplied(
            elem.color.r(), elem.color.g(), elem.color.b(), 25,
        );
        ui.painter().rect_filled(clipped, CornerRadius::same(1), fill);

        // Label badge (top-left corner of element)
        let badge_text = &elem.label;
        let text_width = badge_text.len() as f32 * 7.0 + 6.0;
        let badge_h = 14.0_f32;
        let badge_rect = Rect::from_min_size(
            Pos2::new(clipped.min.x, clipped.min.y - badge_h),
            Vec2::new(text_width.max(20.0), badge_h),
        );

        // Keep badge inside viewport
        let badge_rect = if badge_rect.min.y < viewport_rect.min.y {
            Rect::from_min_size(
                Pos2::new(clipped.min.x, clipped.min.y),
                badge_rect.size(),
            )
        } else {
            badge_rect
        };

        ui.painter().rect_filled(
            badge_rect,
            CornerRadius::same(2),
            Color32::from_rgba_unmultiplied(
                elem.color.r(), elem.color.g(), elem.color.b(), 220,
            ),
        );
        ui.painter().text(
            badge_rect.center(),
            egui::Align2::CENTER_CENTER,
            badge_text,
            egui::FontId::monospace(10.0),
            Color32::WHITE,
        );

        // OCR badge: show a small yellow "OCR" indicator below the label badge
        // when any OCR engine has recognized text for this element.
        let has_ocr = elem.ocr_tesseract.is_some()
            || elem.ocr_paddleocr.is_some()
            || elem.ocr_surya.is_some();
        if has_ocr {
            // Collect the first available OCR text (truncated) for display
            let ocr_text = elem.ocr_tesseract.as_deref()
                .or(elem.ocr_paddleocr.as_deref())
                .or(elem.ocr_surya.as_deref())
                .unwrap_or("OCR");
            let ocr_display: String = ocr_text.chars().take(20).collect();
            let ocr_label = format!("OCR: {}", ocr_display);
            let ocr_text_width = ocr_label.len() as f32 * 6.0 + 6.0;
            let ocr_badge_h = 13.0_f32;
            let ocr_badge_rect = Rect::from_min_size(
                Pos2::new(badge_rect.min.x, badge_rect.max.y + 1.0),
                Vec2::new(ocr_text_width.max(30.0), ocr_badge_h),
            );

            ui.painter().rect_filled(
                ocr_badge_rect,
                CornerRadius::same(2),
                Color32::from_rgba_unmultiplied(180, 160, 0, 220),
            );
            ui.painter().text(
                ocr_badge_rect.center(),
                egui::Align2::CENTER_CENTER,
                &ocr_label,
                egui::FontId::monospace(9.0),
                Color32::BLACK,
            );
        }
    }
}
