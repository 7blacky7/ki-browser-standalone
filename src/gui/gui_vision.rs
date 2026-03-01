//! Direct vision tactic execution and frame buffer utilities for the GUI.
//!
//! Contains helpers that run vision image/text tactics directly through the
//! CEF engine (no REST API round-trip), DOM snapshot capture, and BGRA frame
//! buffer to PNG conversion for OCR and screenshot annotation.
//!
//! All functions are standalone (no `self`) so they can be called from
//! background threads spawned by `KiBrowserApp::handle_devtools_actions`.

use std::sync::Arc;

use parking_lot::RwLock;
use uuid::Uuid;

use crate::browser::cef_engine::CefBrowserEngine;

use super::devtools;

/// Executes JavaScript via the CEF engine from a background thread.
///
/// Sends a `CefCommand::ExecuteJsWithResult` to the CEF command thread and
/// blocks the calling thread until the result arrives. The CEF command thread
/// pumps `do_message_loop_work()` internally while waiting for the JS
/// console.log result via the KI_RESULT protocol, so no deadlock occurs.
///
/// Must only be called from a non-tokio background thread (e.g. `std::thread::spawn`).
/// Creates a minimal single-threaded tokio runtime without IO/timer drivers since
/// the oneshot channel polling does not require those subsystems.
pub(super) fn execute_js_blocking(
    engine: &Arc<CefBrowserEngine>,
    tab_id: Uuid,
    script: &str,
) -> Result<String, String> {
    tracing::debug!("execute_js_blocking: starting JS execution for tab {}", tab_id);

    // Build a minimal current-thread runtime with timer support for the
    // caller-side timeout. IO driver is omitted to avoid epoll conflicts
    // with the global tokio runtime that owns the main IO driver.
    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_time()
        .build()
        .map_err(|e| format!("Tokio runtime error: {}", e))?;

    let result = rt.block_on(async {
        // Caller-side timeout: 15 seconds (CEF internal timeout is 10s, this
        // catches cases where the CEF command thread itself is stuck).
        match tokio::time::timeout(
            std::time::Duration::from_secs(15),
            engine.execute_js_with_result(tab_id, script),
        )
        .await
        {
            Ok(Ok(Some(result))) => Ok(result),
            Ok(Ok(None)) => Err("JavaScript returned no result".to_string()),
            Ok(Err(e)) => Err(format!("JS execution failed: {}", e)),
            Err(_) => Err("JS execution timed out after 15s".to_string()),
        }
    });

    match &result {
        Ok(s) => tracing::debug!("execute_js_blocking: JS returned {} bytes", s.len()),
        Err(e) => tracing::warn!("execute_js_blocking: {}", e),
    }

    result
}

/// Captures DOM snapshot via JavaScript and returns the parsed snapshot.
///
/// Builds the snapshot extraction script, executes it via `execute_js_blocking`,
/// and parses the JSON result into a `DomSnapshot`. Used by vision tactics to
/// obtain element bounding boxes for screenshot annotation.
pub(super) fn capture_dom_snapshot(
    engine: &Arc<CefBrowserEngine>,
    tab_id: Uuid,
) -> Result<crate::browser::dom_snapshot::DomSnapshot, String> {
    tracing::debug!("capture_dom_snapshot: starting for tab {}", tab_id);

    let config = crate::browser::dom_snapshot::SnapshotConfig {
        max_nodes: 5000,
        include_text: true,
    };
    let script = crate::browser::dom_snapshot::build_snapshot_script(&config);
    let json_str = execute_js_blocking(engine, tab_id, &script)?;
    let snapshot = crate::browser::dom_snapshot::parse_snapshot_json(&json_str)
        .map_err(|e| format!("DOM snapshot parsing failed: {}", e))?;

    tracing::debug!("capture_dom_snapshot: {} nodes found", snapshot.nodes.len());
    Ok(snapshot)
}

