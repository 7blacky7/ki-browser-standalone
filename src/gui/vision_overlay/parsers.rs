//! JSON parsers for different vision tactic responses (labels, DOM annotate, DOM snapshot, forms).
//!
//! Each parser converts a raw JSON string from the browser API into a Vec of
//! `OverlayElement` values ready to be rendered on the viewport. Color assignment
//! for element roles/types is handled by the private `role_color` and `type_color`
//! helpers, which are local to this module since they are only used during parsing.

use egui::Color32;
use serde::Deserialize;
use super::types::OverlayElement;

/// Parse `/vision/labels` JSON response into overlay elements.
///
/// Maps each label entry to a colored `OverlayElement` using `role_color` to
/// assign a tactic-specific color based on the ARIA role field.
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
        // `name` field is present in the JSON response but not used after
        // deserialization; serde ignores unknown fields automatically, so
        // omitting it here eliminates the dead_code warning.
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
///
/// Maps each annotated DOM element to a colored `OverlayElement` using
/// `type_color` for color assignment and the element's text content (truncated
/// to 15 chars) as the label badge text.
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
///
/// Filters out invisible nodes (is_visible=false) and zero-size bounding boxes.
/// Interactive nodes are rendered in bright blue; non-interactive in muted grey-blue.
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
///
/// Iterates all detected forms and their fields, skipping invisible fields and
/// those with no bounding box. Each field type (text, button, checkbox, etc.)
/// gets a distinct color for quick visual identification.
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

/// Maps an ARIA role string to a display color for vision-labels overlays.
///
/// Used by `parse_vision_labels` to color-code each detected interactive element
/// by its semantic role (button, link, textbox, etc.).
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

/// Maps a DOM element type string to a display color for dom-annotate overlays.
///
/// Used by `parse_dom_annotate` to assign a tactic-specific color based on the
/// element's structural type (link, button, input, image, price).
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
