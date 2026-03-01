//! Main GUI browser application with CEF rendering and graceful shutdown.
//!
//! Provides the central GUI application loop, single-instance enforcement via
//! PID file, and the eframe integration. Uses `GuiHandle` from the `handle`
//! module for cross-thread shutdown signaling and visibility control.
//! DevTools opens as a separate OS window via `show_viewport_deferred`.

use std::sync::atomic::Ordering;
use std::sync::Arc;

use parking_lot::RwLock;
use tracing::{info, warn};
use uuid::Uuid;

use crate::browser::cef_engine::CefBrowserEngine;
use crate::browser::tab::TabStatus;

use super::context_menu::{self, ContextMenuAction, ContextMenuState};
use super::devtools::{self, DevToolsAction, DevToolsShared};
use super::element_inspector::{self, ElementInspectorState, ElementDetails};
use super::handle::{GuiHandle, GuiVisibility};
use super::tab_bar::{self, TabInfo};
use super::title_bar::{self, TitleBarAction};
use super::toolbar::{self, NavAction};
use super::viewport::{self, ViewportInput, ViewportState};
use super::vision_overlay::{self, VisionOverlayState, VisionMode};
use super::gui_devtools_actions::{self, TabSnapshot};
use super::gui_inspect;
use super::gui_vision;
use super::status_bar;

use std::sync::Mutex;

/// PID file path for single-instance enforcement.
const PID_FILE: &str = "/tmp/ki-browser-gui.pid";

/// Tab state mirrored from the CEF engine for the GUI render loop.
struct GuiTab {
    id: Uuid,
    title: String,
    url: String,
    is_loading: bool,
    frame_buffer: Arc<RwLock<Vec<u8>>>,
    frame_size: Arc<RwLock<(u32, u32)>>,
    can_go_back: bool,
    can_go_forward: bool,
}

/// The main browser application.
pub struct KiBrowserApp {
    /// CEF engine (shared via Arc -- all methods are &self and use internal
    /// channels, so no Mutex needed). The same Arc may be held by the API
    /// server so that both GUI and REST API can drive the browser.
    engine: Arc<CefBrowserEngine>,
    tabs: Vec<GuiTab>,
    active_tab: usize,
    url_input: String,
    viewport: ViewportState,
    api_port: u16,
    first_frame: bool,
    /// Guard: only request initial tab creation once (prevents flooding).
    initial_tab_requested: bool,
    /// Shared handle for cross-thread GUI control and shutdown signaling.
    gui_handle: Arc<GuiHandle>,
    /// Prevents sending duplicate shutdown commands to CEF.
    shutdown_initiated: bool,
    /// Last known viewport pixel size so we only send ResizeViewport on change.
    last_viewport_size: (u32, u32),
    /// Last visibility state to avoid spamming Wayland with repeated commands.
    last_visibility: GuiVisibility,
    /// State for the browser right-click context menu.
    context_menu_state: ContextMenuState,
    /// Shared state for the DevTools OS window (Arc-wrapped for deferred viewport).
    devtools_shared: Arc<DevToolsShared>,
    /// Vision overlay state for Rechtsklick hit-test.
    vision_overlay: VisionOverlayState,
    /// Element, auf das der User rechtsklickte (fuer Element-Inspector).
    inspected_element: Arc<Mutex<Option<vision_overlay::OverlayElement>>>,
    /// Shared state for the Element-Inspector OS window (Arc-wrapped for deferred viewport).
    inspector_state: Arc<ElementInspectorState>,
}

impl KiBrowserApp {
    fn new(
        engine: Arc<CefBrowserEngine>,
        api_port: u16,
        gui_handle: Arc<GuiHandle>,
    ) -> Self {
        Self {
            engine,
            tabs: Vec::new(),
            active_tab: 0,
            url_input: String::new(),
            viewport: ViewportState::new(),
            api_port,
            first_frame: true,
            initial_tab_requested: false,
            gui_handle,
            shutdown_initiated: false,
            last_viewport_size: (0, 0),
            last_visibility: GuiVisibility::Visible,
            context_menu_state: ContextMenuState::default(),
            devtools_shared: Arc::new(DevToolsShared::default()),
            vision_overlay: VisionOverlayState::default(),
            inspected_element: Arc::new(Mutex::new(None)),
            inspector_state: Arc::new(ElementInspectorState::default()),
        }
    }