/// Runs a vision image tactic (annotated/dom_annotate) directly using the
/// CEF engine frame buffer and JavaScript execution. No REST API needed.
///
/// Captures a PNG screenshot from the frame buffer, obtains a DOM snapshot via
/// JS, generates element labels, and annotates the image with bounding boxes.
pub(super) fn run_vision_image_direct(
    tactic: &str,
    tab_id: Uuid,
    frame_buffer: &Arc<RwLock<Vec<u8>>>,
    frame_size: &Arc<RwLock<(u32, u32)>>,
    engine: &Arc<CefBrowserEngine>,
) -> Result<Vec<u8>, String> {
    tracing::debug!("run_vision_image_direct: tactic={}, tab={}", tactic, tab_id);

    // 1. Get screenshot from frame buffer
    let png_data = frame_buffer_to_png(frame_buffer, frame_size)?;
    tracing::debug!("run_vision_image_direct: PNG {} bytes", png_data.len());

    // 2. Get DOM snapshot via JavaScript
    let snapshot = capture_dom_snapshot(engine, tab_id)?;
    tracing::debug!("run_vision_image_direct: snapshot {} nodes", snapshot.nodes.len());

    // 3. Generate labels and annotate
    match tactic {
        "annotated" => {
            let labels = crate::browser::vision::generate_labels(&snapshot);
            tracing::debug!("run_vision_image_direct: {} labels generated", labels.len());
            if labels.is_empty() {
                return Err("No interactive elements found on page".to_string());
            }
            crate::browser::vision::annotate_screenshot_with_labels(
                &png_data,
                &labels,
                crate::browser::screenshot::ScreenshotFormat::Png,
            )
            .map_err(|e| format!("Annotation failed: {}", e))
        }
        "dom_annotate" => {
            // DOM Annotate: annotate all visible elements, not just interactive
            let labels: Vec<crate::browser::vision::VisionLabel> = snapshot
                .nodes
                .iter()
                .filter(|n| n.is_visible && n.bbox.is_visible())
                .enumerate()
                .map(|(idx, node)| crate::browser::vision::VisionLabel {
                    id: (idx + 1) as u32,
                    bbox: node.bbox,
                    role: node.role.clone().unwrap_or_else(|| node.tag.clone()),
                    name: node
                        .text
                        .as_ref()
                        .map(|t| t.chars().take(80).collect())
                        .unwrap_or_default(),
                    text_hint: node.text.clone(),
                    selector_hint: node
                        .attributes
                        .get("id")
                        .map(|id| format!("#{}", id))
                        .unwrap_or_else(|| node.tag.clone()),
                })
                .collect();
            tracing::debug!("run_vision_image_direct: {} labels generated", labels.len());
            if labels.is_empty() {
                return Err("No visible elements found on page".to_string());
            }
            crate::browser::vision::annotate_screenshot_with_labels(
                &png_data,
                &labels,
                crate::browser::screenshot::ScreenshotFormat::Png,
            )
            .map_err(|e| format!("DOM annotation failed: {}", e))
        }
        other => Err(format!("Unknown image tactic: {}", other)),
    }
}

/// Runs a vision text tactic (labels/dom_snapshot/etc.) directly using
/// the CEF engine JavaScript execution. No REST API needed.
///
/// Logs warnings on error and delegates to `run_vision_text_direct_inner`
/// for the actual tactic dispatch.
pub(super) fn run_vision_text_direct(
    tactic: &str,
    tab_id: Uuid,
    engine: &Arc<CefBrowserEngine>,
) -> Result<String, String> {
    tracing::debug!("run_vision_text_direct: tactic={}, tab={}", tactic, tab_id);
    let result = run_vision_text_direct_inner(tactic, tab_id, engine);
    if let Err(ref e) = result {
        tracing::warn!("run_vision_text_direct: {}", e);
    }
    result
}

