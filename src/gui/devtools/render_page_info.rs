//! Renders the page information section showing URL, title, loading status,
//! navigation state, open tab count, and REST API port.

use egui::{Color32, RichText};

use super::types::PageInfo;

/// Renders the PageInfo section as a two-column grid inside the DevTools window.
pub(super) fn render_page_info(ui: &mut egui::Ui, info: &PageInfo) {
    egui::Grid::new("page_info_grid")
        .num_columns(2)
        .spacing([12.0, 6.0])
        .show(ui, |ui| {
            ui.label(RichText::new("Titel:").color(Color32::GRAY));
            ui.label(RichText::new(&info.title).color(Color32::WHITE).strong());
            ui.end_row();

            ui.label(RichText::new("URL:").color(Color32::GRAY));
            ui.label(RichText::new(&info.url).color(Color32::from_rgb(120, 170, 255)).monospace());
            ui.end_row();

            ui.label(RichText::new("Status:").color(Color32::GRAY));
            let (status_text, status_color) = if info.is_loading {
                ("Laden...", Color32::YELLOW)
            } else {
                ("Bereit", Color32::from_rgb(100, 200, 100))
            };
            ui.label(RichText::new(status_text).color(status_color));
            ui.end_row();

            ui.label(RichText::new("Navigation:").color(Color32::GRAY));
            let nav = format!(
                "Zurueck: {} | Vorwaerts: {}",
                if info.can_go_back { "Ja" } else { "Nein" },
                if info.can_go_forward { "Ja" } else { "Nein" },
            );
            ui.label(RichText::new(nav).color(Color32::LIGHT_GRAY));
            ui.end_row();

            ui.label(RichText::new("Tabs:").color(Color32::GRAY));
            ui.label(RichText::new(info.tab_count.to_string()).color(Color32::LIGHT_GRAY));
            ui.end_row();

            ui.label(RichText::new("API-Port:").color(Color32::GRAY));
            ui.label(
                RichText::new(format!(":{}", info.api_port))
                    .color(Color32::from_rgb(100, 200, 100))
                    .monospace(),
            );
            ui.end_row();
        });
}
