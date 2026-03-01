//! DevTools as a separate OS window via egui `show_viewport_deferred`.
//!
//! All mutable state is wrapped in `Arc<Mutex<T>>` or `Arc<AtomicBool>` so it
//! can be shared with the deferred viewport closure which requires
//! `Send + Sync + 'static`. The DevTools window shows page information,
//! HTML source code, KI vision tactics preview, and a tab overview.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};

use egui::{Color32, RichText, ScrollArea, Vec2};
use uuid::Uuid;

// ---------------------------------------------------------------------------
// Section / VisionTactic enums
// ---------------------------------------------------------------------------

/// Which section is active in the DevTools window.
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Section {
    PageInfo,
    Source,
    KiVision,
    Tabs,
}

/// Which KI vision tactic is selected.
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum VisionTactic {
    Annotated,
    Labels,
    DomSnapshot,
    DomAnnotate,
    StructuredData,
    ContentExtract,
    StructureAnalysis,
    Forms,
}

impl VisionTactic {
    fn label(&self) -> &'static str {
        match self {
            Self::Annotated => "Vision Annotated",
            Self::Labels => "Vision Labels",
            Self::DomSnapshot => "DOM Snapshot",
            Self::DomAnnotate => "DOM Annotate",
            Self::StructuredData => "Structured Data",
            Self::ContentExtract => "Content Extract",
            Self::StructureAnalysis => "Seitenstruktur",
            Self::Forms => "Formulare",
        }
    }

    fn description(&self) -> &'static str {
        match self {
            Self::Annotated => "Screenshot mit nummerierten Element-Overlays",
            Self::Labels => "JSON-Liste aller erkannten Elemente mit Rollen",
            Self::DomSnapshot => "Vollstaendiger DOM-Tree mit Bounding Boxes",
            Self::DomAnnotate => "Farbig markierte Element-Typen (Links, Buttons, Inputs)",
            Self::StructuredData => "JSON-LD, OpenGraph, Meta-Tags, Microdata",
            Self::ContentExtract => "Hauptinhalt der Seite (Readability)",
            Self::StructureAnalysis => "Seitenstruktur, Sektionen, Seitentyp",
            Self::Forms => "Erkannte Formulare mit Feldern",
        }
    }

    fn color(&self) -> Color32 {
        match self {
            Self::Annotated => Color32::from_rgb(255, 100, 100),
            Self::Labels => Color32::from_rgb(255, 150, 80),
            Self::DomSnapshot => Color32::from_rgb(100, 200, 255),
            Self::DomAnnotate => Color32::from_rgb(100, 255, 100),
            Self::StructuredData => Color32::from_rgb(200, 150, 255),
            Self::ContentExtract => Color32::from_rgb(255, 220, 100),
            Self::StructureAnalysis => Color32::from_rgb(100, 220, 200),
            Self::Forms => Color32::from_rgb(255, 180, 200),
        }
    }

    fn all() -> &'static [VisionTactic] {
        &[
            Self::Annotated,
            Self::Labels,
            Self::DomSnapshot,
            Self::DomAnnotate,
            Self::StructuredData,
            Self::ContentExtract,
            Self::StructureAnalysis,
            Self::Forms,
        ]
    }
}

// ---------------------------------------------------------------------------
// Shared data containers
// ---------------------------------------------------------------------------

/// Info about a single tab for the Tabs section.
#[derive(Clone)]
pub struct DevToolsTabInfo {
    pub id: Uuid,
    pub title: String,
    pub url: String,
    pub is_loading: bool,
    pub is_active: bool,
}

/// Info about the current page for the PageInfo section.
#[derive(Clone, Default)]
pub struct PageInfo {
    pub title: String,
    pub url: String,
    pub is_loading: bool,
    pub can_go_back: bool,
    pub can_go_forward: bool,
    pub api_port: u16,
    pub tab_count: usize,
}

/// Shared container for async text fetching (source code, vision results).
pub type SharedText = Arc<Mutex<TextState>>;

