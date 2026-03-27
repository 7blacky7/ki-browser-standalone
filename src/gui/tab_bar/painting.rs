//! Low-level painting helpers for individual tab elements and the new-tab button.
//!
//! Extracts the per-tab visual rendering (background, title, close "x" icon)
//! and the "+" new-tab button from the main render loop to keep each file
//! under ~200 lines.

use egui::{Color32, CornerRadius, Pos2, Rect, Sense, Stroke, Ui, Vec2};

use super::types::{
    CLOSE_BTN_MARGIN, CLOSE_BTN_SIZE, NEW_TAB_BTN_SIZE, TAB_HEIGHT, TAB_ROUNDING,
    TAB_TITLE_PADDING, TAB_WIDTH,
};

/// Outcome of painting a single tab, used by the render loop to decide actions.
pub(crate) struct TabPaintResult {
    pub tab_rect: Rect,
    pub clicked: bool,
    pub mouse_on_close: bool,
    pub middle_clicked: bool,
    pub drag_started: bool,
}

/// Paints a single tab (background, title text, close button, active indicator)
/// and returns interaction metadata for the render loop.
pub(crate) fn paint_tab(
    ui: &mut Ui,
    title: &str,
    is_loading: bool,
    is_active: bool,
    is_dragging: bool,
) -> TabPaintResult {
    // Colors
    let bg = if is_active {
        Color32::from_rgb(55, 55, 68)
    } else {
        Color32::from_rgb(36, 36, 46)
    };
    let hover_bg = if is_active {
        Color32::from_rgb(65, 65, 78)
    } else {
        Color32::from_rgb(46, 46, 58)
    };
    let title_color = if is_active {
        Color32::from_rgb(230, 230, 240)
    } else {
        Color32::from_rgb(150, 150, 165)
    };

    // Truncate title for display (handle multi-byte chars safely)
    let display_title = if title.is_empty() {
        "New Tab"
    } else if title.len() > 20 {
        let end = title
            .char_indices()
            .take(20)
            .last()
            .map(|(i, c)| i + c.len_utf8())
            .unwrap_or(20.min(title.len()));
        &title[..end]
    } else {
        title
    };
    let loading_indicator = if is_loading { " ..." } else { "" };
    let display_text = format!("{}{}", display_title, loading_indicator);

    // Allocate tab with click_and_drag sense for reordering
    let tab_size = Vec2::new(TAB_WIDTH, TAB_HEIGHT);
    let (tab_rect, tab_response) = ui.allocate_exact_size(tab_size, Sense::click_and_drag());

    // Close button hit-test
    let close_rect = Rect::from_min_size(
        Pos2::new(
            tab_rect.max.x - CLOSE_BTN_SIZE - CLOSE_BTN_MARGIN,
            tab_rect.min.y + (TAB_HEIGHT - CLOSE_BTN_SIZE) / 2.0,
        ),
        Vec2::new(CLOSE_BTN_SIZE, CLOSE_BTN_SIZE),
    );
    let mouse_on_close = ui.input(|i| {
        i.pointer
            .hover_pos()
            .map(|p| close_rect.contains(p))
            .unwrap_or(false)
    });

    // Visual: dimmed if being dragged
    let alpha = if is_dragging { 0.5 } else { 1.0 };

    // Paint tab background
    let is_hovered = tab_response.hovered();
    let actual_bg = if is_hovered && !mouse_on_close && !is_dragging {
        hover_bg
    } else {
        bg
    };
    let mut bg_color = actual_bg;
    if alpha < 1.0 {
        bg_color = Color32::from_rgba_unmultiplied(
            bg_color.r(),
            bg_color.g(),
            bg_color.b(),
            (bg_color.a() as f32 * alpha) as u8,
        );
    }
    ui.painter()
        .rect_filled(tab_rect, CornerRadius::same(TAB_ROUNDING as u8), bg_color);

    // Active tab indicator (blue underline)
    if is_active {
        let underline = Rect::from_min_max(
            Pos2::new(tab_rect.min.x, tab_rect.max.y - 2.0),
            tab_rect.max,
        );
        ui.painter().rect_filled(
            underline,
            CornerRadius::ZERO,
            Color32::from_rgb(80, 120, 240),
        );
    }

    // Tab title text with clipping
    let max_text_width = TAB_WIDTH - TAB_TITLE_PADDING - CLOSE_BTN_SIZE - CLOSE_BTN_MARGIN - 4.0;
    let text_pos = Pos2::new(tab_rect.min.x + TAB_TITLE_PADDING, tab_rect.center().y);
    let clip_rect = Rect::from_min_max(
        Pos2::new(tab_rect.min.x + TAB_TITLE_PADDING, tab_rect.min.y),
        Pos2::new(
            tab_rect.min.x + TAB_TITLE_PADDING + max_text_width,
            tab_rect.max.y,
        ),
    );
    let painter = ui.painter().with_clip_rect(clip_rect);
    painter.text(
        text_pos,
        egui::Align2::LEFT_CENTER,
        &display_text,
        egui::FontId::proportional(12.0),
        title_color,
    );

    // Close button background and "x" icon
    paint_close_button(ui, close_rect, mouse_on_close, is_hovered);

    // Detect middle-click for close
    let middle_clicked = tab_response.hovered()
        && ui.input(|i| i.pointer.button_clicked(egui::PointerButton::Middle));

    TabPaintResult {
        tab_rect,
        clicked: tab_response.clicked(),
        mouse_on_close,
        middle_clicked,
        drag_started: tab_response.drag_started() && !mouse_on_close,
    }
}

