//! Renders the HTML source code view with a load button, URL label,
//! and a scrollable monospace code editor for the fetched source text.

use egui::{Color32, RichText, ScrollArea};
use uuid::Uuid;

use super::types::{DevToolsAction, PageInfo, SharedText, TextState};

/// Renders the Source section with a load button and scrollable code view.
///
/// Returns a `LoadSource` action when the user clicks the load button,
/// which the caller queues into the shared actions vector.
pub(super) fn render_source_view(
    ui: &mut egui::Ui,
    source: &SharedText,
    page_info: &PageInfo,
) -> Option<DevToolsAction> {
    let mut action = None;

    // Load button
    ui.horizontal(|ui| {
        let source_state = source.lock().ok();
        let is_loading = source_state
            .as_ref()
            .map(|s| matches!(**s, TextState::Loading))
            .unwrap_or(false);

        let btn_text = if is_loading { "Laden..." } else { "Quelltext laden" };
        if ui.add_enabled(!is_loading, egui::Button::new(btn_text)).clicked() {
            action = Some(DevToolsAction::LoadSource(Uuid::nil()));
        }

        ui.label(
            RichText::new(&page_info.url)
                .color(Color32::GRAY)
                .monospace()
                .size(11.0),
        );
    });
    ui.separator();

    // Source display
    let source_text = {
        let guard = source.lock().ok();
        match guard.as_deref() {
            Some(TextState::Empty) => None,
            Some(TextState::Loading) => Some(("Laden...".to_string(), false)),
            Some(TextState::Loaded(s)) => Some((s.clone(), true)),
            Some(TextState::Error(e)) => Some((format!("Fehler: {}", e), false)),
            None => None,
        }
    };

    if let Some((text, is_code)) = source_text {
        ScrollArea::both()
            .auto_shrink([false, false])
            .show(ui, |ui| {
                if is_code {
                    ui.add(
                        egui::TextEdit::multiline(&mut text.as_str())
                            .code_editor()
                            .desired_width(f32::INFINITY)
                            .font(egui::TextStyle::Monospace),
                    );
                } else {
                    ui.label(RichText::new(&text).color(Color32::GRAY).italics());
                }
            });
    } else {
        ui.centered_and_justified(|ui| {
            ui.label(
                RichText::new("Klicke 'Quelltext laden' um den HTML-Quelltext anzuzeigen")
                    .color(Color32::from_rgb(100, 100, 115)),
            );
        });
    }

    action
}