/// Dispatches vision text tactics to the appropriate JS or DOM snapshot handler.
///
/// Supported tactics: `labels`, `dom_snapshot`, `structured_data`,
/// `content_extract`, `structure_analysis`, `forms`. Each tactic runs
/// JavaScript via `execute_js_blocking` or captures a DOM snapshot, then
/// returns the result as a pretty-printed JSON string.
fn run_vision_text_direct_inner(
    tactic: &str,
    tab_id: Uuid,
    engine: &Arc<CefBrowserEngine>,
) -> Result<String, String> {
    match tactic {
        "labels" => {
            let snapshot = capture_dom_snapshot(engine, tab_id)?;
            let labels = crate::browser::vision::generate_labels(&snapshot);
            let response = serde_json::json!({
                "count": labels.len(),
                "labels": labels,
            });
            serde_json::to_string_pretty(&response)
                .map_err(|e| format!("JSON serialization failed: {}", e))
        }
        "dom_snapshot" => {
            let snapshot = capture_dom_snapshot(engine, tab_id)?;
            serde_json::to_string_pretty(&snapshot)
                .map_err(|e| format!("JSON serialization failed: {}", e))
        }
        "structured_data" => {
            let script = r#"(function() {
                var result = { jsonLd: [], openGraph: {}, meta: {}, microdata: [] };
                document.querySelectorAll('script[type="application/ld+json"]').forEach(function(s) {
                    try { result.jsonLd.push(JSON.parse(s.textContent)); } catch(e) {}
                });
                document.querySelectorAll('meta[property^="og:"]').forEach(function(m) {
                    result.openGraph[m.getAttribute('property')] = m.getAttribute('content');
                });
                document.querySelectorAll('meta[name]').forEach(function(m) {
                    result.meta[m.getAttribute('name')] = m.getAttribute('content');
                });
                return JSON.stringify(result);
            })()"#;
            let json_str = execute_js_blocking(engine, tab_id, script)?;
            // Pretty-print the JSON
            match serde_json::from_str::<serde_json::Value>(&json_str) {
                Ok(val) => serde_json::to_string_pretty(&val)
                    .map_err(|e| format!("JSON error: {}", e)),
                Err(_) => Ok(json_str),
            }
        }
        "content_extract" => {
            let script = r#"(function() {
                var article = document.querySelector('article') || document.querySelector('main') || document.body;
                var clone = article.cloneNode(true);
                clone.querySelectorAll('script,style,nav,footer,header,aside,.ad,.ads,.advertisement').forEach(function(el) { el.remove(); });
                var text = clone.innerText || clone.textContent || '';
                return JSON.stringify({
                    title: document.title,
                    url: window.location.href,
                    content: text.trim().substring(0, 50000),
                    length: text.trim().length
                });
            })()"#;
            let json_str = execute_js_blocking(engine, tab_id, script)?;
            match serde_json::from_str::<serde_json::Value>(&json_str) {
                Ok(val) => serde_json::to_string_pretty(&val)
                    .map_err(|e| format!("JSON error: {}", e)),
                Err(_) => Ok(json_str),
            }
        }
        "structure_analysis" => {
            let script = r#"(function() {
                var headings = [];
                document.querySelectorAll('h1,h2,h3,h4,h5,h6').forEach(function(h) {
                    headings.push({ level: parseInt(h.tagName[1]), text: h.textContent.trim().substring(0, 200) });
                });
                var links = document.querySelectorAll('a[href]').length;
                var images = document.querySelectorAll('img').length;
                var forms = document.querySelectorAll('form').length;
                var buttons = document.querySelectorAll('button,input[type="submit"],input[type="button"]').length;
                var inputs = document.querySelectorAll('input,textarea,select').length;
                var sections = [];
                document.querySelectorAll('section,article,nav,aside,main,header,footer').forEach(function(s) {
                    sections.push({ tag: s.tagName.toLowerCase(), id: s.id || null, className: s.className || null });
                });
                return JSON.stringify({
                    title: document.title,
                    url: window.location.href,
                    headings: headings,
                    counts: { links: links, images: images, forms: forms, buttons: buttons, inputs: inputs },
                    sections: sections,
                    pageType: document.querySelector('article') ? 'article' : (forms > 0 ? 'form' : 'general')
                });
            })()"#;
            let json_str = execute_js_blocking(engine, tab_id, script)?;
            match serde_json::from_str::<serde_json::Value>(&json_str) {
                Ok(val) => serde_json::to_string_pretty(&val)
                    .map_err(|e| format!("JSON error: {}", e)),
                Err(_) => Ok(json_str),
            }
        }
        "forms" => {
            let script = r#"(function() {
                var forms = [];
                document.querySelectorAll('form').forEach(function(f, fi) {
                    var fields = [];
                    f.querySelectorAll('input,textarea,select,button').forEach(function(el) {
                        fields.push({
                            tag: el.tagName.toLowerCase(),
                            type: el.type || null,
                            name: el.name || null,
                            id: el.id || null,
                            placeholder: el.placeholder || null,
                            required: el.required || false,
                            value: el.type === 'password' ? '***' : (el.value || '').substring(0, 100)
                        });
                    });
                    forms.push({
                        index: fi,
                        action: f.action || null,
                        method: (f.method || 'GET').toUpperCase(),
                        id: f.id || null,
                        name: f.name || null,
                        fields: fields
                    });
                });
                return JSON.stringify({ count: forms.length, forms: forms });
            })()"#;
            let json_str = execute_js_blocking(engine, tab_id, script)?;
            match serde_json::from_str::<serde_json::Value>(&json_str) {
                Ok(val) => serde_json::to_string_pretty(&val)
                    .map_err(|e| format!("JSON error: {}", e)),
                Err(_) => Ok(json_str),
            }
        }
        other => Err(format!("Unknown text tactic: {}", other)),
    }
}

