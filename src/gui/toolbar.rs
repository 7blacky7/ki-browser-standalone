//! Navigation toolbar with back/forward/reload buttons and URL bar.

use egui::Ui;

/// Navigation action returned by the toolbar.
pub enum NavAction {
    Back,
    Forward,
    Reload,
    Navigate(String),
}

/// Renders the toolbar. Returns an optional navigation action.
pub fn render(
    ui: &mut Ui,
    url_input: &mut String,
    can_go_back: bool,
    can_go_forward: bool,
) -> Option<NavAction> {
    let mut action = None;

    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing.x = 4.0;

        // Back button
        let back_btn = ui.add_enabled(can_go_back, egui::Button::new("<").min_size(egui::vec2(28.0, 24.0)));
        if back_btn.clicked() {
            action = Some(NavAction::Back);
        }

        // Forward button
        let fwd_btn = ui.add_enabled(can_go_forward, egui::Button::new(">").min_size(egui::vec2(28.0, 24.0)));
        if fwd_btn.clicked() {
            action = Some(NavAction::Forward);
        }

        // Reload button
        if ui.button("R").clicked() {
            action = Some(NavAction::Reload);
        }

        // URL bar
        let response = ui.add(
            egui::TextEdit::singleline(url_input)
                .desired_width(ui.available_width() - 10.0)
                .hint_text("URL eingeben...")
                .font(egui::TextStyle::Monospace),
        );

        if response.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter)) {
            let url = if !url_input.starts_with("http://") && !url_input.starts_with("https://") {
                format!("https://{}", url_input)
            } else {
                url_input.clone()
            };
            action = Some(NavAction::Navigate(url));
        }
    });

    action
}
