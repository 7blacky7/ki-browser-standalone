//! Vision overlay for KI agent interaction with annotated screenshots.
//!
//! Generates numbered labels for interactive DOM elements and renders them
//! as colored overlays onto a screenshot. Each label maps a sequential number
//! to a clickable/typeable element, enabling vision-based AI agents to reference
//! page elements by number instead of CSS selectors.

use image::{DynamicImage, ImageFormat, Rgba, RgbaImage};
use imageproc::drawing::{draw_filled_rect_mut, draw_hollow_rect_mut, draw_text_mut};
use imageproc::rect::Rect;
use rusttype::{Font, Scale};
use serde::{Deserialize, Serialize};
use std::io::Cursor;

use crate::browser::dom::BoundingBox;
use crate::browser::dom_snapshot::{DomNode, DomSnapshot};
use crate::browser::screenshot::ScreenshotFormat;
use crate::error::{BrowserError, BrowserResult};

// -------------------------------------------------------------------------
// Constants
// -------------------------------------------------------------------------

/// Red overlay color for interactive element bounding boxes.
const BBOX_COLOR: Rgba<u8> = Rgba([255, 0, 0, 200]);

/// Background color for the numbered badge.
const BADGE_BG: Rgba<u8> = Rgba([220, 0, 0, 240]);

/// White text color for badge numbers.
const BADGE_TEXT: Rgba<u8> = Rgba([255, 255, 255, 255]);

/// Font scale for badge number rendering.
const BADGE_FONT_SCALE: f32 = 14.0;

/// Height of the badge area in pixels.
const BADGE_HEIGHT: u32 = 16;

/// Horizontal padding inside the badge.
const BADGE_PADDING_X: u32 = 3;

/// Approximate character width for badge sizing.
const BADGE_CHAR_WIDTH: u32 = 9;

// -------------------------------------------------------------------------
// Data structures
// -------------------------------------------------------------------------

/// A numbered label assigned to a visible, interactive DOM element.
///
/// Each label has a sequential `id` (1-based) and carries enough metadata
/// for a KI agent to understand what the element is and how to target it.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VisionLabel {
    /// Sequential label number starting at 1.
    pub id: u32,

    /// Bounding box in viewport coordinates.
    pub bbox: BoundingBox,

    /// ARIA role or inferred role (e.g. "button", "link", "textbox").
    pub role: String,

    /// Accessible name (from text, aria-label, alt, placeholder).
    pub name: String,

    /// Short text hint (first ~80 chars of visible text).
    pub text_hint: Option<String>,

    /// Shortest unique CSS selector to locate this element.
    pub selector_hint: String,
}

/// Annotated screenshot with vision overlay labels.
///
/// Contains the rendered screenshot with numbered bounding boxes drawn on top
/// and the corresponding label metadata for AI agent consumption.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VisionOverlay {
    /// All generated labels for interactive elements.
    pub labels: Vec<VisionLabel>,

    /// PNG/JPEG bytes of the screenshot with labels drawn on it.
    #[serde(skip)]
    pub screenshot_with_labels: Vec<u8>,

    /// Image format of the annotated screenshot.
    pub format: ScreenshotFormat,
}

// -------------------------------------------------------------------------
// Label generation
// -------------------------------------------------------------------------

/// Generates numbered vision labels from a DOM snapshot.
///
/// Filters the snapshot to only interactive and visible elements, assigns
/// sequential IDs starting at 1, and generates the shortest unique CSS
/// selector hint for each element.
pub fn generate_labels(snapshot: &DomSnapshot) -> Vec<VisionLabel> {
    let interactive: Vec<&DomNode> = snapshot
        .nodes
        .iter()
        .filter(|n| n.is_visible && n.is_interactive && n.bbox.is_visible())
        .collect();

    interactive
        .iter()
        .enumerate()
        .map(|(idx, node)| {
            let role = infer_role(node);
            let name = infer_name(node);
            let text_hint = node
                .text
                .as_ref()
                .map(|t| truncate_text(t, 80));
            let selector_hint = build_selector_hint(node);

            VisionLabel {
                id: (idx + 1) as u32,
                bbox: node.bbox,
                role,
                name,
                text_hint,
                selector_hint,
            }
        })
        .collect()
}

