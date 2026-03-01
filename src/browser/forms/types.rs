//! Shared data types for HTML form detection, filling, and validation.
//!
//! This module defines the core Rust structs and enums that model the
//! browser-side form DOM: individual fields, options, buttons, form metadata,
//! fill requests/results, and validation results.  All types implement
//! `Serialize` / `Deserialize` so they can round-trip through the JSON
//! produced by the JavaScript scripts generated in sibling modules.

use serde::{Deserialize, Serialize};
use std::collections::HashMap;

// ---------------------------------------------------------------------------
// Data types
// ---------------------------------------------------------------------------

/// A detected form on the page with all its fields and metadata.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FormInfo {
    /// CSS selector that uniquely identifies this form element.
    pub selector: String,
    /// The `id` attribute of the form, if present.
    pub id: Option<String>,
    /// The `name` attribute of the form, if present.
    pub name: Option<String>,
    /// The `action` URL of the form, if present.
    pub action: Option<String>,
    /// HTTP method (`GET` or `POST`). Defaults to `GET` when not specified.
    pub method: String,
    /// All detected fields inside the form.
    pub fields: Vec<FormField>,
    /// The primary submit button, if one was found.
    pub submit_button: Option<FormButton>,
    /// Whether any field in the form is a file upload (`<input type="file">`).
    pub has_file_upload: bool,
    /// Heuristic: form contains a password field and looks like a login form.
    pub is_login_form: bool,
    /// Heuristic: form contains a search field or has `role="search"`.
    pub is_search_form: bool,
    /// Heuristic: form appears to be a registration/signup form.
    pub is_registration_form: bool,
}

/// A single form field with all relevant attributes and state.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FormField {
    /// CSS selector that uniquely identifies this field.
    pub selector: String,
    /// The `name` attribute of the field, if present.
    pub name: Option<String>,
    /// The `id` attribute of the field, if present.
    pub id: Option<String>,
    /// The semantic type of the field.
    pub field_type: FieldType,
    /// Text of the associated `<label>` element, if found.
    pub label: Option<String>,
    /// The `placeholder` attribute, if present.
    pub placeholder: Option<String>,
    /// The current value of the field.
    pub current_value: String,
    /// Whether the field has the `required` attribute.
    pub required: bool,
    /// The `pattern` attribute (regex for validation), if present.
    pub pattern: Option<String>,
    /// The `min` attribute (for number/date inputs), if present.
    pub min: Option<String>,
    /// The `max` attribute (for number/date inputs), if present.
    pub max: Option<String>,
    /// The `maxlength` attribute, if present.
    pub maxlength: Option<u32>,
    /// Available options for `<select>`, radio groups, or `<datalist>`.
    pub options: Vec<FieldOption>,
    /// Whether the field is visible on the page.
    pub is_visible: bool,
    /// Whether the field is disabled.
    pub is_disabled: bool,
    /// Whether the field is read-only.
    pub is_readonly: bool,
    /// The `aria-label` attribute, if present.
    pub aria_label: Option<String>,
    /// The `autocomplete` attribute hint, if present.
    pub autocomplete: Option<String>,
}

/// Semantic type of a form field.
///
/// This covers all standard HTML input types as well as `<select>` and
/// `<textarea>`. Unknown type strings are preserved in the `Unknown` variant.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum FieldType {
    /// `<input type="text">` or unspecified type (defaults to text).
    Text,
    /// `<input type="email">`.
    Email,
    /// `<input type="password">`.
    Password,
    /// `<input type="number">`.
    Number,
    /// `<input type="tel">`.
    Tel,
    /// `<input type="url">`.
    Url,
    /// `<input type="search">`.
    Search,
    /// `<input type="date">`.
    Date,
    /// `<input type="datetime-local">`.
    DateTime,
    /// `<input type="time">`.
    Time,
    /// `<input type="month">`.
    Month,
    /// `<input type="week">`.
    Week,
    /// `<input type="color">`.
    Color,
    /// `<input type="range">`.
    Range,
    /// `<input type="file">`.
    File,
    /// `<input type="hidden">`.
    Hidden,
    /// `<input type="checkbox">`.
    Checkbox,
    /// `<input type="radio">`.
    Radio,
    /// `<select>` element.
    Select,
    /// `<textarea>` element.
    Textarea,
    /// An unrecognized input type string.
    Unknown(String),
}

