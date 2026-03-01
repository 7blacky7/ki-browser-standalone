//! Tab bar widget for the GUI browser.

use egui::{Ui, Color32, RichText, Sense, Vec2};

/// Information about a single tab displayed in the tab bar.
pub struct TabInfo {
    pub id: uuid::Uuid,
    pub title: String,
    pub is_loading: bool,
}

/// Renders the tab bar. Returns (selected_tab_index, close_tab_index, new_tab_requested).
pub fn render(
    ui: &mut Ui,
    tabs: &[TabInfo],
    active_tab: usize,
) -> (Option<usize>, Option<usize>, bool) {
    let mut selected = None;
    let mut close = None;
    let mut new_tab = false;

    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing.x = 2.0;

        for (i, tab) in tabs.iter().enumerate() {
            let is_active = i == active_tab;
            let bg = if is_active {
                Color32::from_rgb(60, 60, 70)
            } else {
                Color32::from_rgb(40, 40, 50)
            };

            let title = if tab.title.is_empty() {
                "New Tab"
            } else if tab.title.len() > 20 {
                &tab.title[..20]
            } else {
                &tab.title
            };

            let loading_indicator = if tab.is_loading { " ..." } else { "" };

            let response = ui.allocate_ui(Vec2::new(160.0, 28.0), |ui| {
                let rect = ui.available_rect_before_wrap();
                ui.painter().rect_filled(rect, 4.0, bg);

                ui.horizontal_centered(|ui| {
                    ui.add_space(8.0);
                    let text = RichText::new(format!("{}{}", title, loading_indicator))
                        .color(if is_active { Color32::WHITE } else { Color32::GRAY })
                        .size(12.0);
                    ui.label(text);

                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        let close_btn = ui.small_button("x");
                        if close_btn.clicked() {
                            close = Some(i);
                        }
                    });
                });

                ui.interact(rect, ui.id().with(("tab", i)), Sense::click())
            });

            if response.inner.clicked() {
                selected = Some(i);
            }
        }

        // New tab button
        if ui.small_button("+").clicked() {
            new_tab = true;
        }
    });

    (selected, close, new_tab)
}
