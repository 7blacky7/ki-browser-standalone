//! Custom window title bar replacing OS decorations.
//!
//! Renders a dark title bar with app name and minimize/maximize/close buttons
//! that match the browser's dark theme. The title bar area is draggable for
//! moving the window.

use egui::{Ui, Color32, Vec2, Rect, Pos2, Sense, Stroke, CornerRadius};

const TITLE_BAR_HEIGHT: f32 = 32.0;
const BUTTON_WIDTH: f32 = 46.0;
const BUTTON_HEIGHT: f32 = 32.0;

/// Action returned by the title bar.
pub enum TitleBarAction {
    Minimize,
    Maximize,
    Close,
}

/// Renders the custom title bar. Returns an optional action.
pub fn render(ui: &mut Ui, title: &str) -> Option<TitleBarAction> {
    let mut action = None;

    let (rect, response) = ui.allocate_exact_size(
        Vec2::new(ui.available_width(), TITLE_BAR_HEIGHT),
        Sense::click_and_drag(),
    );

    // Background
    ui.painter().rect_filled(
        rect,
        CornerRadius::ZERO,
        Color32::from_rgb(24, 24, 30),
    );

    // App title on the left
    ui.painter().text(
        Pos2::new(rect.min.x + 12.0, rect.center().y),
        egui::Align2::LEFT_CENTER,
        title,
        egui::FontId::proportional(13.0),
        Color32::from_rgb(160, 160, 175),
    );

    // Window drag on the title bar area (not on buttons)
    let buttons_start_x = rect.max.x - BUTTON_WIDTH * 3.0;
    if response.dragged() {
        if let Some(pos) = ui.input(|i| i.pointer.hover_pos()) {
            if pos.x < buttons_start_x {
                ui.ctx().send_viewport_cmd(egui::ViewportCommand::StartDrag);
            }
        }
    }

    // Double-click to maximize/restore
    if response.double_clicked() {
        if let Some(pos) = ui.input(|i| i.pointer.hover_pos()) {
            if pos.x < buttons_start_x {
                action = Some(TitleBarAction::Maximize);
            }
        }
    }

    // Window control buttons (right-aligned)
    let mouse_pos = ui.input(|i| i.pointer.hover_pos());

    // --- Minimize button ---
    let min_rect = Rect::from_min_size(
        Pos2::new(buttons_start_x, rect.min.y),
        Vec2::new(BUTTON_WIDTH, BUTTON_HEIGHT),
    );
    let min_hovered = mouse_pos.map(|p| min_rect.contains(p)).unwrap_or(false);
    let min_clicked = min_hovered && ui.input(|i| i.pointer.button_clicked(egui::PointerButton::Primary));

    if min_hovered {
        ui.painter().rect_filled(min_rect, CornerRadius::ZERO, Color32::from_rgb(50, 50, 60));
    }
    // Draw "─" line
    let min_center = min_rect.center();
    ui.painter().line_segment(
        [
            Pos2::new(min_center.x - 6.0, min_center.y),
            Pos2::new(min_center.x + 6.0, min_center.y),
        ],
        Stroke::new(1.5, Color32::from_rgb(200, 200, 210)),
    );
    if min_clicked {
        action = Some(TitleBarAction::Minimize);
    }

    // --- Maximize button ---
    let max_rect = Rect::from_min_size(
        Pos2::new(buttons_start_x + BUTTON_WIDTH, rect.min.y),
        Vec2::new(BUTTON_WIDTH, BUTTON_HEIGHT),
    );
    let max_hovered = mouse_pos.map(|p| max_rect.contains(p)).unwrap_or(false);
    let max_clicked = max_hovered && ui.input(|i| i.pointer.button_clicked(egui::PointerButton::Primary));

    if max_hovered {
        ui.painter().rect_filled(max_rect, CornerRadius::ZERO, Color32::from_rgb(50, 50, 60));
    }
    // Draw "□" square
    let max_center = max_rect.center();
    let sq = Rect::from_center_size(max_center, Vec2::splat(10.0));
    let sq_stroke = Stroke::new(1.2, Color32::from_rgb(200, 200, 210));
    ui.painter().rect_filled(sq, CornerRadius::same(1), Color32::TRANSPARENT);
    // Top
    ui.painter().line_segment([sq.left_top(), sq.right_top()], sq_stroke);
    // Bottom
    ui.painter().line_segment([sq.left_bottom(), sq.right_bottom()], sq_stroke);
    // Left
    ui.painter().line_segment([sq.left_top(), sq.left_bottom()], sq_stroke);
    // Right
    ui.painter().line_segment([sq.right_top(), sq.right_bottom()], sq_stroke);
    if max_clicked {
        action = Some(TitleBarAction::Maximize);
    }

    // --- Close button ---
    let close_rect = Rect::from_min_size(
        Pos2::new(buttons_start_x + BUTTON_WIDTH * 2.0, rect.min.y),
        Vec2::new(BUTTON_WIDTH, BUTTON_HEIGHT),
    );
    let close_hovered = mouse_pos.map(|p| close_rect.contains(p)).unwrap_or(false);
    let close_clicked = close_hovered && ui.input(|i| i.pointer.button_clicked(egui::PointerButton::Primary));

    if close_hovered {
        ui.painter().rect_filled(close_rect, CornerRadius::ZERO, Color32::from_rgb(196, 43, 28));
    }
    // Draw "✕" cross
    let close_center = close_rect.center();
    let x_half = 5.0;
    let x_color = if close_hovered {
        Color32::WHITE
    } else {
        Color32::from_rgb(200, 200, 210)
    };
    ui.painter().line_segment(
        [
            Pos2::new(close_center.x - x_half, close_center.y - x_half),
            Pos2::new(close_center.x + x_half, close_center.y + x_half),
        ],
        Stroke::new(1.5, x_color),
    );
    ui.painter().line_segment(
        [
            Pos2::new(close_center.x + x_half, close_center.y - x_half),
            Pos2::new(close_center.x - x_half, close_center.y + x_half),
        ],
        Stroke::new(1.5, x_color),
    );
    if close_clicked {
        action = Some(TitleBarAction::Close);
    }

    // Bottom border
    ui.painter().line_segment(
        [
            Pos2::new(rect.min.x, rect.max.y),
            Pos2::new(rect.max.x, rect.max.y),
        ],
        Stroke::new(1.0, Color32::from_rgb(45, 45, 55)),
    );

    action
}