    fn active_tab(&self) -> Option<&GuiTab> {
        self.tabs.get(self.active_tab)
    }

    fn current_url(&self) -> &str {
        self.active_tab().map(|t| t.url.as_str()).unwrap_or("")
    }

    fn is_loading(&self) -> bool {
        self.active_tab().map(|t| t.is_loading).unwrap_or(false)
    }

    /// Sync GUI tab list from engine state (fully synchronous, no async).
    fn sync_tabs_from_engine(&mut self) {
        let engine_tabs = self.engine.get_tabs_sync();

        // Add new tabs from engine that we don't have
        for et in &engine_tabs {
            if !self.tabs.iter().any(|t| t.id == et.id) {
                let (frame_buffer, frame_size) = self.engine
                    .get_tab_frame_buffer(et.id)
                    .unwrap_or_else(|| {
                        (Arc::new(RwLock::new(Vec::new())), Arc::new(RwLock::new((0, 0))))
                    });
                self.tabs.push(GuiTab {
                    id: et.id,
                    title: et.title.clone(),
                    url: et.url.clone(),
                    is_loading: matches!(et.status, TabStatus::Loading),
                    frame_buffer,
                    frame_size,
                    can_go_back: self.engine.can_go_back(et.id),
                    can_go_forward: self.engine.can_go_forward(et.id),
                });
            }
        }

        // Update existing tabs
        for gt in &mut self.tabs {
            if let Some(et) = engine_tabs.iter().find(|t| t.id == gt.id) {
                gt.title = et.title.clone();
                gt.url = et.url.clone();
                gt.is_loading = matches!(et.status, TabStatus::Loading);
                gt.can_go_back = self.engine.can_go_back(gt.id);
                gt.can_go_forward = self.engine.can_go_forward(gt.id);
            }
        }

        // Remove tabs that no longer exist in engine
        let engine_ids: Vec<Uuid> = engine_tabs.iter().map(|t| t.id).collect();
        self.tabs.retain(|t| engine_ids.contains(&t.id));

        // Fix active tab index
        if self.active_tab >= self.tabs.len() && !self.tabs.is_empty() {
            self.active_tab = self.tabs.len() - 1;
        }
    }

    /// Create a new tab (fire-and-forget, tab appears in next sync).
    fn create_tab(&mut self, url: &str) {
        let tab_id = self.engine.send_create_tab(url);
        info!("GUI: Creating tab {} -> {}", tab_id, url);
    }

    /// Close a tab by index (fire-and-forget). Adjusts active tab index so
    /// the currently viewed tab stays selected after removal.
    fn close_tab(&mut self, index: usize) {
        if let Some(tab) = self.tabs.get(index) {
            let tab_id = tab.id;
            self.engine.send_close_tab(tab_id);

            self.tabs.remove(index);

            if self.tabs.is_empty() {
                self.active_tab = 0;
            } else if index < self.active_tab {
                // Closed tab was before the active one -- shift index left
                self.active_tab -= 1;
            } else if index == self.active_tab {
                // Closed the active tab -- select the nearest remaining tab
                if self.active_tab >= self.tabs.len() {
                    self.active_tab = self.tabs.len() - 1;
                }
            }
            // If index > active_tab, no adjustment needed

            // Update URL bar to reflect the new active tab
            if let Some(tab) = self.tabs.get(self.active_tab) {
                self.url_input = tab.url.clone();
            }
        }
    }

    /// Reorder a tab from one index to another (drag-and-drop).
    fn reorder_tab(&mut self, from: usize, to: usize) {
        if from == to || from >= self.tabs.len() || to >= self.tabs.len() {
            return;
        }
        let tab = self.tabs.remove(from);
        self.tabs.insert(to, tab);

        // Adjust active_tab to follow the moved tab or stay on the same tab
        if self.active_tab == from {
            self.active_tab = to;
        } else if from < self.active_tab && to >= self.active_tab {
            self.active_tab -= 1;
        } else if from > self.active_tab && to <= self.active_tab {
            self.active_tab += 1;
        }
    }

