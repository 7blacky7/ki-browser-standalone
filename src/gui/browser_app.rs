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
use super::devtools::{self, DevToolsAction, DevToolsShared, DevToolsTabInfo, PageInfo};
use super::element_inspector::{self, ElementInspectorState, ElementDetails};
use super::handle::{GuiHandle, GuiVisibility};
use super::tab_bar::{self, TabInfo};
use super::title_bar::{self, TitleBarAction};
use super::toolbar::{self, NavAction};
use super::viewport::{self, ViewportInput, ViewportState};
use super::vision_overlay::{self, VisionOverlayState, VisionMode};
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
    fn update_devtools_shared_state(&self) {
        if let Ok(mut pi) = self.devtools_shared.page_info.lock() {
            *pi = PageInfo {
                title: self.active_tab().map(|t| t.title.clone()).unwrap_or_default(),
                url: self.current_url().to_string(),
                is_loading: self.is_loading(),
                can_go_back: self.active_tab().map(|t| t.can_go_back).unwrap_or(false),
                can_go_forward: self.active_tab().map(|t| t.can_go_forward).unwrap_or(false),
                api_port: self.api_port,
                tab_count: self.tabs.len(),
            };
        }
        if let Ok(mut tabs) = self.devtools_shared.tabs.lock() {
            *tabs = self.tabs.iter().enumerate().map(|(i, t)| {
                DevToolsTabInfo {
                    id: t.id,
                    title: t.title.clone(),
                    url: t.url.clone(),
                    is_loading: t.is_loading,
                    is_active: i == self.active_tab,
                }
            }).collect();
        }
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
                            let tactic = tactic.to_string();
                            std::thread::spawn(move || {
                                let result = run_vision_image_direct(
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
                                let result = run_vision_text_direct(
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

                        // Capture frame buffer and run OCR engines in background thread.
                        std::thread::spawn(move || {
                            // Read frame buffer directly — no API round-trip needed.
                            let png_data = match frame_buffer_to_png(&frame_buffer, &frame_size) {
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
                            match draw_ocr_bounding_boxes(&png_data, &all_regions) {
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
                            // 1. Sofort Basisdaten aus dem Overlay-Element setzen
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
                            *self.inspector_state.element.lock().unwrap() = Some(details);
                            self.inspector_state.open.store(true, Ordering::Relaxed);

                            // 2. Im Hintergrund per JS detaillierte Infos abrufen
                            let engine = self.engine.clone();
                            let inspector = Arc::clone(&self.inspector_state);
                            let tab_id = self.tabs.get(self.active_tab).map(|t| t.id);
                            let elem_x = elem.x as f64;
                            let elem_y = elem.y as f64;

                            if let Some(tab_id) = tab_id {
                                std::thread::spawn(move || {
                                    let js = format!(r##"
                                        (function() {{
                                            var el = document.elementFromPoint({x}, {y});
                                            if (!el) return JSON.stringify({{error: "no element"}});
                                            function getXPath(el) {{
                                                if (!el.parentNode) return "";
                                                var siblings = el.parentNode.children;
                                                var tag = el.tagName.toLowerCase();
                                                var idx = Array.from(siblings).filter(function(s) {{ return s.tagName === el.tagName; }}).indexOf(el) + 1;
                                                return getXPath(el.parentNode) + "/" + tag + (idx > 1 ? "[" + idx + "]" : "");
                                            }}
                                            function getFullXPath(el) {{
                                                if (!el.parentNode) return "";
                                                var siblings = el.parentNode.children;
                                                var tag = el.tagName.toLowerCase();
                                                var idx = Array.from(siblings).filter(function(s) {{ return s.tagName === el.tagName; }}).indexOf(el) + 1;
                                                return getFullXPath(el.parentNode) + "/" + tag + "[" + idx + "]";
                                            }}
                                            function getCssSelector(el) {{
                                                if (el.id) return "#" + el.id;
                                                var path = [];
                                                while (el && el.nodeType === 1) {{
                                                    var sel = el.tagName.toLowerCase();
                                                    if (el.id) {{ path.unshift("#" + el.id); break; }}
                                                    var sib = el, nth = 1;
                                                    while (sib = sib.previousElementSibling) {{ if (sib.tagName === el.tagName) nth++; }}
                                                    if (nth > 1) sel += ":nth-of-type(" + nth + ")";
                                                    path.unshift(sel);
                                                    el = el.parentNode;
                                                }}
                                                return path.join(" > ");
                                            }}
                                            var rect = el.getBoundingClientRect();
                                            return JSON.stringify({{
                                                tag: el.tagName.toLowerCase(),
                                                type: el.type || el.tagName.toLowerCase(),
                                                title: el.title || "",
                                                text: (el.innerText || el.value || "").substring(0, 200),
                                                xpath: getXPath(el),
                                                fullXpath: getFullXPath(el),
                                                role: el.getAttribute("role") || "",
                                                id: el.id || "",
                                                classes: el.className || "",
                                                href: el.href || "",
                                                src: el.src || "",
                                                placeholder: el.placeholder || "",
                                                cssSelector: getCssSelector(el),
                                                visible: rect.width > 0 && rect.height > 0,
                                                interactive: el.matches("a,button,input,select,textarea,[tabindex],[onclick]"),
                                                x: rect.x, y: rect.y, w: rect.width, h: rect.height
                                            }});
                                        }})()
                                    "##, x = elem_x, y = elem_y);

                                    // Run a small tokio runtime to call the async execute_js_with_result
                                    let rt = tokio::runtime::Builder::new_current_thread()
                                        .enable_all()
                                        .build();
                                    if let Ok(rt) = rt {
                                        match rt.block_on(engine.execute_js_with_result(tab_id, &js)) {
                                            Ok(Some(result)) => {
                                                // The result might be JSON-escaped by CEF, try to parse it
                                                let json_str = result.trim_matches('"');
                                                let json_str = json_str.replace("\\\"", "\"");
                                                let json_str = json_str.replace("\\\\", "\\");
                                                if let Ok(val) = serde_json::from_str::<serde_json::Value>(&json_str) {
                                                    if val.get("error").is_none() {
                                                        let details = ElementDetails {
                                                            tag: val["tag"].as_str().unwrap_or("").to_string(),
                                                            element_type: val["type"].as_str().unwrap_or("").to_string(),
                                                            title: val["title"].as_str().unwrap_or("").to_string(),
                                                            text_value: val["text"].as_str().unwrap_or("").to_string(),
                                                            xpath: val["xpath"].as_str().unwrap_or("").to_string(),
                                                            full_xpath: val["fullXpath"].as_str().unwrap_or("").to_string(),
                                                            role: val["role"].as_str().unwrap_or("").to_string(),
                                                            id: val["id"].as_str().unwrap_or("").to_string(),
                                                            classes: val["classes"].as_str().unwrap_or("").to_string(),
                                                            href: val["href"].as_str().unwrap_or("").to_string(),
                                                            src: val["src"].as_str().unwrap_or("").to_string(),
                                                            placeholder: val["placeholder"].as_str().unwrap_or("").to_string(),
                                                            css_selector: val["cssSelector"].as_str().unwrap_or("").to_string(),
                                                            is_visible: Some(val["visible"].as_bool().unwrap_or(true)),
                                                            is_interactive: Some(val["interactive"].as_bool().unwrap_or(false)),
                                                            x: val["x"].as_f64().unwrap_or(0.0) as f32,
                                                            y: val["y"].as_f64().unwrap_or(0.0) as f32,
                                                            w: val["w"].as_f64().unwrap_or(0.0) as f32,
                                                            h: val["h"].as_f64().unwrap_or(0.0) as f32,
                                                            ..Default::default()
                                                        };
                                                        *inspector.element.lock().unwrap() = Some(details);
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

// --- Vision tactic direct engine helpers (called from background threads) ---

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
fn execute_js_blocking(
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
fn capture_dom_snapshot(
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
fn run_vision_image_direct(
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
        other => Err(format!("Unknown image tactic: {}", other))
    }
}

/// Runs a vision text tactic (labels/dom_snapshot/etc.) directly using
/// the CEF engine JavaScript execution. No REST API needed.
fn run_vision_text_direct(
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
fn frame_buffer_to_png(
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
fn draw_ocr_bounding_boxes(
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
