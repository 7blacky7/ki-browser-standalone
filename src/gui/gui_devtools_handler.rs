//! DevTools action queue processing for the GUI browser application.
//!
//! Drains queued `DevToolsAction` variants from the shared DevTools state and
//! executes them in the main application context. Handles page source loading,
//! tab switching/closing from DevTools, vision tactic execution, and OCR engine
//! invocation with bounding-box annotation. Extracted from `browser_app.rs` to
//! isolate the DevTools ↔ main-app communication concern.

use std::sync::Arc;

use uuid::Uuid;

use crate::browser::cef_engine::CefBrowserEngine;

use super::browser_app::KiBrowserApp;
use super::devtools::{self, DevToolsAction};
use super::gui_devtools_actions::{self, TabSnapshot};
use super::gui_vision;

impl KiBrowserApp {
    /// Update shared DevTools page info and tab list from current GUI state.
    ///
    /// Delegates to `gui_devtools_actions::update_devtools_shared_state` which
    /// operates on plain `TabSnapshot` values so it does not need `&self`.
    pub(super) fn update_devtools_shared_state(&self) {
        let snapshots: Vec<TabSnapshot> = self.tabs.iter().map(|t| TabSnapshot {
            id: t.id,
            title: t.title.clone(),
            url: t.url.clone(),
            is_loading: t.is_loading,
            can_go_back: t.can_go_back,
            can_go_forward: t.can_go_forward,
        }).collect();
        let active_snapshot = snapshots.get(self.active_tab);
        gui_devtools_actions::update_devtools_shared_state(
            &self.devtools_shared,
            active_snapshot,
            &snapshots,
            self.active_tab,
            self.api_port,
        );
    }

    /// Drain queued DevTools actions and handle them in the main app context.
    pub(super) fn handle_devtools_actions(&mut self) {
        let actions: Vec<DevToolsAction> = self.devtools_shared.state.actions
            .lock()
            .map(|mut a| a.drain(..).collect())
            .unwrap_or_default();

        for dt_action in actions {
            match dt_action {
                DevToolsAction::LoadSource(_) => {
                    self.handle_load_source();
                }
                DevToolsAction::SwitchToTab(idx) => {
                    self.handle_switch_to_tab(idx);
                }
                DevToolsAction::CloseTab(idx) => {
                    self.close_tab(idx);
                }
                DevToolsAction::RunVisionTactic { tactic, .. } => {
                    self.handle_run_vision_tactic(tactic);
                }
                DevToolsAction::RunOcr { engines, .. } => {
                    self.handle_run_ocr(&engines);
                }
            }
        }
    }

    /// Fetch page source HTML via JS in a background thread for DevTools display.
    fn handle_load_source(&self) {
        if let Some(tab) = self.tabs.get(self.active_tab) {
            let tab_id = tab.id;
            let engine = self.engine.clone();
            let source_handle = self.devtools_shared.state.source_handle();
            self.devtools_shared.state.set_source_loading();
            // Fetch source in background thread (non-blocking)
            std::thread::spawn(move || {
                let rt = tokio::runtime::Builder::new_current_thread()
                    .enable_all()
                    .build();
                match rt {
                    Ok(rt) => {
                        let result = rt.block_on(async {
                            engine.execute_js_with_result(
                                tab_id,
                                "document.documentElement.outerHTML",
                            ).await
                        });
                        match result {
                            Ok(Some(html)) => {
                                // Result comes as JSON string, strip quotes
                                let clean = html
                                    .trim_start_matches('"')
                                    .trim_end_matches('"')
                                    .replace("\\n", "\n")
                                    .replace("\\t", "\t")
                                    .replace("\\\"", "\"")
                                    .replace("\\\\", "\\");
                                if let Ok(mut s) = source_handle.lock() {
                                    *s = devtools::TextState::Loaded(clean);
                                }
                            }
                            Ok(None) => {
                                if let Ok(mut s) = source_handle.lock() {
                                    *s = devtools::TextState::Error(
                                        "Kein Ergebnis".to_string(),
                                    );
                                }
                            }
                            Err(e) => {
                                if let Ok(mut s) = source_handle.lock() {
                                    *s = devtools::TextState::Error(e.to_string());
                                }
                            }
                        }
                    }
                    Err(e) => {
                        if let Ok(mut s) = source_handle.lock() {
                            *s = devtools::TextState::Error(e.to_string());
                        }
                    }
                }
            });
        }
    }

