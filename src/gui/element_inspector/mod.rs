//! Element-Inspector module: DOM property inspection in a separate OS window.
//!
//! Provides types for element property definitions, shared inspector state,
//! and a standalone renderer for `show_viewport_deferred`. External callers
//! import `ElementInspectorState`, `ElementDetails`, and `render_standalone`.

mod renderer;
mod types;

pub use renderer::render_standalone;
pub use types::{ElementDetails, ElementInspectorState, ElementProperty, InspectorConfig};
