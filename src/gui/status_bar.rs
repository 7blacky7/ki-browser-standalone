//! Status bar at the bottom of the browser window.

use egui::{Ui, Color32, RichText};

/// Renders the status bar with current URL, tab count, and API status.
pub fn render(
    ui: &mut Ui,
    current_url: &str,
    tab_count: usize,
    api_port: u16,
    is_loading: bool,
) {
    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing.x = 16.0;

        // Loading indicator
        let status = if is_loading { "Laden..." } else { "Bereit" };
        ui.label(RichText::new(status).color(Color32::LIGHT_GRAY).size(11.0));

        // Current URL (truncated)
        let url_display = if current_url.len() > 80 {
            format!("{}...", &current_url[..77])
        } else {
            current_url.to_string()
        };
        ui.label(RichText::new(url_display).color(Color32::GRAY).size(11.0));

        // Right-aligned info
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            ui.label(
                RichText::new(format!("API: :{}", api_port))
                    .color(Color32::from_rgb(100, 200, 100))
                    .size(11.0),
            );
            ui.label(
                RichText::new(format!("Tabs: {}", tab_count))
                    .color(Color32::LIGHT_GRAY)
                    .size(11.0),
            );
        });
    });
}
