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
    }
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
