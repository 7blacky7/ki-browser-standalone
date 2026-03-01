//! Context menu action execution for the GUI browser viewport.
//!
//! Handles right-click context menu actions such as back/forward navigation,
//! reload, clipboard operations (copy/cut/paste/select-all), element inspection
//! with background JS execution, and opening the DevTools/page-source viewer.
//! Extracted from `browser_app.rs` to isolate context menu action dispatch.

use std::sync::Arc;
use std::sync::atomic::Ordering;

use uuid::Uuid;

use crate::browser::cef_engine::CefBrowserEngine;

use super::browser_app::KiBrowserApp;
use super::context_menu::ContextMenuAction;
use super::element_inspector::ElementDetails;
use super::gui_inspect;

impl KiBrowserApp {
    /// Execute a context menu action from the viewport right-click menu.
    ///
    /// Dispatches navigation, clipboard, element inspection, and DevTools
    /// actions. Element inspection spawns a background thread to fetch DOM
    /// attributes via JS while immediately showing overlay-based data.
    pub(super) fn handle_context_menu_action(&mut self, action: ContextMenuAction) {
        match action {
            ContextMenuAction::InspectElement => {
                self.handle_inspect_element();
            }
            ContextMenuAction::Back => {
                if let Some(tab) = self.tabs.get(self.active_tab) {
                    self.engine.send_go_back(tab.id);
                }
            }
            ContextMenuAction::Forward => {
                if let Some(tab) = self.tabs.get(self.active_tab) {
                    self.engine.send_go_forward(tab.id);
                }
            }
            ContextMenuAction::Reload => {
                let url = self.current_url().to_string();
                if !url.is_empty() {
                    self.navigate(&url);
                }
            }
            ContextMenuAction::Copy => {
                if let Some(tab) = self.tabs.get(self.active_tab) {
                    fire_and_forget_js(&self.engine, tab.id, "document.execCommand('copy')");
                }
            }
            ContextMenuAction::Cut => {
                if let Some(tab) = self.tabs.get(self.active_tab) {
                    fire_and_forget_js(&self.engine, tab.id, "document.execCommand('cut')");
                }
            }
            ContextMenuAction::Paste => {
                if let Some(tab) = self.tabs.get(self.active_tab) {
                    fire_and_forget_js(&self.engine, tab.id, "document.execCommand('paste')");
                }
            }
            ContextMenuAction::SelectAll => {
                if let Some(tab) = self.tabs.get(self.active_tab) {
                    fire_and_forget_js(
                        &self.engine, tab.id, "document.execCommand('selectAll')",
                    );
                }
            }
            ContextMenuAction::ViewSource => {
                self.devtools_shared.state.open.store(true, Ordering::Relaxed);
            }
        }
    }

    /// Open element inspector with overlay data and fetch full DOM details via JS.
    fn handle_inspect_element(&mut self) {
        if let Ok(ie) = self.inspected_element.lock() {
            if let Some(ref elem) = *ie {
                // 1. Immediately show basic data from the overlay element
                let details = ElementDetails {
                    tag: elem.label.clone(),
                    element_type: elem.element_type.clone(),
                    x: elem.x,
                    y: elem.y,
                    w: elem.w,
                    h: elem.h,
                    ocr_tesseract: elem.ocr_tesseract.clone().unwrap_or_default(),
                    ocr_paddleocr: elem.ocr_paddleocr.clone().unwrap_or_default(),
                    ocr_surya: elem.ocr_surya.clone().unwrap_or_default(),
                    ..Default::default()
                };
                if let Ok(mut el) = self.inspector_state.element.lock() {
                    *el = Some(details);
                }
                self.inspector_state.open.store(true, Ordering::Relaxed);

                // 2. Fetch detailed DOM attributes via JS in a background thread
                let engine = self.engine.clone();
                let inspector = Arc::clone(&self.inspector_state);
                let tab_id = self.tabs.get(self.active_tab).map(|t| t.id);
                let elem_x = elem.x as f64;
                let elem_y = elem.y as f64;

                if let Some(tab_id) = tab_id {
                    std::thread::spawn(move || {
                        fetch_element_details_background(engine, inspector, tab_id, elem_x, elem_y);
                    });
                }
            }
        }
    }
}

/// Fetch detailed DOM element attributes via JavaScript in a background thread.
///
/// Uses `elementFromPoint` to locate the DOM element at the given viewport
/// coordinates, then extracts tag name, attributes, computed styles, and
/// bounding rect. Results are written back into the shared inspector state.
fn fetch_element_details_background(
    engine: Arc<CefBrowserEngine>,
    inspector: Arc<super::element_inspector::ElementInspectorState>,
    tab_id: Uuid,
    elem_x: f64,
    elem_y: f64,
) {
    let js = gui_inspect::element_inspect_js(elem_x, elem_y);
    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build();
    if let Ok(rt) = rt {
        match rt.block_on(engine.execute_js_with_result(tab_id, &js)) {
            Ok(Some(result)) => {
                if let Some(details) = gui_inspect::parse_element_details(&result) {
                    if let Ok(mut el) = inspector.element.lock() {
                        *el = Some(details);
                    }
                }
            }
            Ok(None) => {
                tracing::warn!("JS returned no result for element inspection");
            }
            Err(e) => {
                tracing::warn!("JS execution failed for element inspection: {}", e);
            }
        }
    }
}

/// Execute a JavaScript snippet on a tab without waiting for the result.
///
/// Spawns a background thread with a short-lived tokio runtime because the
/// engine's `execute_js` method is async but the GUI render loop is sync.
pub(super) fn fire_and_forget_js(engine: &Arc<CefBrowserEngine>, tab_id: Uuid, script: &str) {
    let engine = engine.clone();
    let script = script.to_string();
    std::thread::spawn(move || {
        let rt = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build();
        if let Ok(rt) = rt {
            let _ = rt.block_on(async { engine.execute_js(tab_id, &script).await });
        }
    });
}
