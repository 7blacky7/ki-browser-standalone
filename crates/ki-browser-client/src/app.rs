//! egui application for the ki-browser viewer client.
//!
//! Renders the browser viewport from JPEG frames received via WebSocket,
//! provides a tab bar and URL bar, and forwards mouse/keyboard input
//! back to the server for CEF interaction.

use crate::connection::{self, ViewerState};
use crate::protocol::ClientMessage;

use egui::{
    CentralPanel, Color32, ColorImage, Frame, Key, Modifiers, RichText, TextureHandle,
    TextureOptions, TopBottomPanel, Vec2,
};
use std::sync::Arc;
use tokio::sync::mpsc;

/// Main application state for the viewer client.
pub struct ViewerApp {
    /// Server WebSocket URL.
    server_url: String,
    /// Shared state updated by the WebSocket connection.
    state: Arc<ViewerState>,
    /// Sender for input messages to the server.
    input_tx: Option<mpsc::UnboundedSender<ClientMessage>>,
    /// GPU texture for the current frame.
    viewport_texture: Option<TextureHandle>,
    /// Last known frame dimensions (for change detection).
    last_frame_size: (u32, u32),
    /// URL bar text input.
    url_input: String,
    /// Tokio runtime handle for spawning the connection.
    runtime: tokio::runtime::Handle,
    /// Whether connection has been initiated.
    connection_started: bool,
}

impl ViewerApp {
    pub fn new(server_url: String, runtime: tokio::runtime::Handle) -> Self {
        Self {
            server_url,
            state: Arc::new(ViewerState::new()),
            input_tx: None,
            viewport_texture: None,
            last_frame_size: (0, 0),
            url_input: String::new(),
            runtime,
            connection_started: false,
        }
    }

    /// Start the WebSocket connection on first frame.
    fn ensure_connected(&mut self, ctx: &egui::Context) {
        if self.connection_started {
            return;
        }
        self.connection_started = true;
        let _guard = self.runtime.enter();
        let tx = connection::spawn_connection(
            self.server_url.clone(),
            self.state.clone(),
            ctx.clone(),
        );
        self.input_tx = Some(tx);
    }

    /// Send an input message to the server (fire-and-forget).
    fn send(&self, msg: ClientMessage) {
        if let Some(tx) = &self.input_tx {
            let _ = tx.send(msg);
        }
    }

    /// Render the tab bar at the top.
    fn render_tab_bar(&mut self, ui: &mut egui::Ui) {
        let tabs = self.state.tabs.lock().clone();
        let active = self.state.active_tab.lock().clone();

        ui.horizontal(|ui| {
            for tab in &tabs {
                let is_active = active.as_deref() == Some(&tab.id);
                let label = if tab.title.is_empty() {
                    "Loading..."
                } else if tab.title.len() > 25 {
                    &tab.title[..25]
                } else {
                    &tab.title
                };

                let btn = if is_active {
                    egui::Button::new(RichText::new(label).strong())
                        .fill(Color32::from_rgb(60, 60, 80))
                } else {
                    egui::Button::new(label)
                };

                if ui.add(btn).clicked() && !is_active {
                    self.send(ClientMessage::SetActiveTab {
                        tab_id: tab.id.clone(),
                    });
                }

                // Close button per tab.
                if ui.small_button("x").clicked() {
                    self.send(ClientMessage::CloseTab {
                        tab_id: tab.id.clone(),
                    });
                }

                ui.separator();
            }

            // New tab button.
            if ui.button("+").clicked() {
                self.send(ClientMessage::CreateTab {
                    url: "https://example.com".into(),
                });
            }
        });
    }

    /// Render the URL/navigation bar.
    fn render_url_bar(&mut self, ui: &mut egui::Ui) {
        ui.horizontal(|ui| {
            if ui.button("<").clicked() {
                self.send(ClientMessage::GoBack);
            }
            if ui.button(">").clicked() {
                self.send(ClientMessage::GoForward);
            }

            let response = ui.add(
                egui::TextEdit::singleline(&mut self.url_input)
                    .desired_width(ui.available_width() - 60.0)
                    .hint_text("URL eingeben..."),
            );

            if response.lost_focus() && ui.input(|i| i.key_pressed(Key::Enter)) {
                let url = self.url_input.trim().to_string();
                if !url.is_empty() {
                    let url = if !url.starts_with("http://") && !url.starts_with("https://") {
                        format!("https://{url}")
                    } else {
                        url
                    };
                    self.send(ClientMessage::Navigate { url });
                }
            }
        });

        // Sync URL bar with active tab's URL.
        let tabs = self.state.tabs.lock().clone();
        let active = self.state.active_tab.lock().clone();
        if let Some(active_id) = &active {
            if let Some(tab) = tabs.iter().find(|t| &t.id == active_id) {
                if self.url_input != tab.url && !tab.url.is_empty() {
                    self.url_input = tab.url.clone();
                }
            }
        }
    }

