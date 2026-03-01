//! Browser context menu shown on right-click in the viewport.
//!
//! Renders a dark-themed popup menu with common browser actions like
//! Back, Forward, Reload, Copy, Paste, Select All, and View Source.

use egui::{Color32, Vec2, Rect, Pos2, CornerRadius, Stroke, Id, Order, LayerId};

/// Action the user selected from the context menu.
#[derive(Debug, Clone, Copy)]
pub enum ContextMenuAction {
    Back,
    Forward,
    Reload,
    Copy,
    Cut,
    Paste,
    SelectAll,
    ViewSource,
}

/// Persistent state for the context menu.
#[derive(Clone, Default)]
pub struct ContextMenuState {
    /// Screen position where the menu should appear.
    pub position: Option<Pos2>,
    /// Whether the menu is currently visible.
    pub open: bool,
}

const MENU_WIDTH: f32 = 200.0;
const ITEM_HEIGHT: f32 = 28.0;
const SEPARATOR_HEIGHT: f32 = 9.0;
const MENU_PADDING: f32 = 4.0;
const MENU_ROUNDING: f32 = 6.0;

/// A menu item definition.
enum MenuItem {
    Action {
        label: &'static str,
        shortcut: &'static str,
        action: ContextMenuAction,
        enabled: bool,
    },
    Separator,
}