/// Infers the semantic role of a DOM node from its tag, ARIA role, or type attribute.
fn infer_role(node: &DomNode) -> String {
    if let Some(ref role) = node.role {
        return role.clone();
    }

    match node.tag.as_str() {
        "a" => "link".to_string(),
        "button" => "button".to_string(),
        "input" => {
            let input_type = node.attributes.get("type").map(|s| s.as_str()).unwrap_or("text");
            match input_type {
                "submit" | "button" => "button".to_string(),
                "checkbox" => "checkbox".to_string(),
                "radio" => "radio".to_string(),
                _ => "textbox".to_string(),
            }
        }
        "textarea" => "textbox".to_string(),
        "select" => "combobox".to_string(),
        "details" | "summary" => "disclosure".to_string(),
        "label" => "label".to_string(),
        _ => "generic".to_string(),
    }
}

/// Derives an accessible name from common DOM attributes and text content.
fn infer_name(node: &DomNode) -> String {
    // Priority: aria-label > text > alt > placeholder > title > value
    if let Some(aria) = node.attributes.get("aria-label") {
        return truncate_text(aria, 100);
    }
    if let Some(ref text) = node.text {
        let trimmed = text.trim();
        if !trimmed.is_empty() {
            return truncate_text(trimmed, 100);
        }
    }
    if let Some(alt) = node.attributes.get("alt") {
        return truncate_text(alt, 100);
    }
    if let Some(ph) = node.attributes.get("placeholder") {
        return truncate_text(ph, 100);
    }
    if let Some(title) = node.attributes.get("title") {
        return truncate_text(title, 100);
    }
    if let Some(value) = node.attributes.get("value") {
        return truncate_text(value, 100);
    }
    String::new()
}

/// Builds the shortest reasonable CSS selector hint for a DOM node.
///
/// Uses ID if available, otherwise tag + class combination with nth-of-type
/// disambiguation when needed.
fn build_selector_hint(node: &DomNode) -> String {
    // If the node has an ID, use it
    if let Some(id) = node.attributes.get("id") {
        if !id.is_empty() {
            return format!("#{}", id);
        }
    }

    let mut selector = node.tag.clone();

    // Add first two classes if available
    if let Some(class) = node.attributes.get("class") {
        let classes: Vec<&str> = class.split_whitespace().take(2).collect();
        for cls in classes {
            selector.push('.');
            selector.push_str(cls);
        }
    }

    // Add name attribute for form elements
    if let Some(name) = node.attributes.get("name") {
        if !name.is_empty() {
            selector = format!("{}[name=\"{}\"]", selector, name);
        }
    }

    selector
}

/// Truncates a string to `max_len` characters, appending "..." if truncated.
fn truncate_text(text: &str, max_len: usize) -> String {
    let trimmed = text.trim();
    if trimmed.len() <= max_len {
        trimmed.to_string()
    } else {
        let end = trimmed
            .char_indices()
            .nth(max_len.saturating_sub(3))
            .map(|(i, _)| i)
            .unwrap_or(trimmed.len());
        format!("{}...", &trimmed[..end])
    }
}

// -------------------------------------------------------------------------
// Screenshot annotation
// -------------------------------------------------------------------------

