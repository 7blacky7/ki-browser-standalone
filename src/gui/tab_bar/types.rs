//! Tab bar data types, styling constants, and internal drag state.
//!
//! Defines `TabInfo` (per-tab metadata for display), `TabBarAction` (user
//! interaction result), `DragState` (egui-persisted reorder tracking), and
//! all dimension/spacing constants for the tab strip layout.

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
pub const TAB_WIDTH: f32 = 180.0;
pub const TAB_HEIGHT: f32 = 30.0;
pub const CLOSE_BTN_SIZE: f32 = 16.0;
pub const CLOSE_BTN_MARGIN: f32 = 6.0;
pub const TAB_ROUNDING: f32 = 4.0;
pub const TAB_TITLE_PADDING: f32 = 10.0;
pub const TAB_SPACING: f32 = 1.0;
pub const NEW_TAB_BTN_SIZE: f32 = 28.0;

/// Persistent drag state stored in egui memory.
#[derive(Clone, Default)]
pub(crate) struct DragState {
    /// Index of the tab currently being dragged (None if not dragging).
    pub dragging: Option<usize>,
}

#[cfg(test)]
mod tests {
    use super::*;
    use egui::{Pos2, Rect, Vec2};

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