/// State for async-loaded text content.
pub enum TextState {
    Empty,
    Loading,
    Loaded(String),
    Error(String),
}

/// Shared container for async image fetching (annotated screenshots).
pub type SharedImage = Arc<Mutex<ImageState>>;

/// State for async-loaded images.
pub enum ImageState {
    Empty,
    Loading,
    Loaded(Vec<u8>),
    Error(String),
}

/// Action requested by the DevTools window, queued for the main app to handle.
pub enum DevToolsAction {
    /// Request to load the page source code for the active tab.
    LoadSource(Uuid),
    /// Switch to a specific tab.
    SwitchToTab(usize),
    /// Close a tab.
    CloseTab(usize),
    /// Run a KI vision tactic via REST API.
    RunVisionTactic {
        tactic: &'static str,
        tab_id: Uuid,
    },
}

// ---------------------------------------------------------------------------
// DevToolsState — all fields Arc-wrapped for cross-thread sharing
// ---------------------------------------------------------------------------

/// Persistent state for the DevTools window.
///
/// Every field is wrapped in `Arc<AtomicBool>` or `Arc<Mutex<T>>` because
/// `show_viewport_deferred` requires its closure to be `Send + Sync + 'static`.
pub struct DevToolsState {
    /// Whether the DevTools OS window should be shown.
    pub open: Arc<AtomicBool>,
    /// Active section tab inside DevTools.
    pub section: Arc<Mutex<Section>>,
    /// HTML source code state (loaded asynchronously).
    pub source: SharedText,
    /// Currently selected KI vision tactic.
    pub vision_tactic: Arc<Mutex<VisionTactic>>,
    /// Vision text/JSON result (loaded asynchronously).
    pub vision_text: SharedText,
    /// Vision image result (loaded asynchronously).
    pub vision_image: SharedImage,
    /// Cached egui texture handle for vision annotated screenshot.
    pub vision_texture: Arc<Mutex<Option<egui::TextureHandle>>>,
    /// Queued actions to be drained by the main app each frame.
    pub actions: Arc<Mutex<Vec<DevToolsAction>>>,
}

impl Default for DevToolsState {
    fn default() -> Self {
        Self {
            open: Arc::new(AtomicBool::new(false)),
            section: Arc::new(Mutex::new(Section::PageInfo)),
            source: Arc::new(Mutex::new(TextState::Empty)),
            vision_tactic: Arc::new(Mutex::new(VisionTactic::Annotated)),
            vision_text: Arc::new(Mutex::new(TextState::Empty)),
            vision_image: Arc::new(Mutex::new(ImageState::Empty)),
            vision_texture: Arc::new(Mutex::new(None)),
            actions: Arc::new(Mutex::new(Vec::new())),
        }
    }
}

impl DevToolsState {
    /// Call this from a background thread after fetching source.
    pub fn set_source(&self, source: String) {
        if let Ok(mut s) = self.source.lock() {
            *s = TextState::Loaded(source);
        }
    }

    /// Call this to set loading state.
    pub fn set_source_loading(&self) {
        if let Ok(mut s) = self.source.lock() {
            *s = TextState::Loading;
        }
    }

    /// Call this to set an error.
    pub fn set_source_error(&self, err: String) {
        if let Ok(mut s) = self.source.lock() {
            *s = TextState::Error(err);
        }
    }

    /// Get a clone of the shared source state for background threads.
    pub fn source_handle(&self) -> SharedText {
        self.source.clone()
    }

    /// Get the vision text handle for background threads.
    pub fn vision_text_handle(&self) -> SharedText {
        self.vision_text.clone()
    }

    /// Get the vision image handle for background threads.
    pub fn vision_image_handle(&self) -> SharedImage {
        self.vision_image.clone()
    }

