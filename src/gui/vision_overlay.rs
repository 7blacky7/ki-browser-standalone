//! Vision overlay rendering on top of the viewport.
//!
//! Draws bounding boxes, numbered labels, and element type indicators
//! directly on the webpage viewport so developers can see exactly what
//! the KI sees through each vision tactic.

use std::sync::{Arc, Mutex};
use egui::{Color32, Pos2, Rect, Stroke, StrokeKind, Vec2, CornerRadius};
use serde::Deserialize;

/// Which vision tactic is currently active as an overlay.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum VisionMode {
    Off,
    /// Numbered labels for all interactive elements (red boxes + ID badges).
    VisionLabels,
    /// Color-coded element types (links=blue, buttons=green, inputs=orange).
    DomAnnotate,
    /// DOM snapshot tree with bounding boxes for all visible nodes.
    DomSnapshot,
    /// Detected forms highlighted with field indicators.
    Forms,
}

impl VisionMode {
    pub fn label(&self) -> &'static str {
        match self {
            Self::Off => "Aus",
            Self::VisionLabels => "Vision Labels",
            Self::DomAnnotate => "DOM Annotate",
            Self::DomSnapshot => "DOM Snapshot",
            Self::Forms => "Formulare",
        }
    }

    pub fn description(&self) -> &'static str {
        match self {
            Self::Off => "Kein Overlay aktiv",
            Self::VisionLabels => "Nummerierte Labels fuer alle interaktiven Elemente",
            Self::DomAnnotate => "Farbige Markierungen nach Element-Typ",
            Self::DomSnapshot => "Bounding Boxes fuer alle sichtbaren DOM-Knoten",
            Self::Forms => "Erkannte Formulare und Felder hervorgehoben",
        }
    }

    pub fn all_active() -> &'static [VisionMode] {
        &[
            Self::VisionLabels,
            Self::DomAnnotate,
            Self::DomSnapshot,
            Self::Forms,
        ]
    }
}

/// A single overlay element to draw on the viewport.
#[derive(Clone, Debug)]
pub struct OverlayElement {
    pub id: u32,
    pub x: f32,
    pub y: f32,
    pub w: f32,
    pub h: f32,
    pub label: String,
    pub color: Color32,
    pub element_type: String,
    /// OCR text recognized by Tesseract engine (populated by background enrichment).
    pub ocr_tesseract: Option<String>,
    /// OCR text recognized by PaddleOCR engine (populated by background enrichment).
    pub ocr_paddleocr: Option<String>,
    /// OCR text recognized by Surya engine (populated by background enrichment).
    pub ocr_surya: Option<String>,
}

/// State for async overlay data loading.
pub enum OverlayState {
    Empty,
    Loading,
    Loaded(Vec<OverlayElement>),
    Error(String),
}

/// Shared overlay state for background thread loading.
pub type SharedOverlay = Arc<Mutex<OverlayState>>;

/// Persistent state for vision overlays.
pub struct VisionOverlayState {
    pub mode: VisionMode,
    pub overlay: SharedOverlay,
    /// Viewport dimensions used when the overlay was fetched (for coordinate scaling).
    pub source_viewport: (f32, f32),
}

impl Default for VisionOverlayState {
    fn default() -> Self {
        Self {
            mode: VisionMode::Off,
            overlay: Arc::new(Mutex::new(OverlayState::Empty)),
            source_viewport: (1280.0, 720.0),
        }
    }
}

impl VisionOverlayState {
    pub fn overlay_handle(&self) -> SharedOverlay {
        self.overlay.clone()
    }

    pub fn set_loading(&self) {
        if let Ok(mut s) = self.overlay.lock() {
            *s = OverlayState::Loading;
        }
    }

    pub fn clear(&self) {
        if let Ok(mut s) = self.overlay.lock() {
            *s = OverlayState::Empty;
        }
    }
}

