//! Tab lifecycle management and CEF input forwarding for the GUI browser.
//!
//! Contains tab synchronisation from the CEF engine state, tab creation,
//! closing, reordering, URL navigation, and viewport input event forwarding
//! (mouse, keyboard, scroll). Extracted from `browser_app.rs` to keep the
//! main application file focused on the eframe integration and render loop.

use std::sync::Arc;

use parking_lot::RwLock;
use tracing::info;
use uuid::Uuid;

use crate::browser::tab::TabStatus;

use super::browser_app::KiBrowserApp;
use super::viewport::ViewportInput;

/// Tab state mirrored from the CEF engine for the GUI render loop.
pub(super) struct GuiTab {
    pub id: Uuid,
    pub title: String,
    pub url: String,
    pub is_loading: bool,
    pub frame_buffer: Arc<RwLock<Vec<u8>>>,
    pub frame_size: Arc<RwLock<(u32, u32)>>,
    pub can_go_back: bool,
    pub can_go_forward: bool,
}

impl KiBrowserApp {
    /// Sync GUI tab list from engine state (fully synchronous, no async).
    pub(super) fn sync_tabs_from_engine(&mut self) {
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
    pub(super) fn create_tab(&mut self, url: &str) {
        let tab_id = self.engine.send_create_tab(url);
        info!("GUI: Creating tab {} -> {}", tab_id, url);
    }

    /// Close a tab by index (fire-and-forget). Adjusts active tab index so
    /// the currently viewed tab stays selected after removal.
    pub(super) fn close_tab(&mut self, index: usize) {
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
    pub(super) fn reorder_tab(&mut self, from: usize, to: usize) {
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
    pub(super) fn navigate(&mut self, url: &str) {
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
    pub(super) fn forward_input(&self, inputs: &[ViewportInput]) {
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
}
