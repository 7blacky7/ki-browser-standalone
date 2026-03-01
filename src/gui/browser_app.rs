//! Main GUI browser application with CEF rendering and graceful shutdown.
//!
//! Provides the central GUI application loop, single-instance enforcement via
//! PID file, and the eframe integration. Uses `GuiHandle` from the `handle`
//! module for cross-thread shutdown signaling and visibility control.

use std::sync::Arc;
use parking_lot::RwLock;
use uuid::Uuid;
use tracing::{info, warn};

use crate::browser::cef_engine::CefBrowserEngine;
use crate::browser::tab::TabStatus;

use super::handle::{GuiHandle, GuiVisibility};
use super::tab_bar::{self, TabInfo};
use super::toolbar::{self, NavAction};
use super::viewport::{self, ViewportState, ViewportInput};
use super::status_bar;

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

    /// Close a tab by index (fire-and-forget).
    fn close_tab(&mut self, index: usize) {
        if let Some(tab) = self.tabs.get(index) {
            let tab_id = tab.id;
            self.engine.send_close_tab(tab_id);

            self.tabs.remove(index);
            if self.active_tab >= self.tabs.len() && !self.tabs.is_empty() {
                self.active_tab = self.tabs.len() - 1;
            }
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

    /// Initiate graceful shutdown: close all tabs, send CEF shutdown command,
    /// and let the eframe event loop exit naturally (no process::exit!).
    fn initiate_shutdown(&mut self, ctx: &egui::Context) {
        if self.shutdown_initiated {
            return;
        }
        self.shutdown_initiated = true;

        info!("GUI: Initiating graceful shutdown");

        // Close all browser tabs
        for tab in &self.tabs {
            self.engine.send_close_tab(tab.id);
        }
        self.tabs.clear();

        // Tell CEF message loop to exit
        self.engine.send_shutdown();

        // Tell eframe to close the viewport (exits the event loop)
        ctx.send_viewport_cmd(egui::ViewportCommand::Close);
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
        if self.first_frame {
            self.first_frame = false;
            ctx.set_embed_viewports(true);
            self.gui_handle.set_egui_ctx(ctx.clone());
        }

        // --- Shutdown: close button or external request (SIGTERM, API) ---
        let close_requested = ctx.input(|i| i.viewport().close_requested());
        let external_shutdown = self.gui_handle.is_shutdown_requested();

        if close_requested || external_shutdown {
            self.initiate_shutdown(ctx);
            return;
        }

        // --- Visibility: hide/show the window via API toggle ---
        let visibility = self.gui_handle.visibility();
        match visibility {
            GuiVisibility::Hidden => {
                ctx.send_viewport_cmd(egui::ViewportCommand::Minimized(true));
            }
            GuiVisibility::Visible => {
                ctx.send_viewport_cmd(egui::ViewportCommand::Minimized(false));
                ctx.send_viewport_cmd(egui::ViewportCommand::Focus);
            }
            GuiVisibility::Disabled => {
                // Should not happen in a running GUI, but ignore gracefully
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
            let fb = tab.frame_buffer.clone();
            let fs = tab.frame_size.clone();
            self.viewport.update_from_frame_buffer(ctx, &fb, &fs);
        }

        // Dark theme
        ctx.set_visuals(egui::Visuals::dark());

        // Tab bar
        let tab_infos: Vec<TabInfo> = self.tabs.iter().map(|t| TabInfo {
            id: t.id,
            title: t.title.clone(),
            is_loading: t.is_loading,
        }).collect();

        egui::TopBottomPanel::top("tab_bar").show(ctx, |ui| {
            let (selected, close, new_tab) = tab_bar::render(ui, &tab_infos, self.active_tab);

            if let Some(idx) = selected {
                self.active_tab = idx;
                if let Some(tab) = self.tabs.get(idx) {
                    self.url_input = tab.url.clone();
                }
            }

            if let Some(idx) = close {
                self.close_tab(idx);
            }

            if new_tab {
                self.create_tab("about:blank");
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

        // Viewport (central panel) -- also detect resize
        let mut viewport_rect = egui::Rect::NOTHING;
        egui::CentralPanel::default().show(ctx, |ui| {
            viewport_rect = ui.available_rect_before_wrap();
            let inputs = viewport::render(ui, &mut self.viewport);
            self.forward_input(&inputs);
        });

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
            .with_min_inner_size([800.0, 600.0]),
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