    /// Render the browser viewport and handle input forwarding.
    fn render_viewport(&mut self, ui: &mut egui::Ui) {
        let mut frame_guard = self.state.frame_rgba.lock();
        if let Some(frame) = frame_guard.take() {
            let size = [frame.width as usize, frame.height as usize];
            let image = ColorImage::from_rgba_unmultiplied(size, &frame.rgba);

            if self.last_frame_size != (frame.width, frame.height) {
                self.viewport_texture = None;
                self.last_frame_size = (frame.width, frame.height);
            }

            match &mut self.viewport_texture {
                Some(tex) => tex.set(image, TextureOptions::LINEAR),
                None => {
                    self.viewport_texture =
                        Some(ui.ctx().load_texture("viewport", image, TextureOptions::LINEAR));
                }
            }
        }
        drop(frame_guard);

        if let Some(tex) = &self.viewport_texture {
            let available = ui.available_size();
            let tex_size = tex.size_vec2();

            // Scale to fit while maintaining aspect ratio.
            let scale = (available.x / tex_size.x).min(available.y / tex_size.y).min(1.0);
            let display_size = Vec2::new(tex_size.x * scale, tex_size.y * scale);

            let (rect, response) = ui.allocate_exact_size(display_size, egui::Sense::click_and_drag());
            ui.painter().image(tex.id(), rect, egui::Rect::from_min_max(
                egui::pos2(0.0, 0.0),
                egui::pos2(1.0, 1.0),
            ), Color32::WHITE);

            // Forward mouse input.
            self.handle_viewport_input(&response, rect, tex_size, scale);
        } else {
            ui.centered_and_justified(|ui| {
                if *self.state.connected.lock() {
                    ui.label("Waiting for frames...");
                } else if let Some(err) = self.state.last_error.lock().as_ref() {
                    ui.label(RichText::new(format!("Error: {err}")).color(Color32::RED));
                } else {
                    ui.label("Connecting...");
                }
            });
        }
    }

    /// Convert viewport mouse events to server coordinates and send them.
    fn handle_viewport_input(
        &self,
        response: &egui::Response,
        rect: egui::Rect,
        tex_size: Vec2,
        scale: f32,
    ) {
        let to_server_coords = |pos: egui::Pos2| -> (i32, i32) {
            let local_x = (pos.x - rect.min.x) / scale;
            let local_y = (pos.y - rect.min.y) / scale;
            (
                local_x.clamp(0.0, tex_size.x) as i32,
                local_y.clamp(0.0, tex_size.y) as i32,
            )
        };

        if let Some(pos) = response.hover_pos() {
            let (x, y) = to_server_coords(pos);
            self.send(ClientMessage::MouseMove { x, y });
        }

        if response.clicked() {
            if let Some(pos) = response.interact_pointer_pos() {
                let (x, y) = to_server_coords(pos);
                self.send(ClientMessage::MouseClick { x, y, button: 0 });
            }
        }

        if response.secondary_clicked() {
            if let Some(pos) = response.interact_pointer_pos() {
                let (x, y) = to_server_coords(pos);
                self.send(ClientMessage::MouseClick { x, y, button: 2 });
            }
        }

        // Scroll wheel.
        let scroll = response.ctx.input(|i| i.smooth_scroll_delta);
        if scroll.y.abs() > 0.5 || scroll.x.abs() > 0.5 {
            if let Some(pos) = response.hover_pos() {
                let (x, y) = to_server_coords(pos);
                self.send(ClientMessage::MouseWheel {
                    x,
                    y,
                    delta_x: scroll.x as i32,
                    delta_y: scroll.y as i32,
                });
            }
        }

        // Keyboard input forwarding.
        if response.has_focus() || response.hovered() {
            response.ctx.input(|i| {
                for event in &i.events {
                    match event {
                        egui::Event::Text(text) => {
                            self.send(ClientMessage::TypeText {
                                text: text.clone(),
                            });
                        }
                        egui::Event::Key {
                            key,
                            pressed,
                            modifiers,
                            ..
                        } => {
                            if let Some(key_code) = egui_key_to_windows_keycode(key) {
                                let event_type = if *pressed { 0 } else { 3 }; // RawKeyDown / KeyUp
                                let mods = egui_modifiers_to_cef(modifiers);
                                self.send(ClientMessage::KeyEvent {
                                    event_type,
                                    modifiers: mods,
                                    windows_key_code: key_code,
                                    character: 0,
                                });
                            }
                        }
                        _ => {}
                    }
                }
            });
        }
    }
}

