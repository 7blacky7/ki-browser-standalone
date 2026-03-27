//! Vision image tactic execution (annotated screenshots, DOM annotation).
//!
//! Runs `annotated` and `dom_annotate` tactics directly using the CEF engine
//! frame buffer and JavaScript DOM snapshots, producing annotated PNG images
//! with labeled bounding boxes around interactive or visible elements.

use std::sync::Arc;

use parking_lot::RwLock;
use uuid::Uuid;

use crate::browser::cef_engine::CefBrowserEngine;

use super::frame_buffer::frame_buffer_to_png;
use super::js_execution::capture_dom_snapshot;

/// Runs a vision image tactic (annotated/dom_annotate) directly using the
/// CEF engine frame buffer and JavaScript execution. No REST API needed.
///
/// Captures a PNG screenshot from the frame buffer, obtains a DOM snapshot via
/// JS, generates element labels, and annotates the image with bounding boxes.
pub(in crate::gui) fn run_vision_image_direct(
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
