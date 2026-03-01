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
}

impl ViewportState {
    pub fn new() -> Self {
        Self {
            texture: None,
            last_mouse_pos: None,
            last_frame_version: 0,
        }
    }

    /// Update the texture from CEF's BGRA frame buffer.
    /// Only re-converts if the frame buffer has actually changed (version check).
    /// Releases the read lock ASAP by cloning the buffer first.
    pub fn update_from_frame_buffer(
        &mut self,
        ctx: &egui::Context,
        frame_buffer: &Arc<RwLock<Vec<u8>>>,
        frame_size: &Arc<RwLock<(u32, u32)>>,
    ) {
        let current_version = FRAME_VERSION.load(Ordering::Acquire);
        if current_version == self.last_frame_version && self.texture.is_some() {
            // Frame buffer hasn't changed since last upload — skip.
            return;
        }

        // Hold read locks only for the clone, then release immediately.
        let (data, w, h) = {
            let fb = frame_buffer.read();
            let (w, h) = *frame_size.read();

            if fb.is_empty() || w == 0 || h == 0 {
                return;
            }

            // Clone the raw bytes so we can drop the lock before converting.
            (fb.clone(), w, h)
        };
        // Read locks released here.

        // BGRA → RGBA conversion (no lock held, safe for CEF to paint concurrently).
        let rgba: Vec<u8> = data
            .chunks_exact(4)
            .flat_map(|c| [c[2], c[1], c[0], c[3]])
            .collect();

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
            if response.clicked() || response.secondary_clicked() || response.middle_clicked() {
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

                if response.clicked() {
                    inputs.push(ViewportInput::MouseClick {
                        x: rel_x,
                        y: rel_y,
                        button: 0, // left
                    });
                }

                if response.secondary_clicked() {
                    inputs.push(ViewportInput::MouseClick {
                        x: rel_x,
                        y: rel_y,
                        button: 2, // right
                    });
                }

                if response.middle_clicked() {
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

/// Map egui key to Windows virtual key code (used by CEF).
fn egui_key_to_vk(key: egui::Key) -> i32 {
    match key {
        egui::Key::Enter => 0x0D,
        egui::Key::Tab => 0x09,
        egui::Key::Backspace => 0x08,
        egui::Key::Escape => 0x1B,
        egui::Key::Space => 0x20,
        egui::Key::Delete => 0x2E,
        egui::Key::Home => 0x24,
        egui::Key::End => 0x23,
        egui::Key::PageUp => 0x21,
        egui::Key::PageDown => 0x22,
        egui::Key::ArrowLeft => 0x25,
        egui::Key::ArrowUp => 0x26,
        egui::Key::ArrowRight => 0x27,
        egui::Key::ArrowDown => 0x28,
        egui::Key::A => 0x41,
        egui::Key::C => 0x43,
        egui::Key::V => 0x56,
        egui::Key::X => 0x58,
        egui::Key::Z => 0x5A,
        _ => 0,
    }
}
