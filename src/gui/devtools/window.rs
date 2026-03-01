//! Main DevTools standalone window renderer.
//!
//! Orchestrates the different sections (PageInfo, Source, KiVision, Tabs)
//! into a dark-themed deferred OS window. Called by the closure passed to
//! `ctx.show_viewport_deferred()` in the main browser application.

use std::sync::atomic::Ordering;

use egui::{Color32, RichText};

use super::render_page_info::render_page_info;
use super::render_source::render_source_view;
use super::render_tabs::render_tabs;
use super::render_vision::render_ki_vision;
use super::state::DevToolsShared;
use super::types::{Section, VisionTactic};

/// Renders the DevTools UI inside a deferred viewport (separate OS window).
///
/// Called by the closure passed to `ctx.show_viewport_deferred()`. Uses
/// `egui::CentralPanel` instead of `egui::Window` because this IS the window.
/// Handles the OS close button, dispatches to section renderers, and writes
/// changed section/tactic state back into the shared Arc-Mutex fields.
pub fn render_standalone(ctx: &egui::Context, shared: &DevToolsShared) {
    // Handle window close request (user clicks X on the OS window)
    if ctx.input(|i| i.viewport().close_requested()) {
        shared.state.open.store(false, Ordering::Relaxed);
        ctx.send_viewport_cmd(egui::ViewportCommand::Close);
        return;
    }

    // Read shared state (clone to release locks quickly)
    let page_info = shared.page_info.lock()
        .map(|pi| pi.clone())
        .unwrap_or_default();
    let tabs = shared.tabs.lock()
        .map(|t| t.clone())
        .unwrap_or_default();
    let mut section = shared.state.section.lock()
        .map(|s| *s)
        .unwrap_or(Section::PageInfo);
    let mut vision_tactic = shared.state.vision_tactic.lock()
        .map(|t| *t)
        .unwrap_or(VisionTactic::Annotated);

    let source = shared.state.source.clone();
    let vision_text = shared.state.vision_text.clone();
    let vision_image = shared.state.vision_image.clone();
    let actions = shared.state.actions.clone();
    let vision_texture = shared.state.vision_texture.clone();
    let ocr_config = shared.state.ocr_config.clone();
    let ocr_results = shared.state.ocr_results.clone();
    let ocr_image = shared.state.ocr_image.clone();
    let ocr_texture = shared.state.ocr_texture.clone();

    // Dark theme for the standalone window
    ctx.set_visuals(egui::Visuals::dark());

    egui::CentralPanel::default().show(ctx, |ui| {
        // Section tab bar
        ui.horizontal(|ui| {
            let btn = |ui: &mut egui::Ui, label: &str, s: Section, current: &mut Section| {
                let active = *current == s;
                let text = if active {
                    RichText::new(label).color(Color32::WHITE).strong()
                } else {
                    RichText::new(label).color(Color32::GRAY)
                };
                if ui.selectable_label(active, text).clicked() {
                    *current = s;
                }
            };
            btn(ui, "Seiteninfo", Section::PageInfo, &mut section);
            ui.separator();
            btn(ui, "Quelltext", Section::Source, &mut section);
            ui.separator();
            btn(ui, "KI-Vision", Section::KiVision, &mut section);
            ui.separator();
            btn(ui, "Tabs", Section::Tabs, &mut section);
        });
        ui.separator();

        match section {
            Section::PageInfo => {
                render_page_info(ui, &page_info);
            }
            Section::Source => {
                if let Some(action) = render_source_view(ui, &source, &page_info) {
                    if let Ok(mut a) = actions.lock() {
                        a.push(action);
                    }
                }
            }
            Section::KiVision => {
                if let Some(action) = render_ki_vision(
                    ui, ctx, &mut vision_tactic, &vision_text, &vision_image,
                    &vision_texture, &page_info, &actions, &ocr_config, &ocr_results,
                    &ocr_image, &ocr_texture,
                ) {
                    if let Ok(mut a) = actions.lock() {
                        a.push(action);
                    }
                }
            }
            Section::Tabs => {
                let tab_actions = render_tabs(ui, &tabs);
                if !tab_actions.is_empty() {
                    if let Ok(mut a) = actions.lock() {
                        a.extend(tab_actions);
                    }
                }
            }
        }
    });

    // Write back changed section/tactic selections into shared state
    if let Ok(mut s) = shared.state.section.lock() {
        *s = section;
    }
    if let Ok(mut t) = shared.state.vision_tactic.lock() {
        *t = vision_tactic;
    }
}