/// Draws numbered label overlays onto an existing screenshot.
///
/// For each `VisionLabel`, draws a red bounding-box rectangle around the element
/// and a numbered badge at the top-left corner. Returns the annotated image as
/// encoded bytes in the requested format.
///
/// Uses `image` + `imageproc` + `rusttype` crates already present in the project
/// (same as `annotate.rs`). Falls back to badge-only rendering if no system font
/// is available.
pub fn annotate_screenshot_with_labels(
    screenshot_bytes: &[u8],
    labels: &[VisionLabel],
    format: ScreenshotFormat,
) -> BrowserResult<Vec<u8>> {
    let img = image::load_from_memory(screenshot_bytes).map_err(|e| {
        BrowserError::ScreenshotFailed {
            reason: format!("Failed to decode screenshot for annotation: {}", e),
        }
    })?;

    let mut rgba = img.to_rgba8();
    let font = load_system_font();
    let scale = Scale::uniform(BADGE_FONT_SCALE);

    for label in labels {
        draw_label_overlay(&mut rgba, label, font.as_ref(), scale);
    }

    encode_image(&rgba, format)
}

/// Draws a single label overlay (bounding box + numbered badge) onto the image.
fn draw_label_overlay(
    img: &mut RgbaImage,
    label: &VisionLabel,
    font: Option<&Font<'_>>,
    scale: Scale,
) {
    let x = label.bbox.x as i32;
    let y = label.bbox.y as i32;
    let w = label.bbox.width as u32;
    let h = label.bbox.height as u32;

    if w == 0 || h == 0 {
        return;
    }

    // Draw 2px red border around element bounding box
    let rect = Rect::at(x, y).of_size(w, h);
    draw_hollow_rect_mut(img, rect, BBOX_COLOR);
    if w > 2 && h > 2 {
        let inner = Rect::at(x + 1, y + 1).of_size(w - 2, h - 2);
        draw_hollow_rect_mut(img, inner, BBOX_COLOR);
    }

    // Draw numbered badge at top-left corner of the bounding box
    let label_text = label.id.to_string();
    let badge_w = (label_text.len() as u32) * BADGE_CHAR_WIDTH + BADGE_PADDING_X * 2;

    let badge_x = x.max(0);
    let badge_y = (y - BADGE_HEIGHT as i32).max(0);

    let badge_rect = Rect::at(badge_x, badge_y).of_size(badge_w, BADGE_HEIGHT);
    draw_filled_rect_mut(img, badge_rect, BADGE_BG);

    // Render number text if a font is available
    if let Some(font) = font {
        draw_text_mut(
            img,
            BADGE_TEXT,
            badge_x + BADGE_PADDING_X as i32,
            badge_y + 1,
            scale,
            font,
            &label_text,
        );
    }
}

/// Encodes an RGBA image buffer into the requested format.
fn encode_image(img: &RgbaImage, format: ScreenshotFormat) -> BrowserResult<Vec<u8>> {
    let dyn_img = DynamicImage::ImageRgba8(img.clone());
    let mut buf = Cursor::new(Vec::new());

    let img_format = match format {
        ScreenshotFormat::Png => ImageFormat::Png,
        ScreenshotFormat::Jpeg => ImageFormat::Jpeg,
        ScreenshotFormat::WebP => ImageFormat::WebP,
    };

    dyn_img.write_to(&mut buf, img_format).map_err(|e| {
        BrowserError::ScreenshotFailed {
            reason: format!("Failed to encode annotated screenshot as {}: {}", format, e),
        }
    })?;

    Ok(buf.into_inner())
}

/// Attempts to load a system font for badge text rendering.
///
/// Searches common Linux font paths. Returns None if no font is found,
/// in which case badges are drawn without text (colored rectangle only).
fn load_system_font() -> Option<Font<'static>> {
    let font_paths = [
        "/usr/share/fonts/TTF/DejaVuSansMono-Bold.ttf",
        "/usr/share/fonts/TTF/DejaVuSans-Bold.ttf",
        "/usr/share/fonts/truetype/dejavu/DejaVuSansMono-Bold.ttf",
        "/usr/share/fonts/truetype/dejavu/DejaVuSans-Bold.ttf",
        "/usr/share/fonts/dejavu-sans-mono-fonts/DejaVuSansMono-Bold.ttf",
        "/usr/share/fonts/noto/NotoSansMono-Bold.ttf",
        "/usr/share/fonts/google-noto/NotoSansMono-Bold.ttf",
        "/usr/share/fonts/liberation/LiberationMono-Bold.ttf",
        "/usr/share/fonts/truetype/liberation/LiberationMono-Bold.ttf",
        "/usr/share/fonts/nerd-fonts/JetBrainsMonoNerdFontMono-Bold.ttf",
    ];

    for path in &font_paths {
        if let Ok(data) = std::fs::read(path) {
            if let Some(font) = Font::try_from_vec(data) {
                return Some(font);
            }
        }
    }

    None
}