/// Paints the close "x" button inside a tab: hover background and two crossing lines.
fn paint_close_button(ui: &mut Ui, close_rect: Rect, mouse_on_close: bool, is_hovered: bool) {
    let close_hover_bg = if mouse_on_close {
        Color32::from_rgb(196, 43, 28)
    } else {
        Color32::TRANSPARENT
    };
    ui.painter()
        .rect_filled(close_rect, CornerRadius::same(3), close_hover_bg);

    let x_color = if mouse_on_close {
        Color32::WHITE
    } else if is_hovered {
        Color32::from_rgb(180, 180, 190)
    } else {
        Color32::from_rgb(100, 100, 115)
    };
    let x_center = close_rect.center();
    let x_half = 4.0;
    ui.painter().line_segment(
        [
            Pos2::new(x_center.x - x_half, x_center.y - x_half),
            Pos2::new(x_center.x + x_half, x_center.y + x_half),
        ],
        Stroke::new(1.5, x_color),
    );
    ui.painter().line_segment(
        [
            Pos2::new(x_center.x + x_half, x_center.y - x_half),
            Pos2::new(x_center.x - x_half, x_center.y + x_half),
        ],
        Stroke::new(1.5, x_color),
    );
}

/// Paints the "+" new-tab button and returns true if it was clicked.
pub(crate) fn paint_new_tab_button(ui: &mut Ui) -> bool {
    let (plus_rect, plus_response) =
        ui.allocate_exact_size(Vec2::new(NEW_TAB_BTN_SIZE, TAB_HEIGHT), Sense::click());
    let plus_hovered = plus_response.hovered();
    if plus_hovered {
        ui.painter().rect_filled(
            plus_rect,
            CornerRadius::same(TAB_ROUNDING as u8),
            Color32::from_rgb(46, 46, 58),
        );
    }
    let plus_center = plus_rect.center();
    let plus_color = if plus_hovered {
        Color32::from_rgb(200, 200, 210)
    } else {
        Color32::from_rgb(120, 120, 135)
    };
    // Horizontal line
    ui.painter().line_segment(
        [
            Pos2::new(plus_center.x - 5.0, plus_center.y),
            Pos2::new(plus_center.x + 5.0, plus_center.y),
        ],
        Stroke::new(1.5, plus_color),
    );
    // Vertical line
    ui.painter().line_segment(
        [
            Pos2::new(plus_center.x, plus_center.y - 5.0),
            Pos2::new(plus_center.x, plus_center.y + 5.0),
        ],
        Stroke::new(1.5, plus_color),
    );
    plus_response.clicked()
}
