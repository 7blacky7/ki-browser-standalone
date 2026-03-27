//! DevTools as a separate OS window — module re-exports.
//!
//! Splits the 1134-line monolithic `devtools.rs` into focused sub-modules:
//! - `types`: enums, structs, type aliases (Section, VisionTactic, OCR types, DevToolsAction)
//! - `state`: Arc-wrapped DevToolsState and DevToolsShared for cross-thread sharing
//! - `window`: render_standalone entry point for the deferred OS viewport
//! - render sub-modules: one file per DevTools section (page_info, source, vision, ocr, tabs)
//!
//! External code imports everything through this module root unchanged.

pub mod state;
pub mod types;
pub mod window;

mod render_ocr;
mod render_page_info;
mod render_source;
mod render_tabs;
mod render_vision;

// Re-export everything that external code needs
pub use state::*;
pub use types::*;
pub use window::render_standalone;