impl eframe::App for ViewerApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        self.ensure_connected(ctx);

        // Status bar at the bottom.
        TopBottomPanel::bottom("status_bar").show(ctx, |ui| {
            ui.horizontal(|ui| {
                let connected = *self.state.connected.lock();
                if connected {
                    ui.label(RichText::new("Connected").color(Color32::GREEN));
                } else {
                    ui.label(RichText::new("Disconnected").color(Color32::RED));
                }
                ui.separator();
                ui.label(&self.server_url);
            });
        });

        // Tab bar.
        TopBottomPanel::top("tab_bar").show(ctx, |ui| {
            self.render_tab_bar(ui);
        });

        // URL bar.
        TopBottomPanel::top("url_bar").show(ctx, |ui| {
            self.render_url_bar(ui);
        });

        // Browser viewport.
        CentralPanel::default()
            .frame(Frame::NONE.fill(Color32::BLACK))
            .show(ctx, |ui| {
                self.render_viewport(ui);
            });
    }
}

/// Map egui Key to Windows virtual key code (subset for common keys).
fn egui_key_to_windows_keycode(key: &Key) -> Option<i32> {
    match key {
        Key::Enter => Some(0x0D),
        Key::Tab => Some(0x09),
        Key::Backspace => Some(0x08),
        Key::Escape => Some(0x1B),
        Key::Space => Some(0x20),
        Key::Delete => Some(0x2E),
        Key::Home => Some(0x24),
        Key::End => Some(0x23),
        Key::PageUp => Some(0x21),
        Key::PageDown => Some(0x22),
        Key::ArrowLeft => Some(0x25),
        Key::ArrowUp => Some(0x26),
        Key::ArrowRight => Some(0x27),
        Key::ArrowDown => Some(0x28),
        Key::A => Some(0x41),
        Key::B => Some(0x42),
        Key::C => Some(0x43),
        Key::D => Some(0x44),
        Key::E => Some(0x45),
        Key::F => Some(0x46),
        Key::G => Some(0x47),
        Key::H => Some(0x48),
        Key::I => Some(0x49),
        Key::J => Some(0x4A),
        Key::K => Some(0x4B),
        Key::L => Some(0x4C),
        Key::M => Some(0x4D),
        Key::N => Some(0x4E),
        Key::O => Some(0x4F),
        Key::P => Some(0x50),
        Key::Q => Some(0x51),
        Key::R => Some(0x52),
        Key::S => Some(0x53),
        Key::T => Some(0x54),
        Key::U => Some(0x55),
        Key::V => Some(0x56),
        Key::W => Some(0x57),
        Key::X => Some(0x58),
        Key::Y => Some(0x59),
        Key::Z => Some(0x5A),
        Key::Num0 => Some(0x30),
        Key::Num1 => Some(0x31),
        Key::Num2 => Some(0x32),
        Key::Num3 => Some(0x33),
        Key::Num4 => Some(0x34),
        Key::Num5 => Some(0x35),
        Key::Num6 => Some(0x36),
        Key::Num7 => Some(0x37),
        Key::Num8 => Some(0x38),
        Key::Num9 => Some(0x39),
        Key::F1 => Some(0x70),
        Key::F2 => Some(0x71),
        Key::F3 => Some(0x72),
        Key::F4 => Some(0x73),
        Key::F5 => Some(0x74),
        _ => None,
    }
}

/// Convert egui Modifiers to CEF modifier bitmask.
fn egui_modifiers_to_cef(mods: &Modifiers) -> u32 {
    let mut flags = 0u32;
    if mods.shift { flags |= 1 << 1; }  // EVENTFLAG_SHIFT_DOWN
    if mods.ctrl { flags |= 1 << 2; }   // EVENTFLAG_CONTROL_DOWN
    if mods.alt { flags |= 1 << 3; }    // EVENTFLAG_ALT_DOWN
    if mods.command { flags |= 1 << 7; } // EVENTFLAG_COMMAND_DOWN
    flags
}