/// Converts a CEF BGRA frame buffer to PNG bytes.
///
/// Uses `chunks_exact(4)` for efficient BGRA→RGBA channel swapping, matching
/// the conversion approach in cef_render.rs. Releases the frame buffer lock
/// before PNG encoding to minimise lock contention with the render thread.
///
/// Used by OCR and Vision to get screenshots without going through the REST API.
pub(super) fn frame_buffer_to_png(
    frame_buffer: &Arc<RwLock<Vec<u8>>>,
    frame_size: &Arc<RwLock<(u32, u32)>>,
) -> Result<Vec<u8>, String> {
    use image::{ImageBuffer, ImageOutputFormat, Rgba};

    let fb = frame_buffer.read();
    let (w, h) = *frame_size.read();

    tracing::debug!("frame_buffer_to_png: converting {}x{} frame", w, h);

    if fb.is_empty() || w == 0 || h == 0 {
        return Err("No frame buffer available (page not loaded yet?)".to_string());
    }

    let expected_len = (w as usize) * (h as usize) * 4;
    if fb.len() < expected_len {
        return Err(format!(
            "Frame buffer too small: {} bytes for {}x{} (expected {})",
            fb.len(),
            w,
            h,
            expected_len
        ));
    }

    // Efficient BGRA → RGBA conversion using chunks_exact (avoids per-pixel indexing overhead).
    // CEF delivers frames in BGRA order: [B=0, G=1, R=2, A=3].
    // PNG expects RGBA order, so we swap B↔R channels.
    let mut rgba = Vec::with_capacity(expected_len);
    for chunk in fb[..expected_len].chunks_exact(4) {
        rgba.push(chunk[2]); // R ← BGRA[2]
        rgba.push(chunk[1]); // G ← BGRA[1]
        rgba.push(chunk[0]); // B ← BGRA[0]
        rgba.push(chunk[3]); // A ← BGRA[3]
    }
    drop(fb); // Release frame buffer lock early before PNG encoding

    let img: ImageBuffer<Rgba<u8>, Vec<u8>> = ImageBuffer::from_raw(w, h, rgba)
        .ok_or_else(|| "ImageBuffer::from_raw failed".to_string())?;

    let mut output = Vec::new();
    img.write_to(&mut std::io::Cursor::new(&mut output), ImageOutputFormat::Png)
        .map_err(|e| format!("PNG encoding failed: {}", e))?;

    tracing::debug!("frame_buffer_to_png: PNG {} bytes", output.len());
    Ok(output)
}

/// Draws red bounding boxes with 1-based region indices onto a PNG screenshot.
///
/// Decodes the source PNG, draws a 2-pixel red rectangle around each OCR region,
/// re-encodes to PNG and returns the annotated bytes. Used to produce the
/// `ocr_image` shown in DevTools above the per-region table.
pub(super) fn draw_ocr_bounding_boxes(
    png_data: &[u8],
    regions: &[devtools::OcrDisplayRegion],
) -> Result<Vec<u8>, String> {
    use image::{ImageOutputFormat, Rgba};

    let mut img = image::load_from_memory(png_data)
        .map_err(|e| format!("PNG decode failed: {}", e))?
        .to_rgba8();

    let red = Rgba([220u8, 50u8, 50u8, 255u8]);

    for region in regions {
        let x0 = region.x.max(0.0) as u32;
        let y0 = region.y.max(0.0) as u32;
        let x1 = (region.x + region.w).max(0.0) as u32;
        let y1 = (region.y + region.h).max(0.0) as u32;
        let img_w = img.width();
        let img_h = img.height();

        // Draw the four sides of the bounding box rectangle (2 px thick).
        for thickness in 0u32..2 {
            let top = y0.saturating_add(thickness).min(img_h.saturating_sub(1));
            let bottom = y1.saturating_add(thickness).min(img_h.saturating_sub(1));
            let left = x0.saturating_add(thickness).min(img_w.saturating_sub(1));
            let right = x1.saturating_add(thickness).min(img_w.saturating_sub(1));

            // Top and bottom horizontal lines
            for x in left..=right.min(img_w.saturating_sub(1)) {
                img.put_pixel(x, top, red);
                img.put_pixel(x, bottom, red);
            }
            // Left and right vertical lines
            for y in top..=bottom.min(img_h.saturating_sub(1)) {
                img.put_pixel(left, y, red);
                img.put_pixel(right, y, red);
            }
        }
    }

    let mut output = Vec::new();
    img.write_to(&mut std::io::Cursor::new(&mut output), ImageOutputFormat::Png)
        .map_err(|e| format!("PNG encode failed: {}", e))?;
    Ok(output)
}