    /// Navigate the active tab (fire-and-forget).
    fn navigate(&mut self, url: &str) {
        if self.tabs.is_empty() {
            self.create_tab(url);
            return;
        }
        if let Some(tab) = self.tabs.get(self.active_tab) {
            let tab_id = tab.id;
            info!("GUI: Navigating tab {} to {}", tab_id, url);
            self.engine.send_navigate(tab_id, url);
        }
    }

    /// Forward viewport input events to CEF (all fire-and-forget, never blocks GUI).
    fn forward_input(&self, inputs: &[ViewportInput]) {
        if inputs.is_empty() {
            return;
        }
        if let Some(tab) = self.tabs.get(self.active_tab) {
            let tab_id = tab.id;

            for input in inputs {
                match input {
                    ViewportInput::MouseMove { x, y } => {
                        self.engine.send_mouse_move(tab_id, *x, *y);
                    }
                    ViewportInput::MouseClick { x, y, button } => {
                        self.engine.send_mouse_click(tab_id, *x, *y, *button);
                    }
                    ViewportInput::MouseWheel { x, y, delta_x, delta_y } => {
                        self.engine.send_mouse_wheel(tab_id, *x, *y, *delta_x, *delta_y);
                    }
                    ViewportInput::KeyDown { key_code, character } => {
                        self.engine.send_key_event(tab_id, 0, 0, *key_code, *character);
                    }
                    ViewportInput::KeyUp { key_code, character } => {
                        self.engine.send_key_event(tab_id, 1, 0, *key_code, *character);
                    }
                    ViewportInput::CharInput { character } => {
                        let c = char::from_u32(*character as u32).unwrap_or('\0');
                        if c != '\0' {
                            self.engine.send_type_text(tab_id, &c.to_string());
                        }
                    }
                }
            }
        }
    }

    /// Initiate graceful shutdown: send CEF shutdown command (which closes all
    /// browsers internally), then let the eframe event loop exit naturally.
    /// Does NOT close tabs individually -- the shutdown handler does that and
    /// pumps the CEF message loop so browsers can finish their close cycle.
    fn initiate_shutdown(&mut self, ctx: &egui::Context) {
        if self.shutdown_initiated {
            return;
        }
        self.shutdown_initiated = true;

        info!("GUI: Initiating graceful shutdown");

        // Close DevTools window if open
        self.devtools_shared.state.open.store(false, Ordering::Relaxed);

        // Close Element-Inspector window if open
        self.inspector_state.open.store(false, Ordering::Relaxed);

        // Clear our GUI-side tab list (prevents further rendering/interaction)
        self.tabs.clear();

        // Tell CEF to close all browsers and shut down (handled on the CEF thread)
        self.engine.send_shutdown();

        // Tell eframe to close the viewport (exits the event loop)
        ctx.send_viewport_cmd(egui::ViewportCommand::Close);
    }

