//! Tab bar widget with clickable tabs, close buttons, and drag-to-reorder.
//!
//! Renders a horizontal tab strip where each tab is selectable by clicking its
//! title area. Each tab has an "x" close button on the right. A "+" button at
//! the end creates new tabs. Tabs can be reordered by dragging.

use egui::{Ui, Color32, Sense, Vec2, Rect, Pos2, CornerRadius, Stroke, Id};

/// Information about a single tab displayed in the tab bar.
pub struct TabInfo {
    pub id: uuid::Uuid,
    pub title: String,
    pub is_loading: bool,
}

/// Result of rendering the tab bar.
pub struct TabBarAction {
    /// Tab index the user clicked to switch to.
    pub selected: Option<usize>,
    /// Tab index whose close button was clicked.
    pub close: Option<usize>,
    /// Whether the "+" button was clicked.
    pub new_tab: bool,
    /// Reorder: (from_index, to_index).
    pub reorder: Option<(usize, usize)>,
}

/// Tab bar dimensions and styling constants.
const TAB_WIDTH: f32 = 180.0;
const TAB_HEIGHT: f32 = 30.0;
const CLOSE_BTN_SIZE: f32 = 16.0;
const CLOSE_BTN_MARGIN: f32 = 6.0;
const TAB_ROUNDING: f32 = 4.0;
const TAB_TITLE_PADDING: f32 = 10.0;
const TAB_SPACING: f32 = 1.0;
const NEW_TAB_BTN_SIZE: f32 = 28.0;

/// Persistent drag state stored in egui memory.
#[derive(Clone, Default)]
struct DragState {
    /// Index of the tab currently being dragged (None if not dragging).
    dragging: Option<usize>,
}

