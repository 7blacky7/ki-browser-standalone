//! Tab bar widget with clickable tabs, close buttons, and drag-to-reorder.
//!
//! Renders a horizontal tab strip where each tab is selectable by clicking its
//! title area. Each tab has an "x" close button on the right. A "+" button at
//! the end creates new tabs. Tabs can be reordered by dragging.

mod painting;
mod renderer;
mod types;

pub use renderer::render;
pub use types::{TabBarAction, TabInfo};
