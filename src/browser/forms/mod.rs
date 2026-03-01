//! Form detection, analysis, and auto-fill for browser automation.
//!
//! This module provides structures and logic for detecting forms on web pages,
//! analyzing their fields and purpose (login, search, registration), and
//! generating JavaScript to intelligently fill them. It is designed for use
//! by AI agents that need to interact with forms without prior knowledge of
//! page structure.
//!
//! # Overview
//!
//! The module works by generating JavaScript that runs in the browser context:
//!
//! - [`FormHandler::detect_forms_script`] produces JS that scans the page for
//!   all `<form>` elements, analyzes their fields, labels, and purpose, and
//!   returns a JSON array of [`FormInfo`] structs.
//! - [`FormHandler::fill_form_script`] produces JS that matches a
//!   [`FormFillRequest`]'s data keys to form fields (by name, id, label,
//!   placeholder, aria-label, or autocomplete attribute) and fills them with
//!   proper DOM event dispatch.
//! - [`FormHandler::fill_form_human_like_script`] produces an async JS Promise
//!   that types characters one-by-one with random delays to evade bot detection.
//! - [`FormHandler::validate_form_script`] produces JS that checks HTML5
//!   constraint validation on a given form and returns per-field errors.
//!
//! # Submodules
//!
//! - [`types`]    — shared data structs and enums (FormInfo, FormField, FieldType, …)
//! - [`handler`]  — FormHandler struct definition
//! - [`detect`]   — JavaScript generation for form detection and analysis
//! - [`fill`]     — JavaScript generation for form filling (instant and human-like)
//! - [`validate`] — JavaScript generation for HTML5 constraint validation

mod detect;
mod fill;
mod handler;
mod types;
mod validate;

// Re-export all public types and the handler so external code can use
// `use crate::browser::forms::FormHandler` etc. unchanged.
pub use handler::FormHandler;
pub use types::{
    FieldOption, FieldType, FormButton, FormField, FormFillError, FormFillRequest, FormFillResult,
    FormInfo, FormValidationResult, ValidationError,
};
