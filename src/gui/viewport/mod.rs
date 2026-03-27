//! CEF frame buffer rendering, input event forwarding, and key code translation.
//!
//! Splits into three concerns: viewport state & input types (`types`),
//! the egui rendering loop (`renderer`), and the egui-to-VK key mapping
//! (`key_mapping`).

mod key_mapping;
mod renderer;
mod types;

pub use renderer::render;
pub use types::{bump_frame_version, ViewportInput, ViewportState};