/// Finds the overlay element at the given screen position.
/// Returns the smallest (most specific) element if multiple overlap.
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

        // Bounding box
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

// ---- OCR enrichment: match OCR results to overlay elements by bounding-box overlap ----

/// Computes the intersection area between two axis-aligned rectangles.
/// Each rectangle is defined as (x, y, w, h).
fn intersection_area(ax: f32, ay: f32, aw: f32, ah: f32, bx: f32, by: f32, bw: f32, bh: f32) -> f32 {
    let x_overlap = (ax + aw).min(bx + bw) - ax.max(bx);
    let y_overlap = (ay + ah).min(by + bh) - ay.max(by);
    if x_overlap > 0.0 && y_overlap > 0.0 {
        x_overlap * y_overlap
    } else {
        0.0
    }
}

/// Enriches overlay elements with OCR results by matching bounding-box overlap.
///
/// For each OCR result, finds the overlay element whose bounding box has the
/// largest intersection area, then sets the corresponding `ocr_*` field on
/// that element based on the engine name.
///
/// `ocr_results` is a slice of `(engine_name, results)` tuples, where
/// `engine_name` is one of "tesseract", "paddleocr", or "surya".
pub fn enrich_with_ocr(
    overlay: &SharedOverlay,
    ocr_results: &[(String, Vec<crate::ocr::OcrResult>)],
) {
    let mut guard = match overlay.lock() {
        Ok(g) => g,
        Err(_) => return,
    };
    let elements = match &mut *guard {
        OverlayState::Loaded(ref mut elems) => elems,
        _ => return,
    };

    for (engine_name, results) in ocr_results {
        for ocr in results {
            // Skip empty OCR results
            if ocr.text.trim().is_empty() {
                continue;
            }

            // Find the overlay element with the largest bounding-box overlap
            let mut best_idx: Option<usize> = None;
            let mut best_area: f32 = 0.0;

            for (i, elem) in elements.iter().enumerate() {
                let area = intersection_area(
                    elem.x, elem.y, elem.w, elem.h,
                    ocr.x, ocr.y, ocr.w, ocr.h,
                );
                if area > best_area {
                    best_area = area;
                    best_idx = Some(i);
                }
            }

            // Assign OCR text to the best-matching element
            if let Some(idx) = best_idx {
                let elem = &mut elements[idx];
                match engine_name.as_str() {
                    "tesseract" => elem.ocr_tesseract = Some(ocr.text.clone()),
                    "paddleocr" => elem.ocr_paddleocr = Some(ocr.text.clone()),
                    "surya" => elem.ocr_surya = Some(ocr.text.clone()),
                    _ => {} // Unknown engine, ignore
                }
            }
        }
    }
}

/// Triggers background OCR enrichment for the current overlay elements.
///
/// Spawns a background thread that checks which OCR engines are available
/// and logs their readiness. Once screenshot capture infrastructure is ready,
/// this function will run each available engine on the screenshot and call
/// [`enrich_with_ocr`] to attach the results to overlay elements.
pub fn trigger_ocr_enrichment(
    overlay: SharedOverlay,
    // screenshot: Vec<u8> -- will be provided when screenshot infrastructure is ready
) {
    std::thread::spawn(move || {
        let engines = crate::ocr::all_engines();
        let mut available_count = 0;
        for engine in &engines {
            if engine.is_available() {
                tracing::info!(
                    "OCR engine '{}' (v{}) available for overlay enrichment",
                    engine.name(),
                    engine.version().unwrap_or_else(|| "unknown".to_string()),
                );
                available_count += 1;
            }
        }

        if available_count == 0 {
            tracing::debug!("No OCR engines available for overlay enrichment");
            return;
        }

        tracing::info!(
            "{} OCR engine(s) ready for enrichment -- waiting for screenshot infrastructure",
            available_count,
        );

        // TODO: Once screenshot capture is available:
        // 1. Capture screenshot of the active tab as PNG bytes
        // 2. Run each available engine: engine.recognize(&screenshot_png, None)
        // 3. Collect results as Vec<(String, Vec<OcrResult>)>
        // 4. Call enrich_with_ocr(&overlay, &results) to attach text to elements
        let _ = overlay; // suppress unused warning until screenshot is wired up
    });
}