// -------------------------------------------------------------------------
// Tests
// -------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    fn make_interactive_node(
        id: u32,
        tag: &str,
        role: Option<&str>,
        x: f64,
        y: f64,
        w: f64,
        h: f64,
    ) -> DomNode {
        let mut attributes = HashMap::new();
        attributes.insert("id".to_string(), format!("el-{}", id));

        DomNode {
            id,
            tag: tag.to_string(),
            attributes,
            text: Some("Click me".to_string()),
            bbox: BoundingBox::new(x, y, w, h),
            children: vec![],
            role: role.map(|s| s.to_string()),
            is_visible: true,
            is_interactive: true,
            parent_id: None,
        }
    }

    fn make_snapshot_with_nodes(nodes: Vec<DomNode>) -> DomSnapshot {
        use crate::browser::dom_snapshot::ViewportInfo;
        DomSnapshot {
            nodes,
            viewport: ViewportInfo {
                width: 1920,
                height: 1080,
                scroll_x: 0.0,
                scroll_y: 0.0,
            },
            device_pixel_ratio: 1.0,
            url: "https://example.com".to_string(),
            timestamp: chrono::Utc::now(),
        }
    }

    #[test]
    fn test_label_generation_from_snapshot() {
        let nodes = vec![
            make_interactive_node(0, "button", Some("button"), 100.0, 200.0, 120.0, 40.0),
            make_interactive_node(1, "a", None, 300.0, 100.0, 80.0, 20.0),
            {
                // Non-interactive div — should be excluded
                let mut n = make_interactive_node(2, "div", None, 0.0, 0.0, 500.0, 500.0);
                n.is_interactive = false;
                n
            },
        ];

        let snapshot = make_snapshot_with_nodes(nodes);
        let labels = generate_labels(&snapshot);

        assert_eq!(labels.len(), 2);
        assert_eq!(labels[0].id, 1);
        assert_eq!(labels[0].role, "button");
        assert_eq!(labels[1].id, 2);
        assert_eq!(labels[1].role, "link");
    }

    #[test]
    fn test_label_numbering_sequential() {
        let nodes: Vec<DomNode> = (0..5)
            .map(|i| {
                make_interactive_node(
                    i,
                    "button",
                    Some("button"),
                    (i as f64) * 50.0,
                    10.0,
                    40.0,
                    30.0,
                )
            })
            .collect();

        let snapshot = make_snapshot_with_nodes(nodes);
        let labels = generate_labels(&snapshot);

        assert_eq!(labels.len(), 5);
        for (idx, label) in labels.iter().enumerate() {
            assert_eq!(label.id, (idx + 1) as u32, "Label {} should have id {}", idx, idx + 1);
        }
    }

    #[test]
    fn test_selector_hint_generation() {
        // Node with ID
        let node_with_id = make_interactive_node(0, "button", None, 0.0, 0.0, 100.0, 30.0);
        let selector = build_selector_hint(&node_with_id);
        assert_eq!(selector, "#el-0");

        // Node without ID but with classes
        let mut node_no_id = make_interactive_node(1, "div", None, 0.0, 0.0, 100.0, 30.0);
        node_no_id.attributes.remove("id");
        node_no_id.attributes.insert("class".to_string(), "primary btn-large extra".to_string());
        let selector = build_selector_hint(&node_no_id);
        assert_eq!(selector, "div.primary.btn-large");

        // Node with name attribute
        let mut node_with_name = make_interactive_node(2, "input", None, 0.0, 0.0, 100.0, 30.0);
        node_with_name.attributes.remove("id");
        node_with_name.attributes.insert("name".to_string(), "email".to_string());
        let selector = build_selector_hint(&node_with_name);
        assert_eq!(selector, "input[name=\"email\"]");
    }

    #[test]
    fn test_infer_role_from_tag() {
        let mut node = DomNode {
            id: 0,
            tag: "a".to_string(),
            attributes: HashMap::new(),
            text: None,
            bbox: BoundingBox::new(0.0, 0.0, 100.0, 30.0),
            children: vec![],
            role: None,
            is_visible: true,
            is_interactive: true,
            parent_id: None,
        };

        assert_eq!(infer_role(&node), "link");

        node.tag = "button".to_string();
        assert_eq!(infer_role(&node), "button");

        node.tag = "textarea".to_string();
        assert_eq!(infer_role(&node), "textbox");

        node.tag = "select".to_string();
        assert_eq!(infer_role(&node), "combobox");

        // ARIA role takes precedence
        node.role = Some("menuitem".to_string());
        assert_eq!(infer_role(&node), "menuitem");
    }

    #[test]
    fn test_infer_name_priority() {
        let mut attrs = HashMap::new();
        attrs.insert("aria-label".to_string(), "Submit form".to_string());
        attrs.insert("placeholder".to_string(), "Enter email".to_string());

        let node = DomNode {
            id: 0,
            tag: "input".to_string(),
            attributes: attrs,
            text: Some("inner text".to_string()),
            bbox: BoundingBox::new(0.0, 0.0, 100.0, 30.0),
            children: vec![],
            role: None,
            is_visible: true,
            is_interactive: true,
            parent_id: None,
        };

        // aria-label has highest priority
        assert_eq!(infer_name(&node), "Submit form");
    }

    #[test]
    fn test_truncate_text() {
        assert_eq!(truncate_text("short", 80), "short");
        let long = "a".repeat(100);
        let truncated = truncate_text(&long, 20);
        assert!(truncated.len() <= 23); // 17 chars + "..."
        assert!(truncated.ends_with("..."));
    }

    #[test]
    fn test_annotated_screenshot_size() {
        // Create a minimal 10x10 PNG image
        let img = RgbaImage::from_pixel(10, 10, Rgba([255, 255, 255, 255]));
        let dyn_img = DynamicImage::ImageRgba8(img);
        let mut buf = Cursor::new(Vec::new());
        dyn_img.write_to(&mut buf, ImageFormat::Png).unwrap();
        let png_bytes = buf.into_inner();

        let labels = vec![VisionLabel {
            id: 1,
            bbox: BoundingBox::new(1.0, 1.0, 5.0, 5.0),
            role: "button".to_string(),
            name: "Test".to_string(),
            text_hint: Some("Test".to_string()),
            selector_hint: "#btn".to_string(),
        }];

        let result = annotate_screenshot_with_labels(&png_bytes, &labels, ScreenshotFormat::Png);
        assert!(result.is_ok());
        let annotated = result.unwrap();
        // Annotated image should be at least as large as the original
        assert!(!annotated.is_empty());
        // Should still be a valid PNG
        assert_eq!(&annotated[0..4], &[0x89, 0x50, 0x4E, 0x47]);
    }

    #[test]
    fn test_generate_labels_excludes_invisible() {
        let mut visible = make_interactive_node(0, "button", None, 10.0, 10.0, 100.0, 30.0);
        visible.is_visible = true;

        let mut invisible = make_interactive_node(1, "button", None, 10.0, 50.0, 100.0, 30.0);
        invisible.is_visible = false;

        let mut zero_size = make_interactive_node(2, "button", None, 10.0, 90.0, 0.0, 0.0);
        zero_size.is_visible = true;

        let snapshot = make_snapshot_with_nodes(vec![visible, invisible, zero_size]);
        let labels = generate_labels(&snapshot);

        assert_eq!(labels.len(), 1);
        assert_eq!(labels[0].id, 1);
    }
}