/// A single option inside a `<select>`, radio group, or `<datalist>`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FieldOption {
    /// The `value` attribute of the option.
    pub value: String,
    /// The visible label text of the option.
    pub label: String,
    /// Whether this option is currently selected.
    pub selected: bool,
    /// Whether this option is disabled.
    pub disabled: bool,
}

/// A submit button associated with a form.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FormButton {
    /// CSS selector that uniquely identifies this button.
    pub selector: String,
    /// Visible text content of the button.
    pub text: String,
    /// The `type` attribute (`submit`, `button`, etc.).
    pub button_type: String,
}

/// Instructions for filling a form with data.
///
/// Keys in `data` are matched against field name, id, label text,
/// placeholder text, aria-label, or autocomplete attribute (in that order).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FormFillRequest {
    /// CSS selector for the form to fill. If `None`, the first form on the page is used.
    pub form_selector: Option<String>,
    /// Field values keyed by field identifier (name, id, label, placeholder,
    /// aria-label, autocomplete, or CSS selector).
    pub data: HashMap<String, serde_json::Value>,
    /// If `true`, text is typed character-by-character with small random delays.
    pub human_like: bool,
    /// If `true`, the form's submit button is clicked after filling.
    pub submit: bool,
    /// If `true`, existing field values are cleared before filling.
    pub clear_first: bool,
}

/// Result of a form fill operation.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FormFillResult {
    /// Names/identifiers of fields that were successfully filled.
    pub filled_fields: Vec<String>,
    /// Fields that could not be filled, with reasons.
    pub failed_fields: Vec<FormFillError>,
    /// Whether the form was submitted after filling.
    pub submitted: bool,
}

/// Describes a single field fill failure.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FormFillError {
    /// The key from `FormFillRequest::data` that failed.
    pub field: String,
    /// Human-readable reason for the failure.
    pub reason: String,
}

/// Validation error for a single form field.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ValidationError {
    /// CSS selector of the field that failed validation.
    pub selector: String,
    /// The field name or id for identification.
    pub field: String,
    /// The validation error message from the browser.
    pub message: String,
}

/// Validation result for an entire form.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FormValidationResult {
    /// Whether the entire form passes HTML5 validation.
    pub is_valid: bool,
    /// Per-field validation errors (empty when `is_valid` is `true`).
    pub errors: Vec<ValidationError>,
}

// ---------------------------------------------------------------------------
// FieldType helpers
// ---------------------------------------------------------------------------

impl FieldType {
    /// Returns `true` if this field type accepts free-form text input.
    pub fn is_text_like(&self) -> bool {
        matches!(
            self,
            FieldType::Text
                | FieldType::Email
                | FieldType::Password
                | FieldType::Number
                | FieldType::Tel
                | FieldType::Url
                | FieldType::Search
                | FieldType::Textarea
        )
    }

    /// Returns `true` if this field type represents a date or time input.
    pub fn is_temporal(&self) -> bool {
        matches!(
            self,
            FieldType::Date
                | FieldType::DateTime
                | FieldType::Time
                | FieldType::Month
                | FieldType::Week
        )
    }

    /// Returns `true` if this field type uses discrete options rather than
    /// free-form input (select, radio, checkbox).
    pub fn is_choice(&self) -> bool {
        matches!(
            self,
            FieldType::Checkbox | FieldType::Radio | FieldType::Select
        )
    }
}