// ---- Parsing helpers: convert API JSON responses to OverlayElements ----

/// Parse `/vision/labels` JSON response into overlay elements.
pub fn parse_vision_labels(json: &str) -> Result<Vec<OverlayElement>, String> {
    #[derive(Deserialize)]
    struct Resp { data: LabelData }
    #[derive(Deserialize)]
    struct LabelData { labels: Vec<Label> }
    #[derive(Deserialize)]
    struct Label {
        id: u32,
        bbox: Bbox,
        role: String,
        name: String,
    }
    #[derive(Deserialize)]
    struct Bbox { x: f64, y: f64, width: f64, height: f64 }

    let resp: Resp = serde_json::from_str(json)
        .map_err(|e| format!("JSON parse: {}", e))?;

    Ok(resp.data.labels.into_iter().map(|l| {
        let color = role_color(&l.role);
        let label_text = format!("{}", l.id);
        OverlayElement {
            id: l.id,
            x: l.bbox.x as f32,
            y: l.bbox.y as f32,
            w: l.bbox.width as f32,
            h: l.bbox.height as f32,
            label: label_text,
            color,
            element_type: l.role,
            ocr_tesseract: None,
            ocr_paddleocr: None,
            ocr_surya: None,
        }
    }).collect())
}

/// Parse `/dom/annotate` JSON response into overlay elements.
pub fn parse_dom_annotate(json: &str) -> Result<Vec<OverlayElement>, String> {
    #[derive(Deserialize)]
    struct Resp { data: AnnotateData }
    #[derive(Deserialize)]
    struct AnnotateData { elements: Vec<AnnotateElem> }
    #[derive(Deserialize)]
    struct AnnotateElem {
        id: u32,
        #[serde(rename = "type")]
        elem_type: String,
        text: Option<String>,
        x: f64, y: f64, w: f64, h: f64,
    }

    let resp: Resp = serde_json::from_str(json)
        .map_err(|e| format!("JSON parse: {}", e))?;

    Ok(resp.data.elements.into_iter().map(|e| {
        let color = type_color(&e.elem_type);
        let label_text = e.text.as_deref()
            .unwrap_or(&e.elem_type)
            .chars().take(15).collect::<String>();
        OverlayElement {
            id: e.id,
            x: e.x as f32,
            y: e.y as f32,
            w: e.w as f32,
            h: e.h as f32,
            label: label_text,
            color,
            element_type: e.elem_type,
            ocr_tesseract: None,
            ocr_paddleocr: None,
            ocr_surya: None,
        }
    }).collect())
}

/// Parse `/dom/snapshot` JSON response into overlay elements.
pub fn parse_dom_snapshot(json: &str) -> Result<Vec<OverlayElement>, String> {
    #[derive(Deserialize)]
    struct Resp { data: SnapData }
    #[derive(Deserialize)]
    struct SnapData { nodes: Vec<SnapNode> }
    #[derive(Deserialize)]
    struct SnapNode {
        id: u32,
        tag: String,
        bbox: Option<Bbox>,
        is_visible: Option<bool>,
        is_interactive: Option<bool>,
    }
    #[derive(Deserialize)]
    struct Bbox { x: f64, y: f64, width: f64, height: f64 }

    let resp: Resp = serde_json::from_str(json)
        .map_err(|e| format!("JSON parse: {}", e))?;

    Ok(resp.data.nodes.into_iter().filter_map(|n| {
        let bbox = n.bbox?;
        if bbox.width < 1.0 || bbox.height < 1.0 { return None; }
        if n.is_visible == Some(false) { return None; }

        let color = if n.is_interactive == Some(true) {
            Color32::from_rgb(80, 200, 255)
        } else {
            Color32::from_rgb(100, 100, 140)
        };
        Some(OverlayElement {
            id: n.id,
            x: bbox.x as f32,
            y: bbox.y as f32,
            w: bbox.width as f32,
            h: bbox.height as f32,
            label: n.tag,
            color,
            element_type: if n.is_interactive == Some(true) { "interactive".into() } else { "node".into() },
            ocr_tesseract: None,
            ocr_paddleocr: None,
            ocr_surya: None,
        })
    }).collect())
}