    /// Switch active tab from DevTools tab list and resize viewport accordingly.
    fn handle_switch_to_tab(&mut self, idx: usize) {
        self.active_tab = idx;
        if let Some(tab) = self.tabs.get(idx) {
            self.url_input = tab.url.clone();
            let (w, h) = self.last_viewport_size;
            if w > 0 && h > 0 {
                self.engine.send_resize_viewport(tab.id, w, h);
            }
        }
    }

    /// Run a vision tactic (image or text) in a background thread for DevTools.
    fn handle_run_vision_tactic(&self, tactic: &str) {
        if let Some(tab) = self.tabs.get(self.active_tab) {
            let tab_id = tab.id;
            let frame_buffer = tab.frame_buffer.clone();
            let frame_size = tab.frame_size.clone();
            let engine = self.engine.clone();
            let is_image = self.devtools_shared.state.current_tactic_is_image();

            if is_image {
                self.run_vision_image_tactic(tab_id, frame_buffer, frame_size, engine, tactic);
            } else {
                self.run_vision_text_tactic(tab_id, engine, tactic);
            }
        }
    }

    /// Execute an image-producing vision tactic in a background thread.
    fn run_vision_image_tactic(
        &self,
        tab_id: Uuid,
        frame_buffer: Arc<parking_lot::RwLock<Vec<u8>>>,
        frame_size: Arc<parking_lot::RwLock<(u32, u32)>>,
        engine: Arc<CefBrowserEngine>,
        tactic: &str,
    ) {
        let handle = self.devtools_shared.state.vision_image_handle();
        if let Ok(mut s) = handle.lock() {
            *s = devtools::ImageState::Loading;
        }
        // Invalidate the texture cache so the next Loaded state
        // triggers a fresh PNG decode in render_vision_image.
        if let Ok(mut t) = self.devtools_shared.state.vision_texture.lock() {
            *t = None;
        }
        let tactic = tactic.to_string();
        std::thread::spawn(move || {
            let result = gui_vision::run_vision_image_direct(
                &tactic, tab_id, &frame_buffer, &frame_size, &engine,
            );
            match result {
                Ok(bytes) => {
                    if let Ok(mut s) = handle.lock() {
                        *s = devtools::ImageState::Loaded(bytes);
                    }
                }
                Err(e) => {
                    if let Ok(mut s) = handle.lock() {
                        *s = devtools::ImageState::Error(e);
                    }
                }
            }
        });
    }

    /// Execute a text-producing vision tactic in a background thread.
    fn run_vision_text_tactic(
        &self,
        tab_id: Uuid,
        engine: Arc<CefBrowserEngine>,
        tactic: &str,
    ) {
        let handle = self.devtools_shared.state.vision_text_handle();
        if let Ok(mut s) = handle.lock() {
            *s = devtools::TextState::Loading;
        }
        let tactic = tactic.to_string();
        std::thread::spawn(move || {
            let result = gui_vision::run_vision_text_direct(
                &tactic, tab_id, &engine,
            );
            match result {
                Ok(text) => {
                    if let Ok(mut s) = handle.lock() {
                        *s = devtools::TextState::Loaded(text);
                    }
                }
                Err(e) => {
                    if let Ok(mut s) = handle.lock() {
                        *s = devtools::TextState::Error(e);
                    }
                }
            }
        });
    }

    /// Run selected OCR engines on the current frame buffer in a background thread.
    fn handle_run_ocr(&self, engines: &[String]) {
        if let Some(tab) = self.tabs.get(self.active_tab) {
            let frame_buffer = tab.frame_buffer.clone();
            let frame_size = tab.frame_size.clone();
            let ocr_results = self.devtools_shared.state.ocr_results.clone();
            let ocr_image = self.devtools_shared.state.ocr_image.clone();

            // Clear old results and reset annotated image state.
            if let Ok(mut r) = ocr_results.lock() {
                r.clear();
            }
            if let Ok(mut img) = ocr_image.lock() {
                *img = devtools::ImageState::Loading;
            }
            // Invalidate the OCR texture cache so the next Loaded state
            // triggers a fresh PNG decode in render_vision_image.
            if let Ok(mut t) = self.devtools_shared.state.ocr_texture.lock() {
                *t = None;
            }

            let engines = engines.to_vec();

            // Capture frame buffer and run OCR engines in background thread.
            std::thread::spawn(move || {
                run_ocr_background(frame_buffer, frame_size, ocr_results, ocr_image, &engines);
            });
        }
    }
}

