//! Screenshot annotation and OCR support
//!
//! Provides functions to annotate screenshots with numbered element overlays
//! and optional Tesseract OCR text extraction.

use image::{DynamicImage, ImageFormat, Rgba};
use imageproc::drawing::{draw_filled_rect_mut, draw_hollow_rect_mut, draw_text_mut};
use imageproc::rect::Rect;
use rusttype::{Font, Scale};
use serde::{Deserialize, Serialize};
use std::io::Cursor;
use tracing::warn;

/// An element found on the page with its bounding box and metadata
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AnnotatedElement {
    pub id: u32,
    #[serde(rename = "type")]
    pub element_type: String,
    #[serde(default)]
    pub text: String,
    pub x: f64,
    pub y: f64,
    pub w: f64,
    pub h: f64,
    #[serde(default)]
    pub selector: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub href: Option<String>,
}

/// Result of OCR text extraction
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OcrResult {
    pub text: String,
}

/// Color for element type outlines
fn element_color(element_type: &str) -> Rgba<u8> {
    match element_type {
        "link" => Rgba([0, 100, 255, 220]),
        "button" => Rgba([0, 200, 0, 220]),
        "input" => Rgba([255, 165, 0, 220]),
        "price" => Rgba([255, 255, 0, 220]),
        "image" => Rgba([200, 0, 200, 220]),
        _ => Rgba([255, 0, 0, 220]),
    }
}

/// Background color for number labels
fn label_bg_color(element_type: &str) -> Rgba<u8> {
    match element_type {
        "link" => Rgba([0, 70, 180, 240]),
        "button" => Rgba([0, 150, 0, 240]),
        "input" => Rgba([200, 130, 0, 240]),
        "price" => Rgba([180, 180, 0, 240]),
        "image" => Rgba([150, 0, 150, 240]),
        _ => Rgba([200, 0, 0, 240]),
    }
}

/// Try to load a system font for number labels
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

    warn!("No system font found for annotation labels");
    None
}

/// Annotate a PNG screenshot with numbered, colored element overlays
pub fn annotate_screenshot(
    png_bytes: &[u8],
    elements: &[AnnotatedElement],
) -> anyhow::Result<Vec<u8>> {
    let img = image::load_from_memory_with_format(png_bytes, ImageFormat::Png)?;
    let mut rgba = img.to_rgba8();

    let font = load_system_font();
    let scale = Scale::uniform(14.0);

    for element in elements {
        let x = element.x as i32;
        let y = element.y as i32;
        let w = element.w as u32;
        let h = element.h as u32;

        if w == 0 || h == 0 {
            continue;
        }

        let color = element_color(&element.element_type);
        let bg = label_bg_color(&element.element_type);

        // Draw 2px border rectangle around element
        let rect = Rect::at(x, y).of_size(w, h);
        draw_hollow_rect_mut(&mut rgba, rect, color);
        if w > 2 && h > 2 {
            let inner = Rect::at(x + 1, y + 1).of_size(w - 2, h - 2);
            draw_hollow_rect_mut(&mut rgba, inner, color);
        }

        // Draw number label above the element
        let label_text = element.id.to_string();
        let label_w = (label_text.len() as u32) * 9 + 6;
        let label_h: u32 = 16;

        let label_x = x.max(0);
        let label_y = (y - label_h as i32).max(0);

        let label_rect = Rect::at(label_x, label_y).of_size(label_w, label_h);
        draw_filled_rect_mut(&mut rgba, label_rect, bg);

        // Draw number text if font available
        if let Some(ref font) = font {
            draw_text_mut(
                &mut rgba,
                Rgba([255, 255, 255, 255]),
                label_x + 3,
                label_y + 1,
                scale,
                font,
                &label_text,
            );
        }
    }

    // Encode back to PNG
    let dyn_img = DynamicImage::ImageRgba8(rgba);
    let mut buf = Cursor::new(Vec::new());
    dyn_img.write_to(&mut buf, ImageFormat::Png)?;

    Ok(buf.into_inner())
}

/// Run Tesseract OCR on a PNG screenshot
#[cfg(feature = "ocr")]
pub fn ocr_screenshot(png_bytes: &[u8], lang: &str) -> anyhow::Result<OcrResult> {
    use leptess::LepTess;

    let mut lt = LepTess::new(None, lang)
        .map_err(|e| anyhow::anyhow!("Failed to init Tesseract: {:?}", e))?;

    lt.set_image_from_mem(png_bytes)
        .map_err(|e| anyhow::anyhow!("Failed to set image: {:?}", e))?;

    let text = lt
        .get_utf8_text()
        .map_err(|e| anyhow::anyhow!("OCR failed: {:?}", e))?;

    Ok(OcrResult { text })
}