    /// Get the current vision tactic name for the action handler.
    pub fn current_vision_tactic_name(&self) -> &'static str {
        let tactic = self.vision_tactic.lock()
            .map(|t| *t)
            .unwrap_or(VisionTactic::Annotated);
        match tactic {
            VisionTactic::Annotated => "annotated",
            VisionTactic::Labels => "labels",
            VisionTactic::DomSnapshot => "dom_snapshot",
            VisionTactic::DomAnnotate => "dom_annotate",
            VisionTactic::StructuredData => "structured_data",
            VisionTactic::ContentExtract => "content_extract",
            VisionTactic::StructureAnalysis => "structure_analysis",
            VisionTactic::Forms => "forms",
        }
    }

    /// Returns true if the current tactic produces an image result.
    pub fn current_tactic_is_image(&self) -> bool {
        let tactic = self.vision_tactic.lock()
            .map(|t| *t)
            .unwrap_or(VisionTactic::Annotated);
        matches!(tactic, VisionTactic::Annotated | VisionTactic::DomAnnotate)
    }
}

// ---------------------------------------------------------------------------
// DevToolsShared — bundles state + page info + tabs for the deferred viewport
// ---------------------------------------------------------------------------

/// Shared container passed into the deferred viewport closure.
///
/// Groups the DevTools UI state together with page/tab info that the main app
/// updates each frame before the deferred viewport renders.
pub struct DevToolsShared {
    pub state: DevToolsState,
    /// Current page info, updated by the main app every frame.
    pub page_info: Arc<Mutex<PageInfo>>,
    /// Current tab list, updated by the main app every frame.
    pub tabs: Arc<Mutex<Vec<DevToolsTabInfo>>>,
}

impl Default for DevToolsShared {
    fn default() -> Self {
        Self {
            state: DevToolsState::default(),
            page_info: Arc::new(Mutex::new(PageInfo::default())),
            tabs: Arc::new(Mutex::new(Vec::new())),
        }
    }
}

// ---------------------------------------------------------------------------
// Standalone render function for the deferred OS viewport
// ---------------------------------------------------------------------------

/// Renders the DevTools UI inside a deferred viewport (separate OS window).
///
/// Called by the closure passed to `ctx.show_viewport_deferred()`. Uses
/// `egui::CentralPanel` instead of `egui::Window` because this IS the window.
pub fn render_standalone(ctx: &egui::Context, shared: &DevToolsShared) {
    // Handle window close request (user clicks X on the OS window)
    if ctx.input(|i| i.viewport().close_requested()) {
        shared.state.open.store(false, Ordering::Relaxed);
        ctx.send_viewport_cmd(egui::ViewportCommand::Close);
        return;
    }

    // Read shared state (clone to release locks quickly)
    let page_info = shared.page_info.lock()
        .map(|pi| pi.clone())
        .unwrap_or_default();
    let tabs = shared.tabs.lock()
        .map(|t| t.clone())
        .unwrap_or_default();
    let mut section = shared.state.section.lock()
        .map(|s| *s)
        .unwrap_or(Section::PageInfo);
    let mut vision_tactic = shared.state.vision_tactic.lock()
        .map(|t| *t)
        .unwrap_or(VisionTactic::Annotated);

    let source = shared.state.source.clone();
    let vision_text = shared.state.vision_text.clone();
    let vision_image = shared.state.vision_image.clone();
    let actions = shared.state.actions.clone();
    let vision_texture = shared.state.vision_texture.clone();

    // Dark theme for the standalone window
    ctx.set_visuals(egui::Visuals::dark());

    egui::CentralPanel::default().show(ctx, |ui| {
        // Section tabs
        ui.horizontal(|ui| {
            let btn = |ui: &mut egui::Ui, label: &str, s: Section, current: &mut Section| {
                let active = *current == s;
                let text = if active {
                    RichText::new(label).color(Color32::WHITE).strong()
                } else {
                    RichText::new(label).color(Color32::GRAY)
                };
                if ui.selectable_label(active, text).clicked() {
                    *current = s;
                }
            };
            btn(ui, "Seiteninfo", Section::PageInfo, &mut section);
            ui.separator();
            btn(ui, "Quelltext", Section::Source, &mut section);
            ui.separator();
            btn(ui, "KI-Vision", Section::KiVision, &mut section);
            ui.separator();
            btn(ui, "Tabs", Section::Tabs, &mut section);
        });
        ui.separator();

        match section {
            Section::PageInfo => {
                render_page_info(ui, &page_info);
            }
            Section::Source => {
                if let Some(action) = render_source_view(ui, &source, &page_info) {
                    if let Ok(mut a) = actions.lock() {
                        a.push(action);
                    }
                }
            }
            Section::KiVision => {
                if let Some(action) = render_ki_vision(
                    ui, ctx, &mut vision_tactic, &vision_text, &vision_image,
                    &vision_texture, &page_info,
                ) {
                    if let Ok(mut a) = actions.lock() {
                        a.push(action);
                    }
                }
            }
            Section::Tabs => {
                let tab_actions = render_tabs(ui, &tabs);
                if !tab_actions.is_empty() {
                    if let Ok(mut a) = actions.lock() {
                        a.extend(tab_actions);
                    }
                }
            }
        }
    });

    // Write back changed state
    if let Ok(mut s) = shared.state.section.lock() {
        *s = section;
    }
    if let Ok(mut t) = shared.state.vision_tactic.lock() {
        *t = vision_tactic;
    }
}

