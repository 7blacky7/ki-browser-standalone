//! CEF frame buffer rendering and input event forwarding.

use egui::{Ui, TextureHandle, TextureOptions, ColorImage, Sense, Vec2, Rect, Pos2};
use parking_lot::RwLock;
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};

/// Global frame version counter, incremented by CEF's on_paint callback.
/// The viewport compares against its own last-seen version to skip redundant
/// texture uploads.
static FRAME_VERSION: AtomicU64 = AtomicU64::new(0);

/// Call this from the on_paint callback after writing new frame data.
pub fn bump_frame_version() {
    FRAME_VERSION.fetch_add(1, Ordering::Release);
}

/// Holds the current frame texture from CEF.
pub struct ViewportState {
    pub texture: Option<TextureHandle>,
    last_mouse_pos: Option<(i32, i32)>,
    /// Last frame version we uploaded as a texture.
    last_frame_version: u64,
    /// ID of the tab whose frame buffer we last rendered. When the active tab
    /// changes we must force a texture re-upload even if FRAME_VERSION hasn't
    /// changed, because we're now pointing at a different buffer.
    last_tab_id: Option<uuid::Uuid>,
}

impl Default for ViewportState {
    fn default() -> Self {
        Self::new()
    }
}

impl ViewportState {
    pub fn new() -> Self {
        Self {
            texture: None,
            last_mouse_pos: None,
            last_frame_version: 0,
            last_tab_id: None,
        }
    }

    /// Update the texture from CEF's BGRA frame buffer.
    /// Only re-converts if the frame buffer has actually changed (version check)
    /// or if the active tab has changed (different buffer entirely).
    /// Releases the read lock ASAP by cloning the buffer first.
    pub fn update_from_frame_buffer(
        &mut self,
        ctx: &egui::Context,
        frame_buffer: &Arc<RwLock<Vec<u8>>>,
        frame_size: &Arc<RwLock<(u32, u32)>>,
        tab_id: uuid::Uuid,
    ) {
        let tab_changed = self.last_tab_id != Some(tab_id);
        let current_version = FRAME_VERSION.load(Ordering::Acquire);
        if !tab_changed && current_version == self.last_frame_version && self.texture.is_some() {
            // Same tab, same frame version — skip.
            return;
        }
        if tab_changed {
            self.last_tab_id = Some(tab_id);
        }

        // Convert BGRA → RGBA directly while holding the read lock, avoiding
        // a full fb.clone() (~8 MB at 1920x1080). Pure byte-shuffling is O(n)
        // and faster than clone + separate convert because it is a single pass.
        // The CEF on_paint callback only writes when it holds the write lock,
        // so holding the read lock here does not block painting for long.
        let (rgba, w, h) = {
            let fb = frame_buffer.read();
            let (w, h) = *frame_size.read();

            if fb.is_empty() || w == 0 || h == 0 {
                return;
            }

            let expected = (w as usize) * (h as usize) * 4;
            let len = fb.len().min(expected);
            let mut rgba = Vec::with_capacity(len);
            for chunk in fb[..len].chunks_exact(4) {
                rgba.push(chunk[2]); // R from BGRA[2]
                rgba.push(chunk[1]); // G from BGRA[1]
                rgba.push(chunk[0]); // B from BGRA[0]
                rgba.push(chunk[3]); // A from BGRA[3]
            }
            (rgba, w, h)
        };
        // Read lock released here — single allocation, no intermediate clone.

        let image = ColorImage::from_rgba_unmultiplied([w as usize, h as usize], &rgba);
        self.texture = Some(ctx.load_texture("cef_page", image, TextureOptions::LINEAR));
        self.last_frame_version = current_version;
    }
}

/// Input event to forward to CEF.
pub enum ViewportInput {
    MouseMove { x: i32, y: i32 },
    MouseClick { x: i32, y: i32, button: i32 },
    MouseWheel { x: i32, y: i32, delta_x: i32, delta_y: i32 },
    KeyDown { key_code: i32, character: u16 },
    KeyUp { key_code: i32, character: u16 },
    CharInput { character: u16 },
}

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

            // Collect mouse events relative to the viewport
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

            // Scroll events
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

            // Keyboard events (when viewport has focus)
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
        } else {
            ui.centered_and_justified(|ui| {
                ui.label("Lade...");
            });
        }
    });

    inputs
}