    /// Update shared DevTools page info and tab list from current GUI state.
    ///
    /// Delegates to `gui_devtools_actions::update_devtools_shared_state` which
    /// operates on plain `TabSnapshot` values so it does not need `&self`.
    fn update_devtools_shared_state(&self) {
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
    fn handle_devtools_actions(&mut self) {
        let actions: Vec<DevToolsAction> = self.devtools_shared.state.actions
            .lock()
            .map(|mut a| a.drain(..).collect())
            .unwrap_or_default();

        for dt_action in actions {
            match dt_action {
                DevToolsAction::LoadSource(_) => {
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
                DevToolsAction::SwitchToTab(idx) => {
                    self.active_tab = idx;
                    if let Some(tab) = self.tabs.get(idx) {
                        self.url_input = tab.url.clone();
                        let (w, h) = self.last_viewport_size;
                        if w > 0 && h > 0 {
                            self.engine.send_resize_viewport(tab.id, w, h);
                        }
                    }
                }
                DevToolsAction::CloseTab(idx) => {
                    self.close_tab(idx);
                }
                DevToolsAction::RunVisionTactic { tactic, .. } => {
                    if let Some(tab) = self.tabs.get(self.active_tab) {
                        let tab_id = tab.id;
                        let frame_buffer = tab.frame_buffer.clone();
                        let frame_size = tab.frame_size.clone();
                        let engine = self.engine.clone();
                        let is_image = self.devtools_shared.state.current_tactic_is_image();

                        if is_image {
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
                        } else {
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
                    }
                }
                DevToolsAction::RunOcr { engines, .. } => {
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

                        // Capture frame buffer and run OCR engines in background thread.
                        std::thread::spawn(move || {
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
                        });
                    }
                }
            }
        }
    }
}

impl eframe::App for KiBrowserApp {
    fn on_exit(&mut self, _gl: Option<&eframe::glow::Context>) {
        // Safety net: ensure CEF shutdown was sent even if update() missed it.
        if !self.shutdown_initiated {
            info!("on_exit: sending belated CEF shutdown");
            self.engine.send_shutdown();
        }
        // Clean up PID file and signal completion to callers.
        let _ = std::fs::remove_file(PID_FILE);
        self.gui_handle.mark_shutdown_complete();
        info!("GUI: on_exit complete, event loop will return");
    }

    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        // Store egui context in the shared handle so API threads can
        // trigger repaints when they change visibility or request shutdown.
        // CRITICAL: set_embed_viewports(false) so show_viewport_deferred
        // creates real OS windows instead of embedded egui panels.
        if self.first_frame {
            self.first_frame = false;
            ctx.set_embed_viewports(false);
            self.gui_handle.set_egui_ctx(ctx.clone());
        }

        // --- Shutdown: close button or external request (SIGTERM, API) ---
        let close_requested = ctx.input(|i| i.viewport().close_requested());
        let external_shutdown = self.gui_handle.is_shutdown_requested();

        if close_requested || external_shutdown {
            self.initiate_shutdown(ctx);
            return;
        }

        // --- Visibility: hide/show the window via API toggle (only on change) ---
        let visibility = self.gui_handle.visibility();
        if visibility != self.last_visibility {
            self.last_visibility = visibility;
            match visibility {
                GuiVisibility::Hidden => {
                    ctx.send_viewport_cmd(egui::ViewportCommand::Minimized(true));
                }
                GuiVisibility::Visible => {
                    ctx.send_viewport_cmd(egui::ViewportCommand::Minimized(false));
                    ctx.send_viewport_cmd(egui::ViewportCommand::Focus);
                }
                GuiVisibility::Disabled => {}
            }
        }

        // Auto-create first tab on startup (only once!)
        if self.tabs.is_empty() && !self.initial_tab_requested {
            self.initial_tab_requested = true;
            self.create_tab("https://example.com");
        }

        // Sync tab state from engine
        self.sync_tabs_from_engine();

        // Update viewport texture from active tab's frame buffer
        if let Some(tab) = self.tabs.get(self.active_tab) {
            let tab_id = tab.id;
            let fb = tab.frame_buffer.clone();
            let fs = tab.frame_size.clone();
            self.viewport.update_from_frame_buffer(ctx, &fb, &fs, tab_id);
        }

        // Dark theme
        ctx.set_visuals(egui::Visuals::dark());

        // Custom title bar (replaces OS window decorations)
        egui::TopBottomPanel::top("title_bar").show(ctx, |ui| {
            if let Some(action) = title_bar::render(ui, "KI-Browser") {
                match action {
                    TitleBarAction::Minimize => {
                        ctx.send_viewport_cmd(egui::ViewportCommand::Minimized(true));
                    }
                    TitleBarAction::Maximize => {
                        let is_maximized = ctx.input(|i| {
                            i.viewport().maximized.unwrap_or(false)
                        });
                        ctx.send_viewport_cmd(egui::ViewportCommand::Maximized(!is_maximized));
                    }
                    TitleBarAction::Close => {
                        self.initiate_shutdown(ctx);
                        return;
                    }
                }
            }
        });

        // Tab bar
        let tab_infos: Vec<TabInfo> = self.tabs.iter().map(|t| TabInfo {
            id: t.id,
            title: t.title.clone(),
            is_loading: t.is_loading,
        }).collect();

        egui::TopBottomPanel::top("tab_bar").show(ctx, |ui| {
            let tab_action = tab_bar::render(ui, &tab_infos, self.active_tab);

            if let Some(idx) = tab_action.selected {
                self.active_tab = idx;
                if let Some(tab) = self.tabs.get(idx) {
                    self.url_input = tab.url.clone();
                    // Sync viewport size to the newly active tab so it renders
                    // at the current window dimensions (not whatever size it had before).
                    let (w, h) = self.last_viewport_size;
                    if w > 0 && h > 0 {
                        self.engine.send_resize_viewport(tab.id, w, h);
                    }
                }
            }

            if let Some(idx) = tab_action.close {
                self.close_tab(idx);
            }

            if tab_action.new_tab {
                self.create_tab("about:blank");
            }

            if let Some((from, to)) = tab_action.reorder {
                self.reorder_tab(from, to);
            }
        });

        // Toolbar -- read history navigation state from the active tab
        let can_back = self.active_tab().map(|t| t.can_go_back).unwrap_or(false);
        let can_fwd = self.active_tab().map(|t| t.can_go_forward).unwrap_or(false);

        egui::TopBottomPanel::top("toolbar").show(ctx, |ui| {
            if let Some(action) = toolbar::render(ui, &mut self.url_input, can_back, can_fwd) {
                match action {
                    NavAction::Navigate(url) => {
                        self.navigate(&url);
                        self.url_input = url;
                    }
                    NavAction::Reload => {
                        let url = self.current_url().to_string();
                        if !url.is_empty() {
                            self.navigate(&url);
                        }
                    }
                    NavAction::Back => {
                        if let Some(tab) = self.tabs.get(self.active_tab) {
                            self.engine.send_go_back(tab.id);
                        }
                    }
                    NavAction::Forward => {
                        if let Some(tab) = self.tabs.get(self.active_tab) {
                            self.engine.send_go_forward(tab.id);
                        }
                    }
                }
            }
        });

        // Status bar
        let current_url = self.current_url().to_string();
        let tab_count = self.tabs.len();
        let is_loading = self.is_loading();
        let api_port = self.api_port;

        egui::TopBottomPanel::bottom("status_bar").show(ctx, |ui| {
            status_bar::render(ui, &current_url, tab_count, api_port, is_loading);
        });

        // Viewport (central panel) -- also detect resize and right-click
        let mut viewport_rect = egui::Rect::NOTHING;
        egui::CentralPanel::default().show(ctx, |ui| {
            viewport_rect = ui.available_rect_before_wrap();
            let inputs = viewport::render(ui, &mut self.viewport);
            self.forward_input(&inputs);

            // Detect right-click in viewport to open context menu
            let right_clicked = ui.input(|i| {
                i.pointer.button_clicked(egui::PointerButton::Secondary)
            });
            if right_clicked {
                if let Some(pos) = ui.input(|i| i.pointer.hover_pos()) {
                    if viewport_rect.contains(pos) {
                        // Vision Hit-Test — prüfe ob ein Overlay-Element getroffen wurde
                        let hit = vision_overlay::hit_test(
                            &self.vision_overlay, pos, viewport_rect, 1.0,
                        );
                        if let Some(elem) = &hit {
                            if let Ok(mut ie) = self.inspected_element.lock() {
                                *ie = Some(elem.clone());
                            }
                        }
                        self.context_menu_state.position = Some(pos);
                        self.context_menu_state.open = true;
                    }
                }
            }
        });

        // Context menu overlay (rendered on top of everything)
        let can_back = self.active_tab().map(|t| t.can_go_back).unwrap_or(false);
        let can_fwd = self.active_tab().map(|t| t.can_go_forward).unwrap_or(false);
        let vision_active = self.vision_overlay.mode != VisionMode::Off;
        if let Some(action) = context_menu::render(
            ctx, &mut self.context_menu_state, can_back, can_fwd, vision_active,
        ) {
            match action {
                ContextMenuAction::InspectElement => {
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
                                });
                            }
                        }
                    }
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

