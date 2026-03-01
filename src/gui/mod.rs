//! GUI Browser module with CEF rendering and egui UI.
//!
//! Provides the eframe/egui-based browser window, tab management, viewport
//! rendering, and the shared `GuiHandle` for cross-thread visibility control
//! and graceful shutdown coordination.

pub mod browser_app;
pub mod context_menu;
pub mod devtools;
pub mod handle;
pub mod tab_bar;
pub mod title_bar;
pub mod toolbar;
pub mod viewport;
pub mod status_bar;
pub mod vision_overlay;

pub use browser_app::run_gui;
pub use handle::{GuiHandle, GuiVisibility};
