//! Tab bar render orchestration: layout, drag-and-drop logic, and action dispatch.
//!
//! The public `render` function lays out the horizontal tab strip, delegates
//! per-tab painting to `painting::paint_tab`, handles drag-to-reorder state,
//! and paints the bottom border. Returns a `TabBarAction` with all user interactions.

use egui::{Color32, CornerRadius, Id, Pos2, Rect, Stroke, Ui, Vec2};

use super::painting;
use super::types::{DragState, TabBarAction, TabInfo, TAB_HEIGHT, TAB_SPACING};

/// Renders the tab bar and returns user interactions.
pub fn render(ui: &mut Ui, tabs: &[TabInfo], active_tab: usize) -> TabBarAction {
    let mut action = TabBarAction {
        selected: None,
        close: None,
        new_tab: false,
        reorder: None,
    };

    let drag_id = Id::new("tab_bar_drag_state");

    // Background for the entire tab bar area
    let bar_rect = Rect::from_min_size(
        ui.cursor().min,
        Vec2::new(ui.available_width(), TAB_HEIGHT + 4.0),
    );
    ui.painter().rect_filled(
        bar_rect,
        CornerRadius::ZERO,
        Color32::from_rgb(28, 28, 36),
    );

    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing.x = TAB_SPACING;
        ui.set_min_height(TAB_HEIGHT + 4.0);
        ui.add_space(4.0);

        // Collect tab rects for drop target calculation
        let mut tab_rects: Vec<Rect> = Vec::with_capacity(tabs.len());

        // Read current drag state
        let mut drag_state: DragState =
            ui.data(|d| d.get_temp(drag_id).unwrap_or_default());

        for (i, tab) in tabs.iter().enumerate() {
            let is_active = i == active_tab;
            let is_dragging = drag_state.dragging == Some(i);

            let result =
                painting::paint_tab(ui, &tab.title, tab.is_loading, is_active, is_dragging);
            tab_rects.push(result.tab_rect);

            // Drag detection
            if result.drag_started {
                drag_state.dragging = Some(i);
            }

            // Handle clicks: close takes priority, middle-click closes tab
            if result.middle_clicked {
                action.close = Some(i);
            } else if result.clicked {
                if result.mouse_on_close {
                    action.close = Some(i);
                } else {
                    action.selected = Some(i);
                }
            }
        }

        // Drop target: find where to insert the dragged tab
        handle_drag_drop(ui, &tab_rects, &mut drag_state, &mut action);

        // Save drag state
        ui.data_mut(|d| d.insert_temp(drag_id, drag_state));

        ui.add_space(4.0);

        // "+" new tab button
        if painting::paint_new_tab_button(ui) {
            action.new_tab = true;
        }
    });

    // Bottom border
    let bottom_y = bar_rect.max.y;
    ui.painter().line_segment(
        [
            Pos2::new(bar_rect.min.x, bottom_y),
            Pos2::new(bar_rect.max.x, bottom_y),
        ],
        Stroke::new(1.0, Color32::from_rgb(45, 45, 55)),
    );

    action
}

/// Processes drag-and-drop: draws insertion indicator while dragging, resolves
/// drop target on pointer release, and clears drag state.
fn handle_drag_drop(
    ui: &mut Ui,
    tab_rects: &[Rect],
    drag_state: &mut DragState,
    action: &mut TabBarAction,
) {
    let Some(drag_from) = drag_state.dragging else {
        return;
    };

    // Draw drop indicator line at the hovered tab boundary
    if let Some(mouse_pos) = ui.input(|i| i.pointer.hover_pos()) {
        for (i, rect) in tab_rects.iter().enumerate() {
            if i != drag_from && rect.contains(mouse_pos) {
                let insert_x = if i < drag_from {
                    rect.min.x - 1.0
                } else {
                    rect.max.x + 1.0
                };
                ui.painter().line_segment(
                    [
                        Pos2::new(insert_x, rect.min.y + 2.0),
                        Pos2::new(insert_x, rect.max.y - 2.0),
                    ],
                    Stroke::new(2.5, Color32::from_rgb(80, 120, 240)),
                );
                break;
            }
        }
    }

    // Handle drop on pointer release
    if ui.input(|i| i.pointer.any_released()) {
        if let Some(mouse_pos) = ui.input(|i| i.pointer.hover_pos()) {
            for (i, rect) in tab_rects.iter().enumerate() {
                if i != drag_from && rect.contains(mouse_pos) {
                    action.reorder = Some((drag_from, i));
                    break;
                }
            }
        }
        drag_state.dragging = None;
    }
}