        // --- DevTools as separate OS window via show_viewport_deferred ---
        // Update shared page info + tabs each frame so the DevTools window
        // always has current data.
        self.update_devtools_shared_state();

        // Show deferred viewport if DevTools is open
        if self.devtools_shared.state.open.load(Ordering::Relaxed) {
            let shared = Arc::clone(&self.devtools_shared);
            ctx.show_viewport_deferred(
                egui::ViewportId::from_hash_of("devtools_window"),
                egui::ViewportBuilder::default()
                    .with_title("KI-Browser DevTools")
                    .with_inner_size([700.0, 550.0])
                    .with_min_inner_size([450.0, 350.0]),
                move |ctx, _class| {
                    devtools::render_standalone(ctx, &shared);
                },
            );
        }

        // Drain queued DevTools actions and handle them
        self.handle_devtools_actions();

        // Notify CEF when the viewport pixel size changes so it re-renders
        // at the correct resolution (e.g. after the user resizes the window).
        if viewport_rect.width() > 1.0 && viewport_rect.height() > 1.0 {
            let new_w = viewport_rect.width() as u32;
            let new_h = viewport_rect.height() as u32;
            if (new_w, new_h) != self.last_viewport_size {
                self.last_viewport_size = (new_w, new_h);
                if let Some(tab) = self.tabs.get(self.active_tab) {
                    self.engine.send_resize_viewport(tab.id, new_w, new_h);
                }
            }
        }