/// Parse `/dom/forms` JSON response into overlay elements.
pub fn parse_forms(json: &str) -> Result<Vec<OverlayElement>, String> {
    #[derive(Deserialize)]
    struct Resp { data: FormsData }
    #[derive(Deserialize)]
    struct FormsData { forms: Vec<FormInfo> }
    #[derive(Deserialize)]
    struct FormInfo {
        fields: Vec<FormField>,
    }
    #[derive(Deserialize)]
    struct FormField {
        field_type: Option<String>,
        label: Option<String>,
        name: Option<String>,
        bbox: Option<Bbox>,
        is_visible: Option<bool>,
    }
    #[derive(Deserialize)]
    struct Bbox { x: f64, y: f64, width: f64, height: f64 }

    let resp: Resp = serde_json::from_str(json)
        .map_err(|e| format!("JSON parse: {}", e))?;

    let mut elements = Vec::new();
    let mut idx = 1u32;
    for form in resp.data.forms {
        for field in form.fields {
            if field.is_visible == Some(false) { continue; }
            let bbox = match field.bbox {
                Some(b) if b.width > 1.0 && b.height > 1.0 => b,
                _ => continue,
            };
            let ft = field.field_type.as_deref().unwrap_or("unknown");
            let color = match ft {
                "text" | "email" | "password" | "search" | "tel" | "url" => Color32::from_rgb(255, 165, 0),
                "submit" | "button" => Color32::from_rgb(0, 200, 0),
                "checkbox" | "radio" => Color32::from_rgb(200, 100, 255),
                "select" | "select-one" => Color32::from_rgb(100, 200, 255),
                "textarea" => Color32::from_rgb(255, 200, 100),
                _ => Color32::from_rgb(180, 180, 180),
            };
            let label = field.label
                .or(field.name)
                .unwrap_or_else(|| ft.to_string());
            elements.push(OverlayElement {
                id: idx,
                x: bbox.x as f32,
                y: bbox.y as f32,
                w: bbox.width as f32,
                h: bbox.height as f32,
                label: label.chars().take(12).collect(),
                color,
                element_type: ft.to_string(),
                ocr_tesseract: None,
                ocr_paddleocr: None,
                ocr_surya: None,
            });
            idx += 1;
        }
    }
    Ok(elements)
}

fn role_color(role: &str) -> Color32 {
    match role {
        "button" => Color32::from_rgb(0, 200, 0),
        "link" => Color32::from_rgb(0, 100, 255),
        "textbox" | "searchbox" => Color32::from_rgb(255, 165, 0),
        "checkbox" | "radio" | "switch" => Color32::from_rgb(200, 100, 255),
        "combobox" | "listbox" => Color32::from_rgb(100, 200, 255),
        "tab" | "menuitem" => Color32::from_rgb(255, 200, 100),
        "img" | "image" => Color32::from_rgb(200, 0, 200),
        _ => Color32::from_rgb(255, 80, 80),
    }
}

