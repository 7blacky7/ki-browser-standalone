//! Vision overlay system for annotating the browser viewport with detected elements.
//!
//! Provides colored bounding boxes over interactive/visible DOM elements,
//! hit testing for right-click inspection, and OCR text enrichment.
//! Each vision tactic (VisionLabels, DomAnnotate, DomSnapshot, Forms) has
//! a dedicated JSON parser that converts the API response into `OverlayElement`
//! values rendered by the egui-based viewport renderer.

pub mod types;
pub mod renderer;
pub mod parsers;
pub mod ocr_enrichment;

pub use types::*;
pub use renderer::{hit_test, render_overlay};
pub use parsers::*;
pub use ocr_enrichment::{enrich_with_ocr, trigger_ocr_enrichment};