/// Run OCR engines on a captured frame buffer (executed in a background thread).
///
/// Reads the frame buffer as PNG, iterates over selected OCR engines, collects
/// results with bounding-box regions, and draws annotated overlay image.
fn run_ocr_background(
    frame_buffer: Arc<parking_lot::RwLock<Vec<u8>>>,
    frame_size: Arc<parking_lot::RwLock<(u32, u32)>>,
    ocr_results: Arc<std::sync::Mutex<Vec<devtools::OcrDisplayResult>>>,
    ocr_image: Arc<std::sync::Mutex<devtools::ImageState>>,
    engines: &[String],
) {
    // Read frame buffer directly — no API round-trip needed.
    let png_data = match gui_vision::frame_buffer_to_png(&frame_buffer, &frame_size) {
        Ok(data) => data,
        Err(e) => {
            if let Ok(mut r) = ocr_results.lock() {
                r.push(devtools::OcrDisplayResult {
                    engine: "system".to_string(),
                    full_text: String::new(),
                    result_count: 0,
                    duration_ms: 0,
                    error: Some(format!("Frame-Buffer Fehler: {}", e)),
                    regions: vec![],
                });
            }
            if let Ok(mut img) = ocr_image.lock() {
                *img = devtools::ImageState::Error(
                    format!("Frame-Buffer Fehler: {}", e)
                );
            }
            return;
        }
    };

    // Run all selected OCR engines and collect display results.
    let all_engines = crate::ocr::all_engines();
    // Collect all recognised regions from all engines for the annotated image.
    let mut all_regions: Vec<devtools::OcrDisplayRegion> = Vec::new();

    for ocr_engine in all_engines {
        if !engines.contains(&ocr_engine.name().to_string()) {
            continue;
        }
        if !ocr_engine.is_available() {
            if let Ok(mut r) = ocr_results.lock() {
                r.push(devtools::OcrDisplayResult {
                    engine: ocr_engine.name().to_string(),
                    full_text: String::new(),
                    result_count: 0,
                    duration_ms: 0,
                    error: Some("Engine nicht verfuegbar".to_string()),
                    regions: vec![],
                });
            }
            continue;
        }

        match ocr_engine.recognize(&png_data, None) {
            Ok(response) => {
                // Map OCR result regions to display regions,
                // preserving bounding box coordinates for overlay rendering.
                let regions: Vec<devtools::OcrDisplayRegion> = response
                    .results
                    .iter()
                    .map(|region| devtools::OcrDisplayRegion {
                        text: region.text.clone(),
                        confidence: region.confidence,
                        x: region.x,
                        y: region.y,
                        w: region.w,
                        h: region.h,
                    })
                    .collect();
                all_regions.extend(regions.iter().cloned());
                if let Ok(mut r) = ocr_results.lock() {
                    r.push(devtools::OcrDisplayResult {
                        engine: response.engine,
                        full_text: response.full_text,
                        result_count: regions.len(),
                        duration_ms: response.duration_ms,
                        error: None,
                        regions,
                    });
                }
            }
            Err(err) => {
                if let Ok(mut r) = ocr_results.lock() {
                    r.push(devtools::OcrDisplayResult {
                        engine: ocr_engine.name().to_string(),
                        full_text: String::new(),
                        result_count: 0,
                        duration_ms: 0,
                        error: Some(err),
                        regions: vec![],
                    });
                }
            }
        }
    }

    // Draw red bounding boxes on the screenshot for all detected regions.
    // Each detected region is outlined with a red rectangle.
    match gui_vision::draw_ocr_bounding_boxes(&png_data, &all_regions) {
        Ok(annotated_png) => {
            if let Ok(mut img) = ocr_image.lock() {
                *img = devtools::ImageState::Loaded(annotated_png);
            }
        }
        Err(e) => {
            if let Ok(mut img) = ocr_image.lock() {
                *img = devtools::ImageState::Error(
                    format!("Bounding-Box Fehler: {}", e)
                );
            }
        }
    }
}