/// Renders the tab bar and returns user interactions.
pub fn render(
    ui: &mut Ui,
    tabs: &[TabInfo],
    active_tab: usize,
) -> TabBarAction {
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
        let mut drag_state: DragState = ui.data(|d| d.get_temp(drag_id).unwrap_or_default());

        for (i, tab) in tabs.iter().enumerate() {
            let is_active = i == active_tab;
            let is_dragging = drag_state.dragging == Some(i);

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
            let title = if tab.title.is_empty() {
                "New Tab"
            } else if tab.title.len() > 20 {
                // Find a safe char boundary
                let end = tab.title.char_indices()
                    .take(20)
                    .last()
                    .map(|(i, c)| i + c.len_utf8())
                    .unwrap_or(20.min(tab.title.len()));
                &tab.title[..end]
            } else {
                &tab.title
            };
            let loading_indicator = if tab.is_loading { " ..." } else { "" };
            let display_text = format!("{}{}", title, loading_indicator);

            // Allocate tab with click_and_drag sense for reordering
            let tab_size = Vec2::new(TAB_WIDTH, TAB_HEIGHT);
            let (tab_rect, tab_response) = ui.allocate_exact_size(tab_size, Sense::click_and_drag());
            tab_rects.push(tab_rect);

            // Close button hit-test
            let close_rect = Rect::from_min_size(
                Pos2::new(
                    tab_rect.max.x - CLOSE_BTN_SIZE - CLOSE_BTN_MARGIN,
                    tab_rect.min.y + (TAB_HEIGHT - CLOSE_BTN_SIZE) / 2.0,
                ),
                Vec2::new(CLOSE_BTN_SIZE, CLOSE_BTN_SIZE),
            );
            let mouse_on_close = ui.input(|i| {
                i.pointer.hover_pos()
                    .map(|p| close_rect.contains(p))
                    .unwrap_or(false)
            });

            // Drag detection
            if tab_response.drag_started() && !mouse_on_close {
                drag_state.dragging = Some(i);
            }

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
                    bg_color.r(), bg_color.g(), bg_color.b(),
                    (bg_color.a() as f32 * alpha) as u8,
                );
            }
            ui.painter().rect_filled(tab_rect, CornerRadius::same(TAB_ROUNDING as u8), bg_color);

            // Active tab indicator
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

            // Tab title
            let max_text_width = TAB_WIDTH - TAB_TITLE_PADDING - CLOSE_BTN_SIZE - CLOSE_BTN_MARGIN - 4.0;
            let text_pos = Pos2::new(
                tab_rect.min.x + TAB_TITLE_PADDING,
                tab_rect.center().y,
            );
            // Clip to tab area minus close button
            let clip_rect = Rect::from_min_max(
                Pos2::new(tab_rect.min.x + TAB_TITLE_PADDING, tab_rect.min.y),
                Pos2::new(tab_rect.min.x + TAB_TITLE_PADDING + max_text_width, tab_rect.max.y),
            );
            let painter = ui.painter().with_clip_rect(clip_rect);
            painter.text(
                text_pos,
                egui::Align2::LEFT_CENTER,
                &display_text,
                egui::FontId::proportional(12.0),
                title_color,
            );

            // Close button
            let close_hover_bg = if mouse_on_close {
                Color32::from_rgb(196, 43, 28)
            } else {
                Color32::TRANSPARENT
            };
            ui.painter().rect_filled(close_rect, CornerRadius::same(3), close_hover_bg);

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

            // Handle clicks: close takes priority, middle-click closes tab
            let middle_clicked = tab_response.hovered() && ui.input(|i| {
                i.pointer.button_clicked(egui::PointerButton::Middle)
            });
            if middle_clicked {
                action.close = Some(i);
            } else if tab_response.clicked() {
                if mouse_on_close {
                    action.close = Some(i);
                } else {
                    action.selected = Some(i);
                }
            }
        }

        // Drop target: find where to insert the dragged tab
        if let Some(drag_from) = drag_state.dragging {
            // Draw drop indicator
            if let Some(mouse_pos) = ui.input(|i| i.pointer.hover_pos()) {
                for (i, rect) in tab_rects.iter().enumerate() {
                    if i != drag_from && rect.contains(mouse_pos) {
                        // Draw insertion line
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

            // Handle drop
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

        // Save drag state
        ui.data_mut(|d| d.insert_temp(drag_id, drag_state));

        ui.add_space(4.0);

        // "+" new tab button (custom painted to match theme)
        let (plus_rect, plus_response) = ui.allocate_exact_size(
            Vec2::new(NEW_TAB_BTN_SIZE, TAB_HEIGHT),
            Sense::click(),
        );
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
        if plus_response.clicked() {
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_tab_info_default_title_display() {
        let tab = TabInfo {
            id: uuid::Uuid::new_v4(),
            title: String::new(),
            is_loading: false,
        };
        assert!(tab.title.is_empty());
    }

    #[test]
    fn test_tab_info_with_loading() {
        let tab = TabInfo {
            id: uuid::Uuid::new_v4(),
            title: "Test Page".to_string(),
            is_loading: true,
        };
        assert!(tab.is_loading);
        assert_eq!(tab.title, "Test Page");
    }

    #[test]
    fn test_close_rect_dimensions() {
        let tab_rect = Rect::from_min_size(
            Pos2::new(0.0, 0.0),
            Vec2::new(TAB_WIDTH, TAB_HEIGHT),
        );
        let close_rect = Rect::from_min_size(
            Pos2::new(
                tab_rect.max.x - CLOSE_BTN_SIZE - CLOSE_BTN_MARGIN,
                tab_rect.min.y + (TAB_HEIGHT - CLOSE_BTN_SIZE) / 2.0,
            ),
            Vec2::new(CLOSE_BTN_SIZE, CLOSE_BTN_SIZE),
        );
        assert!(close_rect.width() > 0.0);
        assert!(close_rect.height() > 0.0);
        assert!(close_rect.min.x >= tab_rect.min.x);
        assert!(close_rect.max.x <= tab_rect.max.x);
        assert!(close_rect.min.y >= tab_rect.min.y);
        assert!(close_rect.max.y <= tab_rect.max.y);
    }

    #[test]
    fn test_tab_constants_valid() {
        assert!(TAB_WIDTH > CLOSE_BTN_SIZE + CLOSE_BTN_MARGIN + TAB_TITLE_PADDING);
        assert!(TAB_HEIGHT > CLOSE_BTN_SIZE);
        assert!(TAB_ROUNDING >= 0.0);
    }

    #[test]
    fn test_tab_bar_action_defaults() {
        let action = TabBarAction {
            selected: None,
            close: None,
            new_tab: false,
            reorder: None,
        };
        assert!(action.selected.is_none());
        assert!(action.close.is_none());
        assert!(!action.new_tab);
        assert!(action.reorder.is_none());
    }
}
