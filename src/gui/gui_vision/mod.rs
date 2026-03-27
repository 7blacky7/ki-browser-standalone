//! Direct vision tactic execution and frame buffer utilities for the GUI.
//!
//! Contains helpers that run vision image/text tactics directly through the
//! CEF engine (no REST API round-trip), DOM snapshot capture, and BGRA frame
//! buffer to PNG conversion for OCR and screenshot annotation.
//!
//! All functions are standalone (no `self`) so they can be called from
//! background threads spawned by `KiBrowserApp::handle_devtools_actions`.

mod frame_buffer;
mod image_tactics;
mod js_execution;
mod text_tactics;

pub(super) use frame_buffer::{draw_ocr_bounding_boxes, frame_buffer_to_png};
pub(super) use image_tactics::run_vision_image_direct;
pub(super) use text_tactics::run_vision_text_direct;
