//! Viewport rendering and input event collection for the CEF browser surface.
//!
//! Renders the CEF frame buffer texture into an egui `CentralPanel`, scaling
//! it to fit while preserving aspect ratio. Collects mouse moves, clicks,
//! scroll events, and keyboard input, returning them as `ViewportInput` values
//! for forwarding to the CEF browser host.

use egui::{Pos2, Rect, Sense, Ui, Vec2};

use super::key_mapping::egui_key_to_vk;
use super::types::{ViewportInput, ViewportState};

/// Renders the viewport and collects input events. Returns collected inputs.
pub fn render(
    ui: &mut Ui,
    viewport: &mut ViewportState,
) -> Vec<ViewportInput> {
    let mut inputs = Vec::new();

    egui::CentralPanel::default().show_inside(ui, |ui| {
        if let Some(ref tex) = viewport.texture {
            let available = ui.available_size();
            let tex_size = tex.size_vec2();

            // Scale to fit while maintaining aspect ratio
            let scale = (available.x / tex_size.x).min(available.y / tex_size.y).min(1.0);
            let display_size = Vec2::new(tex_size.x * scale, tex_size.y * scale);

            // Use union of click_and_drag + focusable so keyboard events work
            let sense = Sense::click_and_drag().union(Sense::focusable_noninteractive());
            let (rect, response) = ui.allocate_exact_size(display_size, sense);

            // Draw the texture
            ui.painter().image(
                tex.id(),
                rect,
                Rect::from_min_max(Pos2::ZERO, Pos2::new(1.0, 1.0)),
                egui::Color32::WHITE,
            );

            // Request focus on any click so keyboard events are captured
            let any_click = ui.input(|i| {
                i.pointer.button_clicked(egui::PointerButton::Primary)
                    || i.pointer.button_clicked(egui::PointerButton::Secondary)
                    || i.pointer.button_clicked(egui::PointerButton::Middle)
            });
            if any_click && response.hovered() {
                response.request_focus();
            }

            collect_mouse_events(ui, &response, &rect, scale, viewport, &mut inputs);
            collect_scroll_events(ui, &response, &rect, scale, &mut inputs);
            collect_keyboard_events(ui, &response, &mut inputs);
        } else {
            ui.centered_and_justified(|ui| {
                ui.label("Lade...");
            });
        }
    });

    inputs
}

/// Collects mouse move and click events relative to the viewport rect.
fn collect_mouse_events(
    ui: &mut Ui,
    response: &egui::Response,
    rect: &Rect,
    scale: f32,
    viewport: &mut ViewportState,
    inputs: &mut Vec<ViewportInput>,
) {
    if let Some(pos) = response.hover_pos() {
        let rel_x = ((pos.x - rect.min.x) / scale) as i32;
        let rel_y = ((pos.y - rect.min.y) / scale) as i32;

        // Only send mouse move if position changed (avoid flooding CEF)
        let last = viewport.last_mouse_pos;
        if last.is_none() || last != Some((rel_x, rel_y)) {
            inputs.push(ViewportInput::MouseMove { x: rel_x, y: rel_y });
            viewport.last_mouse_pos = Some((rel_x, rel_y));
        }

        // Check button clicks via raw input (more reliable than response.clicked()
        // which can miss secondary/middle clicks when Sense::click_and_drag is used)
        let (left_click, right_click, middle_click) = ui.input(|i| (
            i.pointer.button_clicked(egui::PointerButton::Primary),
            i.pointer.button_clicked(egui::PointerButton::Secondary),
            i.pointer.button_clicked(egui::PointerButton::Middle),
        ));

        if left_click {
            inputs.push(ViewportInput::MouseClick {
                x: rel_x,
                y: rel_y,
                button: 0, // left
            });
        }

        if right_click {
            inputs.push(ViewportInput::MouseClick {
                x: rel_x,
                y: rel_y,
                button: 2, // right
            });
        }

        if middle_click {
            inputs.push(ViewportInput::MouseClick {
                x: rel_x,
                y: rel_y,
                button: 1, // middle
            });
        }
    }
}

/// Collects scroll wheel events when hovering over the viewport.
fn collect_scroll_events(
    ui: &mut Ui,
    response: &egui::Response,
    rect: &Rect,
    scale: f32,
    inputs: &mut Vec<ViewportInput>,
) {
    let scroll = ui.input(|i| i.smooth_scroll_delta);
    if scroll.y.abs() > 0.1 || scroll.x.abs() > 0.1 {
        if let Some(pos) = response.hover_pos() {
            let rel_x = ((pos.x - rect.min.x) / scale) as i32;
            let rel_y = ((pos.y - rect.min.y) / scale) as i32;
            inputs.push(ViewportInput::MouseWheel {
                x: rel_x,
                y: rel_y,
                delta_x: scroll.x as i32,
                delta_y: scroll.y as i32,
            });
        }
    }
}

/// Collects keyboard events (key down/up, character input) when viewport has focus.
fn collect_keyboard_events(
    ui: &mut Ui,
    response: &egui::Response,
    inputs: &mut Vec<ViewportInput>,
) {
    if response.has_focus() {
        ui.input(|i| {
            for event in &i.events {
                match event {
                    egui::Event::Text(text) => {
                        for c in text.chars() {
                            inputs.push(ViewportInput::CharInput {
                                character: c as u16,
                            });
                        }
                    }
                    egui::Event::Key { key, pressed, .. } => {
                        let key_code = egui_key_to_vk(*key);
                        if key_code != 0 {
                            if *pressed {
                                inputs.push(ViewportInput::KeyDown {
                                    key_code,
                                    character: 0,
                                });
                            } else {
                                inputs.push(ViewportInput::KeyUp {
                                    key_code,
                                    character: 0,
                                });
                            }
                        }
                    }
                    _ => {}
                }
            }
        });
    }
}