// ---------------------------------------------------------------------------
// Section renderers (reused from original devtools, adapted for shared state)
// ---------------------------------------------------------------------------

fn render_page_info(ui: &mut egui::Ui, info: &PageInfo) {
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

fn render_source_view(
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

/// Renders the KI-Vision section with tactic selector and results.
fn render_ki_vision(
    ui: &mut egui::Ui,
    ctx: &egui::Context,
    tactic: &mut VisionTactic,
    vision_text: &SharedText,
    vision_image: &SharedImage,
    vision_texture: &Arc<Mutex<Option<egui::TextureHandle>>>,
    page_info: &PageInfo,
) -> Option<DevToolsAction> {
    let mut action = None;

    // Header with description
    ui.label(
        RichText::new("KI-Vision Taktiken")
            .color(Color32::WHITE)
            .strong()
            .size(14.0),
    );
    ui.label(
        RichText::new("Zeigt was die KI bei verschiedenen Analyse-Methoden sieht")
            .color(Color32::from_rgb(140, 140, 160))
            .size(11.0),
    );
    ui.add_space(4.0);

    // Tactic selector grid (2 columns)
    egui::Grid::new("vision_tactic_grid")
        .num_columns(2)
        .spacing([6.0, 4.0])
        .show(ui, |ui| {
            for (i, t) in VisionTactic::all().iter().enumerate() {
                let is_selected = *tactic == *t;
                let bg = if is_selected {
                    Color32::from_rgb(45, 55, 75)
                } else {
                    Color32::from_rgb(32, 32, 40)
                };
                let text_color = if is_selected { t.color() } else { Color32::GRAY };

                egui::Frame::NONE
                    .fill(bg)
                    .corner_radius(4.0)
                    .inner_margin(6.0)
                    .show(ui, |ui| {
                        let resp = ui.selectable_label(
                            is_selected,
                            RichText::new(t.label()).color(text_color).size(11.0),
                        );
                        if resp.clicked() {
                            *tactic = *t;
                        }
                    });

                if i % 2 == 1 {
                    ui.end_row();
                }
            }
        });

    ui.add_space(4.0);

    // Description of selected tactic
    ui.horizontal(|ui| {
        ui.label(
            RichText::new(">>")
                .color(tactic.color())
                .strong(),
        );
        ui.label(
            RichText::new(tactic.description())
                .color(Color32::LIGHT_GRAY)
                .size(11.0),
        );
    });
    ui.add_space(4.0);

    // Run button
    let is_image_tactic = matches!(*tactic, VisionTactic::Annotated | VisionTactic::DomAnnotate);
    let is_loading = if is_image_tactic {
        vision_image.lock().ok()
            .map(|s| matches!(*s, ImageState::Loading))
            .unwrap_or(false)
    } else {
        vision_text.lock().ok()
            .map(|s| matches!(*s, TextState::Loading))
            .unwrap_or(false)
    };

    ui.horizontal(|ui| {
        let btn_text = if is_loading {
            "Analysiere..."
        } else {
            "Analyse starten"
        };
        let btn = egui::Button::new(
            RichText::new(btn_text).color(if is_loading { Color32::GRAY } else { Color32::WHITE }),
        );
        if ui.add_enabled(!is_loading, btn).clicked() {
            action = Some(DevToolsAction::RunVisionTactic {
                tactic: match *tactic {
                    VisionTactic::Annotated => "annotated",
                    VisionTactic::Labels => "labels",
                    VisionTactic::DomSnapshot => "dom_snapshot",
                    VisionTactic::DomAnnotate => "dom_annotate",
                    VisionTactic::StructuredData => "structured_data",
                    VisionTactic::ContentExtract => "content_extract",
                    VisionTactic::StructureAnalysis => "structure_analysis",
                    VisionTactic::Forms => "forms",
                },
                tab_id: Uuid::nil(), // Will be resolved in browser_app
            });
        }

        ui.label(
            RichText::new(format!("Port :{}", page_info.api_port))
                .color(Color32::from_rgb(80, 80, 100))
                .monospace()
                .size(10.0),
        );
    });
    ui.separator();

    // Result display
    if is_image_tactic {
        render_vision_image(ui, ctx, vision_image, vision_texture);
    } else {
        render_vision_text(ui, vision_text);
    }

    action
}

/// Renders an image result (annotated screenshots).
fn render_vision_image(
    ui: &mut egui::Ui,
    ctx: &egui::Context,
    image_state: &SharedImage,
    texture: &Arc<Mutex<Option<egui::TextureHandle>>>,
) {
    let state = {
        let guard = image_state.lock().ok();
        match guard.as_deref() {
            Some(ImageState::Empty) => None,
            Some(ImageState::Loading) => Some(Err("Laden...".to_string())),
            Some(ImageState::Loaded(bytes)) => {
                // Decode PNG to egui texture
                match image::load_from_memory(bytes) {
                    Ok(img) => {
                        let rgba = img.to_rgba8();
                        let size = [rgba.width() as usize, rgba.height() as usize];
                        let pixels = rgba.into_raw();
                        let color_image = egui::ColorImage::from_rgba_unmultiplied(size, &pixels);
                        let tex = ctx.load_texture(
                            "vision_annotated",
                            color_image,
                            egui::TextureOptions::LINEAR,
                        );
                        if let Ok(mut t) = texture.lock() {
                            *t = Some(tex);
                        }
                        Some(Ok(()))
                    }
                    Err(e) => Some(Err(format!("Bild-Dekodierung fehlgeschlagen: {}", e))),
                }
            }
            Some(ImageState::Error(e)) => Some(Err(e.clone())),
            None => None,
        }
    };

    match state {
        None => {
            ui.centered_and_justified(|ui| {
                ui.label(
                    RichText::new("Klicke 'Analyse starten' um die KI-Vision zu testen")
                        .color(Color32::from_rgb(100, 100, 115)),
                );
            });
        }
        Some(Err(msg)) => {
            ui.label(RichText::new(&msg).color(Color32::YELLOW).italics());
        }
        Some(Ok(())) => {
            let tex_opt = texture.lock().ok().and_then(|t| t.clone());
            if let Some(tex) = tex_opt {
                ScrollArea::both()
                    .auto_shrink([false, false])
                    .show(ui, |ui| {
                        let tex_size = tex.size_vec2();
                        let available = ui.available_width();
                        let scale = (available / tex_size.x).min(1.0);
                        let display_size = Vec2::new(tex_size.x * scale, tex_size.y * scale);
                        ui.image(egui::load::SizedTexture::new(tex.id(), display_size));
                    });
            }
        }
    }
}

/// Renders a text/JSON result.
fn render_vision_text(ui: &mut egui::Ui, text_state: &SharedText) {
    let content = {
        let guard = text_state.lock().ok();
        match guard.as_deref() {
            Some(TextState::Empty) => None,
            Some(TextState::Loading) => Some(("Laden...".to_string(), false)),
            Some(TextState::Loaded(s)) => Some((s.clone(), true)),
            Some(TextState::Error(e)) => Some((format!("Fehler: {}", e), false)),
            None => None,
        }
    };

    match content {
        None => {
            ui.centered_and_justified(|ui| {
                ui.label(
                    RichText::new("Klicke 'Analyse starten' um die KI-Vision zu testen")
                        .color(Color32::from_rgb(100, 100, 115)),
                );
            });
        }
        Some((text, is_data)) => {
            ScrollArea::both()
                .auto_shrink([false, false])
                .show(ui, |ui| {
                    if is_data {
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
        }
    }
}

fn render_tabs(ui: &mut egui::Ui, tabs: &[DevToolsTabInfo]) -> Vec<DevToolsAction> {
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
                                if !tab.is_active {
                                    if ui.small_button("Wechseln").clicked() {
                                        actions.push(DevToolsAction::SwitchToTab(i));
                                    }
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_devtools_state_default_is_closed() {
        let state = DevToolsState::default();
        assert!(!state.open.load(Ordering::Relaxed));
    }

    #[test]
    fn test_devtools_state_open_toggle() {
        let state = DevToolsState::default();
        state.open.store(true, Ordering::Relaxed);
        assert!(state.open.load(Ordering::Relaxed));
        state.open.store(false, Ordering::Relaxed);
        assert!(!state.open.load(Ordering::Relaxed));
    }

    #[test]
    fn test_devtools_shared_default() {
        let shared = DevToolsShared::default();
        assert!(!shared.state.open.load(Ordering::Relaxed));
        let pi = shared.page_info.lock().unwrap();
        assert!(pi.title.is_empty());
        let tabs = shared.tabs.lock().unwrap();
        assert!(tabs.is_empty());
    }

    #[test]
    fn test_set_source_loading_and_error() {
        let state = DevToolsState::default();
        state.set_source_loading();
        {
            let guard = state.source.lock().unwrap();
            assert!(matches!(*guard, TextState::Loading));
        }
        state.set_source_error("test error".to_string());
        {
            let guard = state.source.lock().unwrap();
            assert!(matches!(*guard, TextState::Error(ref e) if e == "test error"));
        }
    }

    #[test]
    fn test_set_source_loaded() {
        let state = DevToolsState::default();
        state.set_source("<html></html>".to_string());
        let guard = state.source.lock().unwrap();
        assert!(matches!(*guard, TextState::Loaded(ref s) if s == "<html></html>"));
    }

    #[test]
    fn test_current_vision_tactic_name_default() {
        let state = DevToolsState::default();
        assert_eq!(state.current_vision_tactic_name(), "annotated");
    }

    #[test]
    fn test_current_tactic_is_image_default() {
        let state = DevToolsState::default();
        assert!(state.current_tactic_is_image());
    }

    #[test]
    fn test_current_tactic_is_image_text_tactic() {
        let state = DevToolsState::default();
        *state.vision_tactic.lock().unwrap() = VisionTactic::Labels;
        assert!(!state.current_tactic_is_image());
    }

    #[test]
    fn test_actions_queue_drain() {
        let state = DevToolsState::default();
        {
            let mut actions = state.actions.lock().unwrap();
            actions.push(DevToolsAction::LoadSource(Uuid::nil()));
            actions.push(DevToolsAction::SwitchToTab(0));
        }
        let drained: Vec<DevToolsAction> = state.actions.lock().unwrap().drain(..).collect();
        assert_eq!(drained.len(), 2);
        assert!(state.actions.lock().unwrap().is_empty());
    }

    #[test]
    fn test_vision_tactic_all_count() {
        assert_eq!(VisionTactic::all().len(), 8);
    }

    #[test]
    fn test_page_info_default() {
        let info = PageInfo::default();
        assert!(info.title.is_empty());
        assert!(info.url.is_empty());
        assert!(!info.is_loading);
        assert_eq!(info.api_port, 0);
        assert_eq!(info.tab_count, 0);
    }
}