impl std::fmt::Display for FieldType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            FieldType::Text => write!(f, "text"),
            FieldType::Email => write!(f, "email"),
            FieldType::Password => write!(f, "password"),
            FieldType::Number => write!(f, "number"),
            FieldType::Tel => write!(f, "tel"),
            FieldType::Url => write!(f, "url"),
            FieldType::Search => write!(f, "search"),
            FieldType::Date => write!(f, "date"),
            FieldType::DateTime => write!(f, "datetime-local"),
            FieldType::Time => write!(f, "time"),
            FieldType::Month => write!(f, "month"),
            FieldType::Week => write!(f, "week"),
            FieldType::Color => write!(f, "color"),
            FieldType::Range => write!(f, "range"),
            FieldType::File => write!(f, "file"),
            FieldType::Hidden => write!(f, "hidden"),
            FieldType::Checkbox => write!(f, "checkbox"),
            FieldType::Radio => write!(f, "radio"),
            FieldType::Select => write!(f, "select"),
            FieldType::Textarea => write!(f, "textarea"),
            FieldType::Unknown(s) => write!(f, "{}", s),
        }
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_field_type_is_text_like() {
        assert!(FieldType::Text.is_text_like());
        assert!(FieldType::Email.is_text_like());
        assert!(FieldType::Password.is_text_like());
        assert!(FieldType::Number.is_text_like());
        assert!(FieldType::Tel.is_text_like());
        assert!(FieldType::Url.is_text_like());
        assert!(FieldType::Search.is_text_like());
        assert!(FieldType::Textarea.is_text_like());
        assert!(!FieldType::Checkbox.is_text_like());
        assert!(!FieldType::Radio.is_text_like());
        assert!(!FieldType::Select.is_text_like());
        assert!(!FieldType::File.is_text_like());
        assert!(!FieldType::Hidden.is_text_like());
        assert!(!FieldType::Date.is_text_like());
        assert!(!FieldType::Color.is_text_like());
        assert!(!FieldType::Range.is_text_like());
        assert!(!FieldType::Unknown("custom".to_string()).is_text_like());
    }

    #[test]
    fn test_field_type_is_temporal() {
        assert!(FieldType::Date.is_temporal());
        assert!(FieldType::DateTime.is_temporal());
        assert!(FieldType::Time.is_temporal());
        assert!(FieldType::Month.is_temporal());
        assert!(FieldType::Week.is_temporal());
        assert!(!FieldType::Text.is_temporal());
        assert!(!FieldType::Number.is_temporal());
        assert!(!FieldType::Select.is_temporal());
    }

    #[test]
    fn test_field_type_is_choice() {
        assert!(FieldType::Checkbox.is_choice());
        assert!(FieldType::Radio.is_choice());
        assert!(FieldType::Select.is_choice());
        assert!(!FieldType::Text.is_choice());
        assert!(!FieldType::Email.is_choice());
        assert!(!FieldType::File.is_choice());
    }

    #[test]
    fn test_field_type_display() {
        assert_eq!(FieldType::Text.to_string(), "text");
        assert_eq!(FieldType::Email.to_string(), "email");
        assert_eq!(FieldType::Password.to_string(), "password");
        assert_eq!(FieldType::DateTime.to_string(), "datetime-local");
        assert_eq!(FieldType::Textarea.to_string(), "textarea");
        assert_eq!(FieldType::Select.to_string(), "select");
        assert_eq!(
            FieldType::Unknown("custom-widget".to_string()).to_string(),
            "custom-widget"
        );
    }

    #[test]
    fn test_form_info_serialization() {
        let form = FormInfo {
            selector: "form#login".to_string(),
            id: Some("login".to_string()),
            name: None,
            action: Some("/auth/login".to_string()),
            method: "POST".to_string(),
            fields: vec![],
            submit_button: Some(FormButton {
                selector: "button#submit".to_string(),
                text: "Sign In".to_string(),
                button_type: "submit".to_string(),
            }),
            has_file_upload: false,
            is_login_form: true,
            is_search_form: false,
            is_registration_form: false,
        };
        let json = serde_json::to_string(&form).expect("serialize FormInfo");
        assert!(json.contains("\"is_login_form\":true"));
        assert!(json.contains("\"method\":\"POST\""));
        assert!(json.contains("\"action\":\"/auth/login\""));
        let deserialized: FormInfo = serde_json::from_str(&json).expect("deserialize FormInfo");
        assert_eq!(deserialized.selector, "form#login");
        assert!(deserialized.is_login_form);
        assert!(!deserialized.is_search_form);
    }

    #[test]
    fn test_form_field_serialization() {
        let field = FormField {
            selector: "#email".to_string(),
            name: Some("email".to_string()),
            id: Some("email".to_string()),
            field_type: FieldType::Email,
            label: Some("Email Address".to_string()),
            placeholder: Some("you@example.com".to_string()),
            current_value: String::new(),
            required: true,
            pattern: None,
            min: None,
            max: None,
            maxlength: Some(255),
            options: vec![],
            is_visible: true,
            is_disabled: false,
            is_readonly: false,
            aria_label: None,
            autocomplete: Some("email".to_string()),
        };
        let json = serde_json::to_string(&field).expect("serialize FormField");
        assert!(json.contains("\"required\":true"));
        assert!(json.contains("\"maxlength\":255"));
        let deserialized: FormField = serde_json::from_str(&json).expect("deserialize FormField");
        assert_eq!(deserialized.field_type, FieldType::Email);
        assert_eq!(deserialized.label, Some("Email Address".to_string()));
    }

    #[test]
    fn test_field_option_serialization() {
        let option = FieldOption {
            value: "us".to_string(),
            label: "United States".to_string(),
            selected: true,
            disabled: false,
        };
        let json = serde_json::to_string(&option).expect("serialize FieldOption");
        let deserialized: FieldOption = serde_json::from_str(&json).expect("deserialize FieldOption");
        assert_eq!(deserialized.value, "us");
        assert!(deserialized.selected);
    }

    #[test]
    fn test_form_fill_result_serialization() {
        let result = FormFillResult {
            filled_fields: vec!["username".to_string(), "password".to_string()],
            failed_fields: vec![FormFillError {
                field: "captcha".to_string(),
                reason: "No matching field found".to_string(),
            }],
            submitted: true,
        };
        let json = serde_json::to_string(&result).expect("serialize FormFillResult");
        let deserialized: FormFillResult = serde_json::from_str(&json).expect("deserialize FormFillResult");
        assert_eq!(deserialized.filled_fields.len(), 2);
        assert_eq!(deserialized.failed_fields.len(), 1);
        assert!(deserialized.submitted);
    }

    #[test]
    fn test_validation_result_serialization() {
        let result = FormValidationResult {
            is_valid: false,
            errors: vec![
                ValidationError {
                    selector: "#email".to_string(),
                    field: "email".to_string(),
                    message: "This field is required".to_string(),
                },
                ValidationError {
                    selector: "#age".to_string(),
                    field: "age".to_string(),
                    message: "Value is too low (min 18)".to_string(),
                },
            ],
        };
        let json = serde_json::to_string(&result).expect("serialize FormValidationResult");
        let deserialized: FormValidationResult = serde_json::from_str(&json).expect("deserialize FormValidationResult");
        assert!(!deserialized.is_valid);
        assert_eq!(deserialized.errors.len(), 2);
    }

    #[test]
    fn test_form_fill_request_serialization() {
        let mut data = HashMap::new();
        data.insert("name".to_string(), serde_json::json!("John Doe"));
        data.insert("age".to_string(), serde_json::json!(30));
        data.insert("agree".to_string(), serde_json::json!(true));
        let request = FormFillRequest {
            form_selector: Some("form#profile".to_string()),
            data,
            human_like: true,
            submit: false,
            clear_first: true,
        };
        let json = serde_json::to_string(&request).expect("serialize FormFillRequest");
        let deserialized: FormFillRequest = serde_json::from_str(&json).expect("deserialize FormFillRequest");
        assert_eq!(deserialized.form_selector, Some("form#profile".to_string()));
        assert!(deserialized.human_like);
        assert!(!deserialized.submit);
        assert!(deserialized.clear_first);
        assert_eq!(deserialized.data.len(), 3);
        assert_eq!(deserialized.data.get("name"), Some(&serde_json::json!("John Doe")));
    }
}