/// Renders the context menu. Returns an action if the user clicked an item.
/// Call this from the main `update()` method after all panels.
pub fn render(
    ctx: &egui::Context,
    state: &mut ContextMenuState,
    can_go_back: bool,
    can_go_forward: bool,
) -> Option<ContextMenuAction> {
    if !state.open {
        return None;
    }

    let pos = match state.position {
        Some(p) => p,
        None => {
            state.open = false;
            return None;
        }
    };

    // Close menu on left-click outside or Escape
    let close_requested = ctx.input(|i| {
        i.key_pressed(egui::Key::Escape)
            || (i.pointer.button_clicked(egui::PointerButton::Primary)
                && !i.pointer.hover_pos()
                    .map(|p| {
                        let menu_rect = Rect::from_min_size(pos, Vec2::new(MENU_WIDTH + MENU_PADDING * 2.0, 300.0));
                        menu_rect.contains(p)
                    })
                    .unwrap_or(false))
    });

    if close_requested {
        state.open = false;
        return None;
    }

    let items = vec![
        MenuItem::Action {
            label: "Zurueck",
            shortcut: "Alt+Left",
            action: ContextMenuAction::Back,
            enabled: can_go_back,
        },
        MenuItem::Action {
            label: "Vorwaerts",
            shortcut: "Alt+Right",
            action: ContextMenuAction::Forward,
            enabled: can_go_forward,
        },
        MenuItem::Action {
            label: "Neu laden",
            shortcut: "F5",
            action: ContextMenuAction::Reload,
            enabled: true,
        },
        MenuItem::Separator,
        MenuItem::Action {
            label: "Ausschneiden",
            shortcut: "Ctrl+X",
            action: ContextMenuAction::Cut,
            enabled: true,
        },
        MenuItem::Action {
            label: "Kopieren",
            shortcut: "Ctrl+C",
            action: ContextMenuAction::Copy,
            enabled: true,
        },
        MenuItem::Action {
            label: "Einfuegen",
            shortcut: "Ctrl+V",
            action: ContextMenuAction::Paste,
            enabled: true,
        },
        MenuItem::Separator,
        MenuItem::Action {
            label: "Alles auswaehlen",
            shortcut: "Ctrl+A",
            action: ContextMenuAction::SelectAll,
            enabled: true,
        },
        MenuItem::Separator,
        MenuItem::Action {
            label: "Seitenquelltext anzeigen",
            shortcut: "Ctrl+U",
            action: ContextMenuAction::ViewSource,
            enabled: true,
        },
    ];

    // Calculate total menu height
    let total_height: f32 = items.iter().map(|item| match item {
        MenuItem::Action { .. } => ITEM_HEIGHT,
        MenuItem::Separator => SEPARATOR_HEIGHT,
    }).sum::<f32>() + MENU_PADDING * 2.0;

    let mut result = None;

    // Render as an overlay area
    let layer_id = LayerId::new(Order::Foreground, Id::new("context_menu_layer"));
    let painter = ctx.layer_painter(layer_id);

    let menu_rect = Rect::from_min_size(pos, Vec2::new(MENU_WIDTH, total_height));

    // Shadow
    let shadow_rect = menu_rect.expand(2.0);
    painter.rect_filled(
        shadow_rect,
        CornerRadius::same(MENU_ROUNDING as u8 + 2),
        Color32::from_rgba_unmultiplied(0, 0, 0, 80),
    );

    // Background
    painter.rect_filled(
        menu_rect,
        CornerRadius::same(MENU_ROUNDING as u8),
        Color32::from_rgb(38, 38, 48),
    );

    // Border
    let border_points = [
        menu_rect.left_top(), menu_rect.right_top(),
        menu_rect.right_bottom(), menu_rect.left_bottom(),
        menu_rect.left_top(),
    ];
    for pair in border_points.windows(2) {
        painter.line_segment([pair[0], pair[1]], Stroke::new(1.0, Color32::from_rgb(60, 60, 72)));
    }

    // Render items
    let mouse_pos = ctx.input(|i| i.pointer.hover_pos());
    let left_clicked = ctx.input(|i| i.pointer.button_clicked(egui::PointerButton::Primary));

    let mut y = menu_rect.min.y + MENU_PADDING;

    for item in &items {
        match item {
            MenuItem::Action { label, shortcut, action, enabled } => {
                let item_rect = Rect::from_min_size(
                    Pos2::new(menu_rect.min.x, y),
                    Vec2::new(MENU_WIDTH, ITEM_HEIGHT),
                );
                let hovered = *enabled && mouse_pos
                    .map(|p| item_rect.contains(p))
                    .unwrap_or(false);

                // Hover highlight
                if hovered {
                    painter.rect_filled(
                        item_rect.shrink2(Vec2::new(MENU_PADDING, 0.0)),
                        CornerRadius::same(4),
                        Color32::from_rgb(55, 90, 200),
                    );
                }

                // Label
                let label_color = if *enabled {
                    if hovered { Color32::WHITE } else { Color32::from_rgb(210, 210, 220) }
                } else {
                    Color32::from_rgb(90, 90, 100)
                };
                painter.text(
                    Pos2::new(item_rect.min.x + 12.0, item_rect.center().y),
                    egui::Align2::LEFT_CENTER,
                    label,
                    egui::FontId::proportional(13.0),
                    label_color,
                );

                // Shortcut (right-aligned)
                let shortcut_color = if *enabled {
                    Color32::from_rgb(120, 120, 135)
                } else {
                    Color32::from_rgb(70, 70, 80)
                };
                painter.text(
                    Pos2::new(item_rect.max.x - 12.0, item_rect.center().y),
                    egui::Align2::RIGHT_CENTER,
                    shortcut,
                    egui::FontId::proportional(11.0),
                    shortcut_color,
                );

                // Click handler
                if hovered && left_clicked {
                    result = Some(*action);
                    state.open = false;
                }

                y += ITEM_HEIGHT;
            }
            MenuItem::Separator => {
                let sep_y = y + SEPARATOR_HEIGHT / 2.0;
                painter.line_segment(
                    [
                        Pos2::new(menu_rect.min.x + 8.0, sep_y),
                        Pos2::new(menu_rect.max.x - 8.0, sep_y),
                    ],
                    Stroke::new(1.0, Color32::from_rgb(55, 55, 65)),
                );
                y += SEPARATOR_HEIGHT;
            }
        }
    }

    result
}