/// Stub when OCR feature is not enabled
#[cfg(not(feature = "ocr"))]
pub fn ocr_screenshot(_png_bytes: &[u8], _lang: &str) -> anyhow::Result<OcrResult> {
    Err(anyhow::anyhow!(
        "OCR feature not enabled. Build with --features ocr"
    ))
}

/// Generate JavaScript to find interactive elements on the page.
///
/// The KI chooses which `types` to request (links, buttons, inputs, prices, images)
/// and optionally provides a custom CSS `selector`.
pub fn generate_find_elements_js(types: &[String], custom_selector: Option<&str>) -> String {
    let types_json = serde_json::to_string(types).unwrap_or_else(|_| "[]".to_string());
    let selector_json = match custom_selector {
        Some(s) => serde_json::to_string(s).unwrap_or_else(|_| "null".to_string()),
        None => "null".to_string(),
    };

    format!(
        r#"
(function() {{
    const types = {types_json};
    const customSelector = {selector_json};
    let elements = [];
    let seen = new Set();

    function addElements(nodeList, elType) {{
        for (const el of nodeList) {{
            if (seen.has(el)) continue;
            seen.add(el);
            elements.push({{ el, elType }});
        }}
    }}

    if (types.includes('links'))
        addElements(document.querySelectorAll('a[href]'), 'link');
    if (types.includes('buttons'))
        addElements(document.querySelectorAll('button, input[type="submit"], input[type="button"], [role="button"]'), 'button');
    if (types.includes('inputs'))
        addElements(document.querySelectorAll('input:not([type="hidden"]):not([type="submit"]):not([type="button"]), textarea, select'), 'input');
    if (types.includes('prices'))
        addElements(document.querySelectorAll('[class*="price" i], [class*="Price"], [class*="cost" i], [class*="preis" i], [data-price], .gh_price'), 'price');
    if (types.includes('images'))
        addElements(document.querySelectorAll('img[src]'), 'image');
    if (customSelector)
        addElements(document.querySelectorAll(customSelector), 'custom');

    return elements
        .filter(item => {{
            const el = item.el;
            if (!el.offsetParent && el.tagName !== 'BODY' && el.tagName !== 'HTML') return false;
            const rect = el.getBoundingClientRect();
            return rect.width > 0 && rect.height > 0 &&
                   rect.top < window.innerHeight && rect.bottom > 0 &&
                   rect.left < window.innerWidth && rect.right > 0;
        }})
        .map((item, i) => {{
            const el = item.el;
            const rect = el.getBoundingClientRect();

            let selector = '';
            if (el.id) {{
                selector = '#' + CSS.escape(el.id);
            }} else {{
                const tag = el.tagName.toLowerCase();
                const classes = Array.from(el.classList).slice(0, 2).map(c => '.' + CSS.escape(c)).join('');
                const parent = el.parentElement;
                if (parent) {{
                    const siblings = Array.from(parent.children).filter(c => c.tagName === el.tagName);
                    if (siblings.length > 1) {{
                        const idx = siblings.indexOf(el) + 1;
                        selector = tag + classes + ':nth-of-type(' + idx + ')';
                    }} else {{
                        selector = tag + classes;
                    }}
                }} else {{
                    selector = tag + classes;
                }}
            }}

            return {{
                id: i + 1,
                type: item.elType,
                text: (el.textContent || el.alt || el.title || el.placeholder || '').trim().substring(0, 100),
                x: Math.round(rect.x),
                y: Math.round(rect.y),
                w: Math.round(rect.width),
                h: Math.round(rect.height),
                selector: selector,
                href: el.href || null
            }};
        }});
}})()
"#,
        types_json = types_json,
        selector_json = selector_json,
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_generate_js_with_types() {
        let types = vec!["links".to_string(), "buttons".to_string()];
        let js = generate_find_elements_js(&types, None);
        assert!(js.contains("links"));
        assert!(js.contains("buttons"));
        assert!(js.contains("customSelector = null"));
    }

    #[test]
    fn test_generate_js_with_selector() {
        let types = vec![];
        let js = generate_find_elements_js(&types, Some(".my-class"));
        assert!(js.contains(".my-class"));
    }

    #[test]
    fn test_element_color_variants() {
        assert_eq!(element_color("link")[0], 0);
        assert_eq!(element_color("button")[1], 200);
        assert_eq!(element_color("input")[0], 255);
        assert_eq!(element_color("unknown")[0], 255); // red fallback
    }

    #[test]
    fn test_annotated_element_serde() {
        let el = AnnotatedElement {
            id: 1,
            element_type: "link".to_string(),
            text: "Test".to_string(),
            x: 10.0,
            y: 20.0,
            w: 100.0,
            h: 30.0,
            selector: "a.test".to_string(),
            href: Some("https://example.com".to_string()),
        };
        let json = serde_json::to_string(&el).unwrap();
        assert!(json.contains("\"type\":\"link\""));

        let parsed: AnnotatedElement = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed.element_type, "link");
        assert_eq!(parsed.id, 1);
    }
}