        // --- Element-Inspector as separate OS window via show_viewport_deferred ---
        if self.inspector_state.open.load(Ordering::Relaxed) {
            let state = Arc::clone(&self.inspector_state);
            ctx.show_viewport_deferred(
                egui::ViewportId::from_hash_of("element_inspector"),
                egui::ViewportBuilder::default()
                    .with_title("Element-Details")
                    .with_inner_size([400.0, 500.0])
                    .with_min_inner_size([300.0, 200.0]),
                move |ctx, _class| {
                    element_inspector::render_standalone(ctx, &state);
                },
            );
        }

        // Request repainting at ~60fps (not unlimited)
        ctx.request_repaint_after(std::time::Duration::from_millis(16));
    }
}

/// Check if another GUI instance is already running via PID file.
fn acquire_instance_lock() -> anyhow::Result<()> {
    use std::io::Read;

    if let Ok(mut f) = std::fs::File::open(PID_FILE) {
        let mut contents = String::new();
        if f.read_to_string(&mut contents).is_ok() {
            if let Ok(pid) = contents.trim().parse::<u32>() {
                let proc_path = format!("/proc/{}", pid);
                if std::path::Path::new(&proc_path).exists() {
                    return Err(anyhow::anyhow!(
                        "Another KI-Browser GUI instance is already running (PID {}). \
                         Kill it first or remove {}",
                        pid, PID_FILE
                    ));
                }
            }
        }
        warn!("Removing stale PID file");
        let _ = std::fs::remove_file(PID_FILE);
    }

    std::fs::write(PID_FILE, std::process::id().to_string())
        .map_err(|e| anyhow::anyhow!("Failed to write PID file: {}", e))?;

    Ok(())
}

/// Execute a JavaScript snippet on a tab without waiting for the result.
///
/// Spawns a background thread with a short-lived tokio runtime because the
/// engine's `execute_js` method is async but the GUI render loop is sync.
fn fire_and_forget_js(engine: &Arc<CefBrowserEngine>, tab_id: Uuid, script: &str) {
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

/// Starts the GUI browser. MUST be called from the main thread (X11/Wayland requirement).
/// Blocks until the GUI window is closed or a shutdown is requested.
///
/// The `gui_handle` parameter is created by `GuiHandle::new()` and should be
/// shared with the API server *before* calling this function so that REST
/// endpoints can control visibility and request shutdown.
pub fn run_gui(
    engine: Arc<CefBrowserEngine>,
    api_port: u16,
    gui_handle: Arc<GuiHandle>,
) -> anyhow::Result<()> {
    acquire_instance_lock()?;

    info!("Starting GUI browser window");

    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_title("KI-Browser")
            .with_inner_size([1280.0, 800.0])
            .with_min_inner_size([800.0, 600.0])
            .with_decorations(false),
        ..Default::default()
    };

    let app = KiBrowserApp::new(engine, api_port, gui_handle.clone());

    let result = eframe::run_native(
        "KI-Browser",
        options,
        Box::new(|_cc| Ok(Box::new(app))),
    ).map_err(|e| anyhow::anyhow!("GUI error: {}", e));

    // Ensure cleanup even if on_exit was not called (e.g. panic)
    let _ = std::fs::remove_file(PID_FILE);
    gui_handle.mark_shutdown_complete();

    result
}
