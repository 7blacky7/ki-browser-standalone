//! Renders the tab overview section showing all open browser tabs
//! with their title, URL, loading state, and navigation action buttons.

use egui::{Color32, RichText, ScrollArea};

use super::types::{DevToolsAction, DevToolsTabInfo};

/// Renders the Tabs section as a scrollable list of tab cards with switch/close buttons.
///
/// Returns all actions queued during this frame (SwitchToTab, CloseTab).
pub(super) fn render_tabs(ui: &mut egui::Ui, tabs: &[DevToolsTabInfo]) -> Vec<DevToolsAction> {
    let mut actions = Vec::new();

    ui.label(
        RichText::new(format!("{} Tabs", tabs.len()))
            .color(Color32::LIGHT_GRAY)
            .size(13.0),
    );
    ui.separator();

    ScrollArea::vertical()
        .auto_shrink([false, false])
        .show(ui, |ui| {
            for (i, tab) in tabs.iter().enumerate() {
                let bg = if tab.is_active {
                    Color32::from_rgb(45, 55, 75)
                } else {
                    Color32::TRANSPARENT
                };

                egui::Frame::NONE
                    .fill(bg)
                    .corner_radius(4.0)
                    .inner_margin(8.0)
                    .show(ui, |ui| {
                        ui.horizontal(|ui| {
                            if tab.is_active {
                                ui.label(RichText::new(">").color(Color32::from_rgb(80, 120, 240)).strong());
                            }

                            ui.vertical(|ui| {
                                let title = if tab.title.is_empty() { "New Tab" } else { &tab.title };
                                let title_color = if tab.is_active { Color32::WHITE } else { Color32::LIGHT_GRAY };
                                ui.label(RichText::new(title).color(title_color).strong());
                                ui.label(
                                    RichText::new(&tab.url)
                                        .color(Color32::GRAY)
                                        .monospace()
                                        .size(11.0),
                                );
                            });

                            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                                if ui.small_button("X").clicked() {
                                    actions.push(DevToolsAction::CloseTab(i));
                                }
                                if !tab.is_active && ui.small_button("Wechseln").clicked() {
                                    actions.push(DevToolsAction::SwitchToTab(i));
                                }
                                if tab.is_loading {
                                    ui.label(RichText::new("Laden...").color(Color32::YELLOW).size(11.0));
                                }
                            });
                        });
                    });
            }
        });

    actions
}