/// Maps egui key to Windows virtual key code (VK_*) used by CEF's key event API.
///
/// Covers navigation, editing, function keys, and the full A-Z alphabet
/// so that keyboard shortcuts (Ctrl+S, Ctrl+F, etc.) work correctly.
fn egui_key_to_vk(key: egui::Key) -> i32 {
    match key {
        // Navigation & editing
        egui::Key::Enter => 0x0D,
        egui::Key::Tab => 0x09,
        egui::Key::Backspace => 0x08,
        egui::Key::Escape => 0x1B,
        egui::Key::Space => 0x20,
        egui::Key::Delete => 0x2E,
        egui::Key::Insert => 0x2D,
        egui::Key::Home => 0x24,
        egui::Key::End => 0x23,
        egui::Key::PageUp => 0x21,
        egui::Key::PageDown => 0x22,
        egui::Key::ArrowLeft => 0x25,
        egui::Key::ArrowUp => 0x26,
        egui::Key::ArrowRight => 0x27,
        egui::Key::ArrowDown => 0x28,
        // Function keys (F1-F12)
        egui::Key::F1 => 0x70,
        egui::Key::F2 => 0x71,
        egui::Key::F3 => 0x72,
        egui::Key::F4 => 0x73,
        egui::Key::F5 => 0x74,
        egui::Key::F6 => 0x75,
        egui::Key::F7 => 0x76,
        egui::Key::F8 => 0x77,
        egui::Key::F9 => 0x78,
        egui::Key::F10 => 0x79,
        egui::Key::F11 => 0x7A,
        egui::Key::F12 => 0x7B,
        // Full A-Z alphabet (VK_A = 0x41 .. VK_Z = 0x5A)
        egui::Key::A => 0x41,
        egui::Key::B => 0x42,
        egui::Key::C => 0x43,
        egui::Key::D => 0x44,
        egui::Key::E => 0x45,
        egui::Key::F => 0x46,
        egui::Key::G => 0x47,
        egui::Key::H => 0x48,
        egui::Key::I => 0x49,
        egui::Key::J => 0x4A,
        egui::Key::K => 0x4B,
        egui::Key::L => 0x4C,
        egui::Key::M => 0x4D,
        egui::Key::N => 0x4E,
        egui::Key::O => 0x4F,
        egui::Key::P => 0x50,
        egui::Key::Q => 0x51,
        egui::Key::R => 0x52,
        egui::Key::S => 0x53,
        egui::Key::T => 0x54,
        egui::Key::U => 0x55,
        egui::Key::V => 0x56,
        egui::Key::W => 0x57,
        egui::Key::X => 0x58,
        egui::Key::Y => 0x59,
        egui::Key::Z => 0x5A,
        // Number row (VK_0 = 0x30 .. VK_9 = 0x39)
        egui::Key::Num0 => 0x30,
        egui::Key::Num1 => 0x31,
        egui::Key::Num2 => 0x32,
        egui::Key::Num3 => 0x33,
        egui::Key::Num4 => 0x34,
        egui::Key::Num5 => 0x35,
        egui::Key::Num6 => 0x36,
        egui::Key::Num7 => 0x37,
        egui::Key::Num8 => 0x38,
        egui::Key::Num9 => 0x39,
        _ => 0,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_egui_key_to_vk_navigation_keys() {
        assert_eq!(egui_key_to_vk(egui::Key::Enter), 0x0D);
        assert_eq!(egui_key_to_vk(egui::Key::Tab), 0x09);
        assert_eq!(egui_key_to_vk(egui::Key::Backspace), 0x08);
        assert_eq!(egui_key_to_vk(egui::Key::Escape), 0x1B);
        assert_eq!(egui_key_to_vk(egui::Key::Space), 0x20);
        assert_eq!(egui_key_to_vk(egui::Key::Delete), 0x2E);
        assert_eq!(egui_key_to_vk(egui::Key::Insert), 0x2D);
        assert_eq!(egui_key_to_vk(egui::Key::Home), 0x24);
        assert_eq!(egui_key_to_vk(egui::Key::End), 0x23);
        assert_eq!(egui_key_to_vk(egui::Key::PageUp), 0x21);
        assert_eq!(egui_key_to_vk(egui::Key::PageDown), 0x22);
    }

    #[test]
    fn test_egui_key_to_vk_arrow_keys() {
        assert_eq!(egui_key_to_vk(egui::Key::ArrowLeft), 0x25);
        assert_eq!(egui_key_to_vk(egui::Key::ArrowUp), 0x26);
        assert_eq!(egui_key_to_vk(egui::Key::ArrowRight), 0x27);
        assert_eq!(egui_key_to_vk(egui::Key::ArrowDown), 0x28);
    }

    #[test]
    fn test_egui_key_to_vk_function_keys() {
        assert_eq!(egui_key_to_vk(egui::Key::F1), 0x70);
        assert_eq!(egui_key_to_vk(egui::Key::F5), 0x74);
        assert_eq!(egui_key_to_vk(egui::Key::F12), 0x7B);
    }

    #[test]
    fn test_egui_key_to_vk_alphabet_complete() {
        // Verify all 26 letters map to VK_A (0x41) through VK_Z (0x5A)
        let keys = [
            egui::Key::A, egui::Key::B, egui::Key::C, egui::Key::D,
            egui::Key::E, egui::Key::F, egui::Key::G, egui::Key::H,
            egui::Key::I, egui::Key::J, egui::Key::K, egui::Key::L,
            egui::Key::M, egui::Key::N, egui::Key::O, egui::Key::P,
            egui::Key::Q, egui::Key::R, egui::Key::S, egui::Key::T,
            egui::Key::U, egui::Key::V, egui::Key::W, egui::Key::X,
            egui::Key::Y, egui::Key::Z,
        ];
        for (i, key) in keys.iter().enumerate() {
            assert_eq!(egui_key_to_vk(*key), 0x41 + i as i32, "Key {:?} should map to VK {:#X}", key, 0x41 + i as i32);
        }
    }

    #[test]
    fn test_egui_key_to_vk_number_row() {
        assert_eq!(egui_key_to_vk(egui::Key::Num0), 0x30);
        assert_eq!(egui_key_to_vk(egui::Key::Num5), 0x35);
        assert_eq!(egui_key_to_vk(egui::Key::Num9), 0x39);
    }

    #[test]
    fn test_egui_key_to_vk_unmapped_returns_zero() {
        // Minus key is not mapped and should return 0
        assert_eq!(egui_key_to_vk(egui::Key::Minus), 0);
    }

    #[test]
    fn test_viewport_state_new_defaults() {
        let state = ViewportState::new();
        assert!(state.texture.is_none());
        assert_eq!(state.last_frame_version, 0);
        assert!(state.last_tab_id.is_none());
    }

    #[test]
    fn test_bump_frame_version_increments() {
        let before = FRAME_VERSION.load(Ordering::Acquire);
        bump_frame_version();
        let after = FRAME_VERSION.load(Ordering::Acquire);
        assert_eq!(after, before + 1);
    }
}
