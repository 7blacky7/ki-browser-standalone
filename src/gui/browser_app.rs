//! Main GUI browser application with CEF rendering and graceful shutdown.
//!
//! Provides the central GUI application loop, single-instance enforcement via
//! PID file, and the eframe integration. Uses `GuiHandle` from the `handle`
//! module for cross-thread shutdown signaling and visibility control.
//! DevTools opens as a separate OS window via `show_viewport_deferred`.
//!
//! Tab management, DevTools action handling, context menu actions, and
//! application utilities (PID lock, launcher) are extracted into sibling
//! modules: `gui_tab_management`, `gui_devtools_handler`,
//! `gui_context_actions`, and `gui_app_utils`.

use std::sync::atomic::Ordering;
use std::sync::Arc;

use tracing::info;

use crate::browser::cef_engine::CefBrowserEngine;

use super::context_menu::{self, ContextMenuState};
use super::devtools::{self, DevToolsShared};
use super::element_inspector::{self, ElementInspectorState};
use super::gui_app_utils;
use super::gui_tab_management::GuiTab;
use super::handle::{GuiHandle, GuiVisibility};
use super::tab_bar::{self, TabInfo};
use super::title_bar::{self, TitleBarAction};
use super::toolbar::{self, NavAction};
use super::viewport::{self, ViewportState};
use super::vision_overlay::{self, VisionOverlayState, VisionMode};
use super::status_bar;

use std::sync::Mutex;

/// The main browser application.
pub struct KiBrowserApp {
    /// CEF engine (shared via Arc -- all methods are &self and use internal
    /// channels, so no Mutex needed). The same Arc may be held by the API
    /// server so that both GUI and REST API can drive the browser.
    pub(super) engine: Arc<CefBrowserEngine>,
    pub(super) tabs: Vec<GuiTab>,
    pub(super) active_tab: usize,
    pub(super) url_input: String,
    pub(super) viewport: ViewportState,
    pub(super) api_port: u16,
    first_frame: bool,
    /// Guard: only request initial tab creation once (prevents flooding).
    pub(super) initial_tab_requested: bool,
    /// Shared handle for cross-thread GUI control and shutdown signaling.
    pub(super) gui_handle: Arc<GuiHandle>,
    /// Prevents sending duplicate shutdown commands to CEF.
    shutdown_initiated: bool,
    /// Last known viewport pixel size so we only send ResizeViewport on change.
    pub(super) last_viewport_size: (u32, u32),
    /// Last visibility state to avoid spamming Wayland with repeated commands.
    last_visibility: GuiVisibility,
    /// State for the browser right-click context menu.
    context_menu_state: ContextMenuState,
    /// Shared state for the DevTools OS window (Arc-wrapped for deferred viewport).
    pub(super) devtools_shared: Arc<DevToolsShared>,
    /// Vision overlay state for Rechtsklick hit-test.
    vision_overlay: VisionOverlayState,
    /// Element, auf das der User rechtsklickte (fuer Element-Inspector).
    pub(super) inspected_element: Arc<Mutex<Option<vision_overlay::OverlayElement>>>,
    /// Shared state for the Element-Inspector OS window (Arc-wrapped for deferred viewport).
    pub(super) inspector_state: Arc<ElementInspectorState>,
}

impl KiBrowserApp {
    /// Create a new browser application instance with the given CEF engine,
    /// API port, and shared GUI handle for cross-thread control.
    pub(super) fn new(
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

    /// Get the currently active tab, if any.
    pub(super) fn active_tab(&self) -> Option<&GuiTab> {
        self.tabs.get(self.active_tab)
    }

    /// URL of the currently active tab (empty string if no tab exists).
    pub(super) fn current_url(&self) -> &str {
        self.active_tab().map(|t| t.url.as_str()).unwrap_or("")
    }

    /// Whether the currently active tab is loading.
    fn is_loading(&self) -> bool {
        self.active_tab().map(|t| t.is_loading).unwrap_or(false)
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
}

impl eframe::App for KiBrowserApp {
    fn on_exit(&mut self) {
        // Safety net: ensure CEF shutdown was sent even if update() missed it.
        if !self.shutdown_initiated {
            info!("on_exit: sending belated CEF shutdown");
            self.engine.send_shutdown();
        }
        // Clean up PID file and signal completion to callers.
        gui_app_utils::remove_pid_file();
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
                        // Vision Hit-Test — check if an overlay element was hit
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
            self.handle_context_menu_action(action);
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

/// Re-export the public entry point from `gui_app_utils`.
pub use super::gui_app_utils::run_gui;
