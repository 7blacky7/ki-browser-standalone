//! `FormHandler` struct definition — stateless JavaScript generator for HTML form automation.
//!
//! `FormHandler` is a zero-size marker struct whose associated functions generate
//! JavaScript source strings for form detection, filling, and validation.  All
//! scripts are designed to be evaluated in a browser context via
//! `DomAccessor::evaluate_js` and return JSON that can be deserialized into the
//! corresponding Rust structs defined in the `types` module.

/// Generates JavaScript for form detection, filling, and validation.
///
/// `FormHandler` does not hold any state. All methods are associated functions
/// that return JavaScript source strings. The JS is designed to be evaluated
/// in a browser context via `DomAccessor::evaluate_js` and returns JSON that
/// can be deserialized into the corresponding Rust structs.
pub struct FormHandler;