fn type_color(element_type: &str) -> Color32 {
    match element_type {
        "link" => Color32::from_rgb(0, 100, 255),
        "button" => Color32::from_rgb(0, 200, 0),
        "input" => Color32::from_rgb(255, 165, 0),
        "image" => Color32::from_rgb(200, 0, 200),
        "price" => Color32::from_rgb(255, 255, 0),
        _ => Color32::from_rgb(255, 0, 0),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ocr::OcrResult;

    /// Helper to create a test OverlayElement at the given position/size.
    fn make_elem(id: u32, x: f32, y: f32, w: f32, h: f32) -> OverlayElement {
        OverlayElement {
            id,
            x,
            y,
            w,
            h,
            label: format!("elem-{}", id),
            color: Color32::RED,
            element_type: "test".to_string(),
            ocr_tesseract: None,
            ocr_paddleocr: None,
            ocr_surya: None,
        }
    }

    #[test]
    fn test_overlay_element_ocr_fields_default_none() {
        let elem = make_elem(1, 0.0, 0.0, 100.0, 50.0);
        assert!(elem.ocr_tesseract.is_none());
        assert!(elem.ocr_paddleocr.is_none());
        assert!(elem.ocr_surya.is_none());
    }

    #[test]
    fn test_enrich_with_ocr_basic() {
        // Create two overlay elements side by side
        let elem1 = make_elem(1, 0.0, 0.0, 100.0, 50.0);
        let elem2 = make_elem(2, 150.0, 0.0, 100.0, 50.0);

        let overlay: SharedOverlay = Arc::new(Mutex::new(
            OverlayState::Loaded(vec![elem1, elem2]),
        ));

        // OCR result overlapping with elem1 (at position 10,10 size 80x30)
        let ocr_results = vec![
            ("tesseract".to_string(), vec![
                OcrResult {
                    text: "Hello World".to_string(),
                    confidence: 0.95,
                    x: 10.0,
                    y: 10.0,
                    w: 80.0,
                    h: 30.0,
                },
            ]),
            ("paddleocr".to_string(), vec![
                OcrResult {
                    text: "Button Text".to_string(),
                    confidence: 0.88,
                    x: 160.0,
                    y: 5.0,
                    w: 70.0,
                    h: 20.0,
                },
            ]),
        ];

        enrich_with_ocr(&overlay, &ocr_results);

        // Verify: elem1 should have tesseract text, elem2 should have paddleocr text
        let guard = overlay.lock().unwrap();
        if let OverlayState::Loaded(ref elems) = *guard {
            assert_eq!(elems[0].ocr_tesseract, Some("Hello World".to_string()));
            assert!(elems[0].ocr_paddleocr.is_none());
            assert!(elems[0].ocr_surya.is_none());

            assert!(elems[1].ocr_tesseract.is_none());
            assert_eq!(elems[1].ocr_paddleocr, Some("Button Text".to_string()));
            assert!(elems[1].ocr_surya.is_none());
        } else {
            panic!("Expected OverlayState::Loaded");
        }
    }

    #[test]
    fn test_enrich_with_ocr_empty() {
        let elem = make_elem(1, 0.0, 0.0, 100.0, 50.0);
        let overlay: SharedOverlay = Arc::new(Mutex::new(
            OverlayState::Loaded(vec![elem]),
        ));

        // Empty OCR results should not modify anything
        let ocr_results: Vec<(String, Vec<OcrResult>)> = vec![];
        enrich_with_ocr(&overlay, &ocr_results);

        let guard = overlay.lock().unwrap();
        if let OverlayState::Loaded(ref elems) = *guard {
            assert!(elems[0].ocr_tesseract.is_none());
            assert!(elems[0].ocr_paddleocr.is_none());
            assert!(elems[0].ocr_surya.is_none());
        } else {
            panic!("Expected OverlayState::Loaded");
        }
    }

    #[test]
    fn test_enrich_with_ocr_skips_empty_text() {
        let elem = make_elem(1, 0.0, 0.0, 100.0, 50.0);
        let overlay: SharedOverlay = Arc::new(Mutex::new(
            OverlayState::Loaded(vec![elem]),
        ));

        // OCR result with empty/whitespace text should be skipped
        let ocr_results = vec![
            ("tesseract".to_string(), vec![
                OcrResult {
                    text: "   ".to_string(),
                    confidence: 0.5,
                    x: 10.0,
                    y: 10.0,
                    w: 80.0,
                    h: 30.0,
                },
            ]),
        ];
        enrich_with_ocr(&overlay, &ocr_results);

        let guard = overlay.lock().unwrap();
        if let OverlayState::Loaded(ref elems) = *guard {
            assert!(elems[0].ocr_tesseract.is_none());
        } else {
            panic!("Expected OverlayState::Loaded");
        }
    }

    #[test]
    fn test_enrich_with_ocr_no_overlap() {
        let elem = make_elem(1, 0.0, 0.0, 50.0, 50.0);
        let overlay: SharedOverlay = Arc::new(Mutex::new(
            OverlayState::Loaded(vec![elem]),
        ));

        // OCR result far away from the element -- no overlap
        let ocr_results = vec![
            ("surya".to_string(), vec![
                OcrResult {
                    text: "Distant Text".to_string(),
                    confidence: 0.9,
                    x: 500.0,
                    y: 500.0,
                    w: 100.0,
                    h: 30.0,
                },
            ]),
        ];
        enrich_with_ocr(&overlay, &ocr_results);

        let guard = overlay.lock().unwrap();
        if let OverlayState::Loaded(ref elems) = *guard {
            assert!(elems[0].ocr_surya.is_none());
        } else {
            panic!("Expected OverlayState::Loaded");
        }
    }

    #[test]
    fn test_enrich_with_ocr_best_overlap_wins() {
        // Two elements: elem1 partially overlaps, elem2 fully overlaps with OCR bbox
        let elem1 = make_elem(1, 0.0, 0.0, 60.0, 50.0);    // overlaps OCR by small area
        let elem2 = make_elem(2, 40.0, 0.0, 100.0, 50.0);   // overlaps OCR by large area
        let overlay: SharedOverlay = Arc::new(Mutex::new(
            OverlayState::Loaded(vec![elem1, elem2]),
        ));

        // OCR result centered on elem2 (50..130 x 0..30)
        let ocr_results = vec![
            ("tesseract".to_string(), vec![
                OcrResult {
                    text: "Overlapping".to_string(),
                    confidence: 0.9,
                    x: 50.0,
                    y: 0.0,
                    w: 80.0,
                    h: 30.0,
                },
            ]),
        ];
        enrich_with_ocr(&overlay, &ocr_results);

        let guard = overlay.lock().unwrap();
        if let OverlayState::Loaded(ref elems) = *guard {
            // elem1 overlaps by 10*30=300, elem2 overlaps by 80*30=2400
            // So elem2 should get the OCR text
            assert!(elems[0].ocr_tesseract.is_none());
            assert_eq!(elems[1].ocr_tesseract, Some("Overlapping".to_string()));
        } else {
            panic!("Expected OverlayState::Loaded");
        }
    }

    #[test]
    fn test_intersection_area_no_overlap() {
        assert_eq!(intersection_area(0.0, 0.0, 10.0, 10.0, 20.0, 20.0, 10.0, 10.0), 0.0);
    }

    #[test]
    fn test_intersection_area_partial() {
        // Rect A: 0..10 x 0..10, Rect B: 5..15 x 5..15 => overlap 5x5 = 25
        assert_eq!(intersection_area(0.0, 0.0, 10.0, 10.0, 5.0, 5.0, 10.0, 10.0), 25.0);
    }

    #[test]
    fn test_intersection_area_full_containment() {
        // Rect A: 0..100 x 0..100, Rect B: 10..30 x 10..30 => overlap 20x20 = 400
        assert_eq!(intersection_area(0.0, 0.0, 100.0, 100.0, 10.0, 10.0, 20.0, 20.0), 400.0);
    }
}
