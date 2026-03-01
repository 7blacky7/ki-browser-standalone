//! GUI Browser module with CEF rendering and egui UI.

pub mod browser_app;
pub mod tab_bar;
pub mod toolbar;
pub mod viewport;
pub mod status_bar;

pub use browser_app::run_gui;
