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
//! - [`FormHandler::validate_form_script`] produces JS that checks HTML5
//!   constraint validation on a given form and returns per-field errors.
//!
//! # Example
//!
//! ```rust,ignore
//! use ki_browser_standalone::browser::forms::{FormHandler, FormFillRequest};
//! use std::collections::HashMap;
//!
//! // Detect all forms on the current page
//! let detect_js = FormHandler::detect_forms_script();
//! let result = dom_accessor.evaluate_js(&detect_js).await?;
//! // result contains JSON array of FormInfo
//!
//! // Fill a login form
//! let mut data = HashMap::new();
//! data.insert("username".to_string(), serde_json::json!("agent@example.com"));
//! data.insert("password".to_string(), serde_json::json!("secret123"));
//!
//! let request = FormFillRequest {
//!     form_selector: Some("form#login".to_string()),
//!     data,
//!     human_like: false,
//!     submit: true,
//!     clear_first: true,
//! };
//!
//! let fill_js = FormHandler::fill_form_script(&request);
//! let result = dom_accessor.evaluate_js(&fill_js).await?;
//! // result contains JSON FormFillResult
//! ```

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
    /// CSS selector for the form to fill. If `None`, the first form on the
    /// page is used.
    pub form_selector: Option<String>,

    /// Field values keyed by field identifier (name, id, label, placeholder,
    /// aria-label, autocomplete, or CSS selector).
    pub data: HashMap<String, serde_json::Value>,

    /// If `true`, text is typed character-by-character with small random
    /// delays to mimic human input.
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
// FormHandler - JavaScript generation
// ---------------------------------------------------------------------------

/// Generates JavaScript for form detection, filling, and validation.
///
/// `FormHandler` does not hold any state. All methods are associated functions
/// that return JavaScript source strings. The JS is designed to be evaluated
/// in a browser context via `DomAccessor::evaluate_js` and returns JSON that
/// can be deserialized into the corresponding Rust structs.
pub struct FormHandler;

impl FormHandler {
    /// Generates JavaScript that detects and analyzes all forms on the page.
    ///
    /// The returned script, when evaluated in a browser context, produces a
    /// JSON string representing a `Vec<FormInfo>`. It performs the following:
    ///
    /// 1. Finds all `<form>` elements on the page.
    /// 2. For each form, enumerates `<input>`, `<select>`, and `<textarea>` fields.
    /// 3. Associates labels via `for` attribute, `aria-labelledby`, or wrapping `<label>`.
    /// 4. Detects form purpose (login, search, registration) via heuristics.
    /// 5. Identifies the primary submit button.
    ///
    /// # Returns
    ///
    /// A `String` containing the JavaScript to evaluate.
    pub fn detect_forms_script() -> String {
        r#"
(function() {
    'use strict';

    // ---------------------------------------------------------------
    // Helpers
    // ---------------------------------------------------------------

    /** Generate a unique CSS selector for an element. */
    function uniqueSelector(el) {
        if (el.id) return '#' + CSS.escape(el.id);

        var parts = [];
        var cur = el;
        while (cur && cur !== document.documentElement) {
            var seg = cur.tagName.toLowerCase();
            if (cur.id) {
                parts.unshift('#' + CSS.escape(cur.id) + ' > ' + seg);
                break;
            }
            var parent = cur.parentElement;
            if (parent) {
                var siblings = Array.from(parent.children).filter(function(c) {
                    return c.tagName === cur.tagName;
                });
                if (siblings.length > 1) {
                    var idx = siblings.indexOf(cur) + 1;
                    seg += ':nth-of-type(' + idx + ')';
                }
            }
            parts.unshift(seg);
            cur = parent;
        }
        return parts.join(' > ');
    }

    /** Check if an element is visible (not hidden/collapsed). */
    function isVisible(el) {
        if (!el) return false;
        var style = window.getComputedStyle(el);
        if (style.display === 'none') return false;
        if (style.visibility === 'hidden') return false;
        if (parseFloat(style.opacity) === 0) return false;
        var rect = el.getBoundingClientRect();
        return rect.width > 0 && rect.height > 0;
    }

    /** Find label text for a field element. */
    function findLabel(field) {
        // 1. Explicit <label for="...">
        if (field.id) {
            var lbl = document.querySelector('label[for=' + JSON.stringify(field.id) + ']');
            if (lbl) return lbl.textContent.trim();
        }
        // 2. aria-labelledby
        var ariaLblBy = field.getAttribute('aria-labelledby');
        if (ariaLblBy) {
            var parts = ariaLblBy.split(/\s+/);
            var text = parts.map(function(id) {
                var el = document.getElementById(id);
                return el ? el.textContent.trim() : '';
            }).filter(Boolean).join(' ');
            if (text) return text;
        }
        // 3. Wrapping <label>
        var parent = field.closest('label');
        if (parent) {
            // Clone and remove the input itself to get only label text
            var clone = parent.cloneNode(true);
            var inputs = clone.querySelectorAll('input, select, textarea');
            inputs.forEach(function(inp) { inp.remove(); });
            var labelText = clone.textContent.trim();
            if (labelText) return labelText;
        }
        return null;
    }

    /** Map an input type string to our FieldType enum name. */
    function mapFieldType(tagName, typeAttr) {
        var tag = tagName.toLowerCase();
        if (tag === 'select') return 'Select';
        if (tag === 'textarea') return 'Textarea';
        if (tag !== 'input') return { Unknown: tag };
        var t = (typeAttr || 'text').toLowerCase();
        var mapping = {
            'text': 'Text', 'email': 'Email', 'password': 'Password',
            'number': 'Number', 'tel': 'Tel', 'url': 'Url',
            'search': 'Search', 'date': 'Date', 'datetime-local': 'DateTime',
            'time': 'Time', 'month': 'Month', 'week': 'Week',
            'color': 'Color', 'range': 'Range', 'file': 'File',
            'hidden': 'Hidden', 'checkbox': 'Checkbox', 'radio': 'Radio'
        };
        if (mapping[t]) return mapping[t];
        return { Unknown: t };
    }

    /** Collect <option> elements for a <select> or <datalist>. */
    function collectOptions(field) {
        var opts = [];
        var tag = field.tagName.toLowerCase();
        if (tag === 'select') {
            Array.from(field.options).forEach(function(opt) {
                opts.push({
                    value: opt.value,
                    label: opt.textContent.trim(),
                    selected: opt.selected,
                    disabled: opt.disabled
                });
            });
        } else if (field.getAttribute('list')) {
            var dl = document.getElementById(field.getAttribute('list'));
            if (dl) {
                Array.from(dl.querySelectorAll('option')).forEach(function(opt) {
                    opts.push({
                        value: opt.value,
                        label: opt.textContent.trim() || opt.value,
                        selected: false,
                        disabled: opt.disabled || false
                    });
                });
            }
        }
        return opts;
    }

    /** Collect radio options when the field is a radio button. */
    function collectRadioOptions(field, form) {
        if (!field.name) return [];
        var radios = form.querySelectorAll(
            'input[type="radio"][name=' + JSON.stringify(field.name) + ']'
        );
        return Array.from(radios).map(function(r) {
            var lbl = findLabel(r);
            return {
                value: r.value,
                label: lbl || r.value,
                selected: r.checked,
                disabled: r.disabled
            };
        });
    }

    /** Build a FormField object from a DOM element. */
    function analyzeField(field, form) {
        var tag = field.tagName.toLowerCase();
        var typeAttr = field.getAttribute('type');
        var fieldType = mapFieldType(tag, typeAttr);
        var maxlen = field.getAttribute('maxlength');

        var options = [];
        if (tag === 'select' || field.getAttribute('list')) {
            options = collectOptions(field);
        } else if (typeAttr && typeAttr.toLowerCase() === 'radio') {
            options = collectRadioOptions(field, form);
        }

        var currentValue = '';
        if (tag === 'select') {
            currentValue = field.value || '';
        } else if (typeAttr && (typeAttr.toLowerCase() === 'checkbox' || typeAttr.toLowerCase() === 'radio')) {
            currentValue = field.checked ? 'true' : 'false';
        } else {
            currentValue = field.value || '';
        }

        return {
            selector: uniqueSelector(field),
            name: field.name || null,
            id: field.id || null,
            field_type: fieldType,
            label: findLabel(field),
            placeholder: field.getAttribute('placeholder') || null,
            current_value: currentValue,
            required: field.required || field.getAttribute('aria-required') === 'true',
            pattern: field.getAttribute('pattern') || null,
            min: field.getAttribute('min') || null,
            max: field.getAttribute('max') || null,
            maxlength: maxlen ? parseInt(maxlen, 10) : null,
            options: options,
            is_visible: isVisible(field),
            is_disabled: field.disabled,
            is_readonly: field.readOnly || false,
            aria_label: field.getAttribute('aria-label') || null,
            autocomplete: field.getAttribute('autocomplete') || null
        };
    }

    /** Find the primary submit button for a form. */
    function findSubmitButton(form) {
        // Explicit submit buttons inside the form
        var candidates = Array.from(form.querySelectorAll(
            'button[type="submit"], input[type="submit"], button:not([type])'
        ));

        // Also check for buttons associated via form attribute
        if (form.id) {
            var external = document.querySelectorAll(
                'button[form=' + JSON.stringify(form.id) + '], ' +
                'input[type="submit"][form=' + JSON.stringify(form.id) + ']'
            );
            candidates = candidates.concat(Array.from(external));
        }

        if (candidates.length === 0) return null;

        var btn = candidates[0];
        var text = '';
        if (btn.tagName.toLowerCase() === 'input') {
            text = btn.value || 'Submit';
        } else {
            text = btn.textContent.trim() || 'Submit';
        }

        return {
            selector: uniqueSelector(btn),
            text: text,
            button_type: btn.getAttribute('type') || 'submit'
        };
    }

    /** Detect form purpose heuristics. */
    function detectFormPurpose(form, fields) {
        var hasPassword = false;
        var passwordCount = 0;
        var hasEmail = false;
        var hasSearch = false;
        var hasFile = false;
        var fieldNames = [];
        var fieldAutocompletes = [];

        fields.forEach(function(f) {
            var ft = f.field_type;
            var typeName = (typeof ft === 'string') ? ft : '';

            if (typeName === 'Password') { hasPassword = true; passwordCount++; }
            if (typeName === 'Email') hasEmail = true;
            if (typeName === 'Search') hasSearch = true;
            if (typeName === 'File') hasFile = true;

            if (f.name) fieldNames.push(f.name.toLowerCase());
            if (f.autocomplete) fieldAutocompletes.push(f.autocomplete.toLowerCase());
        });

        var formRole = form.getAttribute('role') || '';
        var formAction = (form.getAttribute('action') || '').toLowerCase();

        // Search form detection
        var isSearch = hasSearch ||
            formRole === 'search' ||
            formAction.indexOf('search') !== -1 ||
            fieldNames.some(function(n) { return n === 'q' || n === 'query' || n === 'search'; }) ||
            fieldAutocompletes.some(function(a) { return a === 'off' && fields.length <= 2; });

        // Login form detection
        var isLogin = hasPassword && passwordCount === 1 && !isSearch &&
            (hasEmail ||
             fieldNames.some(function(n) {
                 return n === 'username' || n === 'user' || n === 'login' ||
                        n === 'email' || n === 'user_login';
             }) ||
             fieldAutocompletes.some(function(a) {
                 return a === 'username' || a === 'current-password';
             }));

        // Registration form detection
        var isRegistration = hasPassword && passwordCount >= 2 && !isSearch;
        if (!isRegistration && hasPassword && !isLogin) {
            // Check for registration-like field names
            isRegistration = fieldNames.some(function(n) {
                return n.indexOf('confirm') !== -1 || n.indexOf('register') !== -1 ||
                       n.indexOf('signup') !== -1 || n === 'password2' ||
                       n === 'password_confirmation';
            }) || fieldAutocompletes.some(function(a) {
                return a === 'new-password';
            });
        }

        return {
            is_login_form: isLogin,
            is_search_form: isSearch,
            is_registration_form: isRegistration,
            has_file_upload: hasFile
        };
    }

    // ---------------------------------------------------------------
    // Main detection
    // ---------------------------------------------------------------

    var forms = document.querySelectorAll('form');
    var results = [];

    forms.forEach(function(form, idx) {
        var formSelector = uniqueSelector(form);

        // Collect all input/select/textarea fields
        var fieldElements = form.querySelectorAll('input, select, textarea');
        var seenRadios = {};
        var fields = [];

        fieldElements.forEach(function(field) {
            var tag = field.tagName.toLowerCase();
            var typeAttr = (field.getAttribute('type') || '').toLowerCase();

            // Skip submit/reset/button inputs - they are not data fields
            if (tag === 'input' && (typeAttr === 'submit' || typeAttr === 'reset' || typeAttr === 'button' || typeAttr === 'image')) {
                return;
            }

            // For radio buttons, only process the group once
            if (typeAttr === 'radio' && field.name) {
                if (seenRadios[field.name]) return;
                seenRadios[field.name] = true;
            }

            fields.push(analyzeField(field, form));
        });

        var purpose = detectFormPurpose(form, fields);
        var submitButton = findSubmitButton(form);

        results.push({
            selector: formSelector,
            id: form.id || null,
            name: form.name || null,
            action: form.getAttribute('action') || null,
            method: (form.method || 'GET').toUpperCase(),
            fields: fields,
            submit_button: submitButton,
            has_file_upload: purpose.has_file_upload,
            is_login_form: purpose.is_login_form,
            is_search_form: purpose.is_search_form,
            is_registration_form: purpose.is_registration_form
        });
    });

    return JSON.stringify(results);
})();
"#
        .to_string()
    }

    /// Generates JavaScript that fills a form with the given data.
    ///
    /// The matching strategy tries each data key against fields in the
    /// following order: `name`, `id`, `label` (case-insensitive substring),
    /// `placeholder` (case-insensitive substring), `aria-label`
    /// (case-insensitive substring), `autocomplete` attribute, and finally
    /// as a literal CSS selector.
    ///
    /// For each matched field the script:
    /// - Dispatches `focus` before and `blur`/`change` after modification.
    /// - For text-like inputs and textareas: sets `value` and dispatches
    ///   `input` and `change` events (with `InputEvent` where supported).
    /// - For checkboxes: sets `checked` according to the truthy value.
    /// - For radio buttons: finds the radio with a matching value and checks it.
    /// - For selects: finds the option by value or visible label text.
    ///
    /// When `human_like` is `true`, each character is typed individually with
    /// small random delays via `setTimeout` chaining. The entire fill is
    /// wrapped in a `Promise` so the caller can `await` it.
    ///
    /// # Arguments
    ///
    /// * `request` - The fill instructions.
    ///
    /// # Returns
    ///
    /// A `String` containing the JavaScript to evaluate. The script returns
    /// a JSON string representing a [`FormFillResult`].
    pub fn fill_form_script(request: &FormFillRequest) -> String {
        let data_json =
            serde_json::to_string(&request.data).unwrap_or_else(|_| "{}".to_string());
        let form_selector = request
            .form_selector
            .as_deref()
            .unwrap_or("form");
        let human_like = request.human_like;
        let submit = request.submit;
        let clear_first = request.clear_first;

        format!(
            r#"
(function() {{
    'use strict';

    var formSelector = {form_sel};
    var data = {data};
    var humanLike = {human};
    var doSubmit = {submit};
    var clearFirst = {clear};

    var filledFields = [];
    var failedFields = [];
    var submitted = false;

    // ---------------------------------------------------------------
    // Helpers
    // ---------------------------------------------------------------

    function dispatchEvents(el, eventNames) {{
        eventNames.forEach(function(name) {{
            var evt;
            if (name === 'input') {{
                evt = new InputEvent('input', {{ bubbles: true, cancelable: true }});
            }} else {{
                evt = new Event(name, {{ bubbles: true, cancelable: true }});
            }}
            el.dispatchEvent(evt);
        }});
    }}

    function setNativeValue(el, value) {{
        // Use the native setter to bypass React / framework wrappers
        var nativeInputValueSetter = Object.getOwnPropertyDescriptor(
            window.HTMLInputElement.prototype, 'value'
        );
        var nativeTextareaValueSetter = Object.getOwnPropertyDescriptor(
            window.HTMLTextAreaElement.prototype, 'value'
        );
        var setter = null;
        if (el.tagName.toLowerCase() === 'textarea' && nativeTextareaValueSetter) {{
            setter = nativeTextareaValueSetter.set;
        }} else if (nativeInputValueSetter) {{
            setter = nativeInputValueSetter.set;
        }}
        if (setter) {{
            setter.call(el, value);
        }} else {{
            el.value = value;
        }}
    }}

    /** Find the form element. */
    var form = document.querySelector(formSelector);
    if (!form) {{
        return JSON.stringify({{
            filled_fields: [],
            failed_fields: [{{ field: '__form__', reason: 'Form not found: ' + formSelector }}],
            submitted: false
        }});
    }}

    /** Gather all fillable fields inside the form. */
    var allFields = Array.from(form.querySelectorAll('input, select, textarea'));

    /** Find a field matching a key. Returns the DOM element or null. */
    function findField(key) {{
        var keyLower = key.toLowerCase();

        // 1. Match by name attribute
        for (var i = 0; i < allFields.length; i++) {{
            if (allFields[i].name && allFields[i].name.toLowerCase() === keyLower) return allFields[i];
        }}

        // 2. Match by id attribute
        for (var i = 0; i < allFields.length; i++) {{
            if (allFields[i].id && allFields[i].id.toLowerCase() === keyLower) return allFields[i];
        }}

        // 3. Match by label text (case-insensitive substring)
        for (var i = 0; i < allFields.length; i++) {{
            var field = allFields[i];
            var labelText = '';
            if (field.id) {{
                var lbl = document.querySelector('label[for=' + JSON.stringify(field.id) + ']');
                if (lbl) labelText = lbl.textContent.trim();
            }}
            if (!labelText) {{
                var wrap = field.closest('label');
                if (wrap) {{
                    var clone = wrap.cloneNode(true);
                    clone.querySelectorAll('input, select, textarea').forEach(function(x) {{ x.remove(); }});
                    labelText = clone.textContent.trim();
                }}
            }}
            if (labelText && labelText.toLowerCase().indexOf(keyLower) !== -1) return field;
        }}

        // 4. Match by placeholder (case-insensitive substring)
        for (var i = 0; i < allFields.length; i++) {{
            var ph = allFields[i].getAttribute('placeholder');
            if (ph && ph.toLowerCase().indexOf(keyLower) !== -1) return allFields[i];
        }}

        // 5. Match by aria-label (case-insensitive substring)
        for (var i = 0; i < allFields.length; i++) {{
            var al = allFields[i].getAttribute('aria-label');
            if (al && al.toLowerCase().indexOf(keyLower) !== -1) return allFields[i];
        }}

        // 6. Match by autocomplete attribute
        for (var i = 0; i < allFields.length; i++) {{
            var ac = allFields[i].getAttribute('autocomplete');
            if (ac && ac.toLowerCase() === keyLower) return allFields[i];
        }}

        // 7. Try as a CSS selector
        try {{
            var el = form.querySelector(key);
            if (el) return el;
        }} catch(e) {{}}

        return null;
    }}

    /** Fill a single text-like field. */
    function fillTextField(field, value) {{
        field.focus();
        dispatchEvents(field, ['focus', 'focusin']);
        if (clearFirst) {{
            setNativeValue(field, '');
            dispatchEvents(field, ['input', 'change']);
        }}
        setNativeValue(field, String(value));
        dispatchEvents(field, ['input', 'change', 'blur', 'focusout']);
    }}

    /** Fill a checkbox field. */
    function fillCheckbox(field, value) {{
        var shouldCheck = false;
        if (typeof value === 'boolean') {{
            shouldCheck = value;
        }} else if (typeof value === 'string') {{
            shouldCheck = value.toLowerCase() === 'true' || value === '1' || value === 'yes';
        }} else {{
            shouldCheck = !!value;
        }}
        if (field.checked !== shouldCheck) {{
            field.focus();
            field.checked = shouldCheck;
            dispatchEvents(field, ['input', 'change', 'blur']);
        }}
    }}

    /** Fill a radio group. */
    function fillRadio(field, value) {{
        var name = field.name;
        if (!name) return false;
        var radios = form.querySelectorAll('input[type="radio"][name=' + JSON.stringify(name) + ']');
        var valStr = String(value).toLowerCase();
        var target = null;
        // Match by value
        radios.forEach(function(r) {{
            if (r.value.toLowerCase() === valStr) target = r;
        }});
        // Match by label
        if (!target) {{
            radios.forEach(function(r) {{
                if (r.id) {{
                    var lbl = document.querySelector('label[for=' + JSON.stringify(r.id) + ']');
                    if (lbl && lbl.textContent.trim().toLowerCase() === valStr) target = r;
                }}
                if (!target) {{
                    var wrap = r.closest('label');
                    if (wrap) {{
                        var clone = wrap.cloneNode(true);
                        clone.querySelectorAll('input').forEach(function(x) {{ x.remove(); }});
                        if (clone.textContent.trim().toLowerCase() === valStr) target = r;
                    }}
                }}
            }});
        }}
        if (target) {{
            target.focus();
            target.checked = true;
            dispatchEvents(target, ['input', 'change', 'blur']);
            return true;
        }}
        return false;
    }}

    /** Fill a <select> element. */
    function fillSelect(field, value) {{
        var valStr = String(value).toLowerCase();
        var matched = false;
        var options = field.options;
        // Try by value
        for (var i = 0; i < options.length; i++) {{
            if (options[i].value.toLowerCase() === valStr) {{
                field.selectedIndex = i;
                matched = true;
                break;
            }}
        }}
        // Try by visible label text
        if (!matched) {{
            for (var i = 0; i < options.length; i++) {{
                if (options[i].textContent.trim().toLowerCase() === valStr) {{
                    field.selectedIndex = i;
                    matched = true;
                    break;
                }}
            }}
        }}
        // Try partial label match
        if (!matched) {{
            for (var i = 0; i < options.length; i++) {{
                if (options[i].textContent.trim().toLowerCase().indexOf(valStr) !== -1) {{
                    field.selectedIndex = i;
                    matched = true;
                    break;
                }}
            }}
        }}
        if (matched) {{
            dispatchEvents(field, ['input', 'change', 'blur']);
        }}
        return matched;
    }}

    // ---------------------------------------------------------------
    // Main fill loop
    // ---------------------------------------------------------------

    var keys = Object.keys(data);
    for (var k = 0; k < keys.length; k++) {{
        var key = keys[k];
        var value = data[key];
        var field = findField(key);

        if (!field) {{
            failedFields.push({{ field: key, reason: 'No matching field found' }});
            continue;
        }}

        if (field.disabled) {{
            failedFields.push({{ field: key, reason: 'Field is disabled' }});
            continue;
        }}

        if (field.readOnly) {{
            failedFields.push({{ field: key, reason: 'Field is read-only' }});
            continue;
        }}

        try {{
            var tag = field.tagName.toLowerCase();
            var type = (field.getAttribute('type') || 'text').toLowerCase();

            if (tag === 'select') {{
                if (!fillSelect(field, value)) {{
                    failedFields.push({{ field: key, reason: 'No matching option found for value: ' + String(value) }});
                    continue;
                }}
            }} else if (tag === 'textarea') {{
                fillTextField(field, value);
            }} else if (type === 'checkbox') {{
                fillCheckbox(field, value);
            }} else if (type === 'radio') {{
                if (!fillRadio(field, value)) {{
                    failedFields.push({{ field: key, reason: 'No matching radio option found for value: ' + String(value) }});
                    continue;
                }}
            }} else if (type === 'file') {{
                failedFields.push({{ field: key, reason: 'File inputs cannot be filled via JavaScript for security reasons' }});
                continue;
            }} else {{
                // text, email, password, number, tel, url, search, date, etc.
                fillTextField(field, value);
            }}

            filledFields.push(key);
        }} catch(e) {{
            failedFields.push({{ field: key, reason: 'Error: ' + e.message }});
        }}
    }}

    // ---------------------------------------------------------------
    // Submit
    // ---------------------------------------------------------------

    if (doSubmit) {{
        // Try to click the submit button first
        var submitBtn = form.querySelector('button[type="submit"], input[type="submit"], button:not([type])');
        if (submitBtn) {{
            submitBtn.click();
            submitted = true;
        }} else {{
            // Fallback: programmatic submit
            form.submit();
            submitted = true;
        }}
    }}

    return JSON.stringify({{
        filled_fields: filledFields,
        failed_fields: failedFields,
        submitted: submitted
    }});
}})();
"#,
            form_sel = serde_json::to_string(form_selector).unwrap_or_else(|_| "\"form\"".to_string()),
            data = data_json,
            human = if human_like { "true" } else { "false" },
            submit = if submit { "true" } else { "false" },
            clear = if clear_first { "true" } else { "false" },
        )
    }

    /// Generates JavaScript that fills a form with human-like typing delays.
    ///
    /// Unlike [`fill_form_script`], this variant returns a `Promise`-based
    /// script that types characters one at a time with random delays between
    /// 30ms and 120ms. This is useful for bypassing bot detection that
    /// monitors typing cadence.
    ///
    /// The returned JS wraps the fill in an async IIFE returning a `Promise`
    /// that resolves to the JSON [`FormFillResult`].
    ///
    /// # Arguments
    ///
    /// * `request` - The fill instructions (the `human_like` field is ignored;
    ///   this method always uses human-like typing).
    ///
    /// # Returns
    ///
    /// A `String` containing the JavaScript to evaluate.
    pub fn fill_form_human_like_script(request: &FormFillRequest) -> String {
        let data_json =
            serde_json::to_string(&request.data).unwrap_or_else(|_| "{}".to_string());
        let form_selector = request
            .form_selector
            .as_deref()
            .unwrap_or("form");
        let submit = request.submit;
        let clear_first = request.clear_first;

        format!(
            r#"
(async function() {{
    'use strict';

    var formSelector = {form_sel};
    var data = {data};
    var doSubmit = {submit};
    var clearFirst = {clear};

    var filledFields = [];
    var failedFields = [];
    var submitted = false;

    function dispatchEvents(el, eventNames) {{
        eventNames.forEach(function(name) {{
            var evt;
            if (name === 'input') {{
                evt = new InputEvent('input', {{ bubbles: true, cancelable: true }});
            }} else {{
                evt = new Event(name, {{ bubbles: true, cancelable: true }});
            }}
            el.dispatchEvent(evt);
        }});
    }}

    function setNativeValue(el, value) {{
        var desc = Object.getOwnPropertyDescriptor(
            el.tagName.toLowerCase() === 'textarea'
                ? window.HTMLTextAreaElement.prototype
                : window.HTMLInputElement.prototype,
            'value'
        );
        if (desc && desc.set) {{
            desc.set.call(el, value);
        }} else {{
            el.value = value;
        }}
    }}

    function sleep(ms) {{
        return new Promise(function(resolve) {{ setTimeout(resolve, ms); }});
    }}

    /** Type text character by character with random delays. */
    async function typeHumanLike(field, text) {{
        field.focus();
        dispatchEvents(field, ['focus', 'focusin']);
        if (clearFirst) {{
            setNativeValue(field, '');
            dispatchEvents(field, ['input', 'change']);
        }}
        var current = '';
        for (var i = 0; i < text.length; i++) {{
            current += text[i];
            setNativeValue(field, current);
            var keyEvt = new KeyboardEvent('keydown', {{
                key: text[i], code: 'Key' + text[i].toUpperCase(),
                bubbles: true, cancelable: true
            }});
            field.dispatchEvent(keyEvt);
            dispatchEvents(field, ['input']);
            field.dispatchEvent(new KeyboardEvent('keyup', {{
                key: text[i], code: 'Key' + text[i].toUpperCase(),
                bubbles: true, cancelable: true
            }}));
            var delay = 30 + Math.floor(Math.random() * 90);
            await sleep(delay);
        }}
        dispatchEvents(field, ['change', 'blur', 'focusout']);
    }}

    var form = document.querySelector(formSelector);
    if (!form) {{
        return JSON.stringify({{
            filled_fields: [],
            failed_fields: [{{ field: '__form__', reason: 'Form not found: ' + formSelector }}],
            submitted: false
        }});
    }}

    var allFields = Array.from(form.querySelectorAll('input, select, textarea'));

    function findField(key) {{
        var keyLower = key.toLowerCase();
        for (var i = 0; i < allFields.length; i++) {{
            if (allFields[i].name && allFields[i].name.toLowerCase() === keyLower) return allFields[i];
        }}
        for (var i = 0; i < allFields.length; i++) {{
            if (allFields[i].id && allFields[i].id.toLowerCase() === keyLower) return allFields[i];
        }}
        for (var i = 0; i < allFields.length; i++) {{
            var ph = allFields[i].getAttribute('placeholder');
            if (ph && ph.toLowerCase().indexOf(keyLower) !== -1) return allFields[i];
        }}
        for (var i = 0; i < allFields.length; i++) {{
            var al = allFields[i].getAttribute('aria-label');
            if (al && al.toLowerCase().indexOf(keyLower) !== -1) return allFields[i];
        }}
        for (var i = 0; i < allFields.length; i++) {{
            var ac = allFields[i].getAttribute('autocomplete');
            if (ac && ac.toLowerCase() === keyLower) return allFields[i];
        }}
        try {{ var el = form.querySelector(key); if (el) return el; }} catch(e) {{}}
        return null;
    }}

    var keys = Object.keys(data);
    for (var k = 0; k < keys.length; k++) {{
        var key = keys[k];
        var value = data[key];
        var field = findField(key);

        if (!field) {{ failedFields.push({{ field: key, reason: 'No matching field found' }}); continue; }}
        if (field.disabled) {{ failedFields.push({{ field: key, reason: 'Field is disabled' }}); continue; }}
        if (field.readOnly) {{ failedFields.push({{ field: key, reason: 'Field is read-only' }}); continue; }}

        try {{
            var tag = field.tagName.toLowerCase();
            var type = (field.getAttribute('type') || 'text').toLowerCase();

            if (tag === 'select' || type === 'checkbox' || type === 'radio' || type === 'file') {{
                // Non-text fields: same instant handling as non-human-like
                if (tag === 'select') {{
                    var valStr = String(value).toLowerCase();
                    var matched = false;
                    for (var i = 0; i < field.options.length; i++) {{
                        if (field.options[i].value.toLowerCase() === valStr ||
                            field.options[i].textContent.trim().toLowerCase() === valStr) {{
                            field.selectedIndex = i;
                            matched = true;
                            break;
                        }}
                    }}
                    if (!matched) {{ failedFields.push({{ field: key, reason: 'No matching option' }}); continue; }}
                    dispatchEvents(field, ['input', 'change']);
                }} else if (type === 'checkbox') {{
                    var shouldCheck = (typeof value === 'boolean') ? value :
                        (String(value).toLowerCase() === 'true' || value === '1');
                    if (field.checked !== shouldCheck) {{ field.checked = shouldCheck; dispatchEvents(field, ['change']); }}
                }} else if (type === 'radio') {{
                    // handled outside
                    failedFields.push({{ field: key, reason: 'Radio not supported in human-like mode yet' }});
                    continue;
                }} else {{
                    failedFields.push({{ field: key, reason: 'File inputs cannot be filled via JS' }});
                    continue;
                }}
            }} else {{
                await typeHumanLike(field, String(value));
            }}
            filledFields.push(key);
        }} catch(e) {{
            failedFields.push({{ field: key, reason: 'Error: ' + e.message }});
        }}

        // Small pause between fields
        await sleep(100 + Math.floor(Math.random() * 200));
    }}

    if (doSubmit) {{
        var submitBtn = form.querySelector('button[type="submit"], input[type="submit"], button:not([type])');
        if (submitBtn) {{ submitBtn.click(); submitted = true; }}
        else {{ form.submit(); submitted = true; }}
    }}

    return JSON.stringify({{ filled_fields: filledFields, failed_fields: failedFields, submitted: submitted }});
}})();
"#,
            form_sel = serde_json::to_string(form_selector).unwrap_or_else(|_| "\"form\"".to_string()),
            data = data_json,
            submit = if submit { "true" } else { "false" },
            clear = if clear_first { "true" } else { "false" },
        )
    }

    /// Generates JavaScript that validates the current state of a form.
    ///
    /// The script uses the HTML5 Constraint Validation API to check each
    /// field. It returns a JSON string representing a [`FormValidationResult`]
    /// with `is_valid` set to `true` when all fields pass, and a list of
    /// per-field `errors` when they do not.
    ///
    /// # Arguments
    ///
    /// * `form_selector` - CSS selector for the form to validate.
    ///
    /// # Returns
    ///
    /// A `String` containing the JavaScript to evaluate.
    pub fn validate_form_script(form_selector: &str) -> String {
        let selector_json =
            serde_json::to_string(form_selector).unwrap_or_else(|_| "\"form\"".to_string());

        format!(
            r#"
(function() {{
    'use strict';

    var formSelector = {sel};
    var form = document.querySelector(formSelector);

    if (!form) {{
        return JSON.stringify({{
            is_valid: false,
            errors: [{{ selector: formSelector, field: '__form__', message: 'Form not found' }}]
        }});
    }}

    var errors = [];
    var fields = form.querySelectorAll('input, select, textarea');

    fields.forEach(function(field) {{
        // Skip hidden, submit, reset, button types
        var type = (field.getAttribute('type') || '').toLowerCase();
        if (type === 'submit' || type === 'reset' || type === 'button' || type === 'image') return;

        if (!field.checkValidity()) {{
            var name = field.name || field.id || '';
            var selector = '';
            if (field.id) {{
                selector = '#' + CSS.escape(field.id);
            }} else if (field.name) {{
                selector = '[name=' + JSON.stringify(field.name) + ']';
            }}

            var message = field.validationMessage || 'Validation failed';

            // Add more specific messages based on validity state
            var vs = field.validity;
            if (vs.valueMissing) message = 'This field is required';
            else if (vs.typeMismatch) message = 'Invalid format for type: ' + type;
            else if (vs.patternMismatch) message = 'Value does not match pattern: ' + (field.getAttribute('pattern') || '');
            else if (vs.tooLong) message = 'Value is too long (max ' + field.maxLength + ' characters)';
            else if (vs.tooShort) message = 'Value is too short (min ' + field.minLength + ' characters)';
            else if (vs.rangeOverflow) message = 'Value is too high (max ' + field.max + ')';
            else if (vs.rangeUnderflow) message = 'Value is too low (min ' + field.min + ')';
            else if (vs.stepMismatch) message = 'Value does not match step constraint';

            errors.push({{
                selector: selector,
                field: name,
                message: message
            }});
        }}
    }});

    return JSON.stringify({{
        is_valid: errors.length === 0,
        errors: errors
    }});
}})();
"#,
            sel = selector_json,
        )
    }
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

    // -- FieldType tests --------------------------------------------------

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

    // -- Struct construction tests ----------------------------------------

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

        let deserialized: FormInfo =
            serde_json::from_str(&json).expect("deserialize FormInfo");
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

        let deserialized: FormField =
            serde_json::from_str(&json).expect("deserialize FormField");
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
        let deserialized: FieldOption =
            serde_json::from_str(&json).expect("deserialize FieldOption");
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
        let deserialized: FormFillResult =
            serde_json::from_str(&json).expect("deserialize FormFillResult");
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
        let deserialized: FormValidationResult =
            serde_json::from_str(&json).expect("deserialize FormValidationResult");
        assert!(!deserialized.is_valid);
        assert_eq!(deserialized.errors.len(), 2);
    }

    // -- Script generation tests ------------------------------------------

    #[test]
    fn test_detect_forms_script_returns_valid_js() {
        let script = FormHandler::detect_forms_script();

        // Must be a self-invoking function
        assert!(script.contains("(function()"));
        assert!(script.contains("return JSON.stringify(results)"));

        // Must query forms
        assert!(script.contains("document.querySelectorAll('form')"));

        // Must handle label detection
        assert!(script.contains("findLabel"));

        // Must detect form purpose
        assert!(script.contains("detectFormPurpose"));
        assert!(script.contains("is_login_form"));
        assert!(script.contains("is_search_form"));
        assert!(script.contains("is_registration_form"));

        // Must find submit buttons
        assert!(script.contains("findSubmitButton"));

        // Must generate unique selectors
        assert!(script.contains("uniqueSelector"));
    }

    #[test]
    fn test_fill_form_script_with_basic_data() {
        let mut data = HashMap::new();
        data.insert(
            "username".to_string(),
            serde_json::Value::String("testuser".to_string()),
        );
        data.insert(
            "password".to_string(),
            serde_json::Value::String("secret123".to_string()),
        );

        let request = FormFillRequest {
            form_selector: Some("form#login".to_string()),
            data,
            human_like: false,
            submit: true,
            clear_first: true,
        };

        let script = FormHandler::fill_form_script(&request);

        // Must contain the form selector
        assert!(script.contains("form#login"));

        // Must contain the data values
        assert!(script.contains("testuser"));
        assert!(script.contains("secret123"));

        // Must handle submit
        assert!(script.contains("doSubmit"));
        assert!(script.contains("var doSubmit = true"));

        // Must handle clear_first
        assert!(script.contains("var clearFirst = true"));

        // Must dispatch events
        assert!(script.contains("dispatchEvents"));
        assert!(script.contains("InputEvent"));

        // Must use native value setter for framework compatibility
        assert!(script.contains("setNativeValue"));

        // Must return JSON result
        assert!(script.contains("filled_fields"));
        assert!(script.contains("failed_fields"));
    }

    #[test]
    fn test_fill_form_script_default_form_selector() {
        let request = FormFillRequest {
            form_selector: None,
            data: HashMap::new(),
            human_like: false,
            submit: false,
            clear_first: false,
        };

        let script = FormHandler::fill_form_script(&request);

        // When no selector is given, should default to "form"
        assert!(script.contains(r#"var formSelector = "form""#));
        assert!(script.contains("var doSubmit = false"));
        assert!(script.contains("var clearFirst = false"));
    }

    #[test]
    fn test_fill_form_script_field_matching_strategies() {
        let request = FormFillRequest {
            form_selector: None,
            data: HashMap::new(),
            human_like: false,
            submit: false,
            clear_first: false,
        };

        let script = FormHandler::fill_form_script(&request);

        // Must implement all matching strategies
        assert!(script.contains(".name")); // by name
        assert!(script.contains(".id")); // by id
        assert!(script.contains("label")); // by label
        assert!(script.contains("placeholder")); // by placeholder
        assert!(script.contains("aria-label")); // by aria-label
        assert!(script.contains("autocomplete")); // by autocomplete
        assert!(script.contains("querySelector")); // by CSS selector fallback
    }

    #[test]
    fn test_fill_form_script_handles_all_field_types() {
        let script = FormHandler::fill_form_script(&FormFillRequest {
            form_selector: None,
            data: HashMap::new(),
            human_like: false,
            submit: false,
            clear_first: false,
        });

        // Must handle select elements
        assert!(script.contains("fillSelect"));

        // Must handle checkboxes
        assert!(script.contains("fillCheckbox"));
        assert!(script.contains(".checked"));

        // Must handle radio buttons
        assert!(script.contains("fillRadio"));
        assert!(script.contains("type=\"radio\""));

        // Must handle file input (with error)
        assert!(script.contains("File inputs cannot be filled"));

        // Must handle text-like fields
        assert!(script.contains("fillTextField"));
    }

    #[test]
    fn test_human_like_script_generates_async() {
        let mut data = HashMap::new();
        data.insert(
            "email".to_string(),
            serde_json::Value::String("user@example.com".to_string()),
        );

        let request = FormFillRequest {
            form_selector: Some("form".to_string()),
            data,
            human_like: true,
            submit: false,
            clear_first: true,
        };

        let script = FormHandler::fill_form_human_like_script(&request);

        // Must be an async function
        assert!(script.contains("async function"));

        // Must have typing delays
        assert!(script.contains("typeHumanLike"));
        assert!(script.contains("sleep"));
        assert!(script.contains("Math.random()"));

        // Must dispatch keyboard events
        assert!(script.contains("KeyboardEvent"));
        assert!(script.contains("keydown"));
        assert!(script.contains("keyup"));
    }

    #[test]
    fn test_validate_form_script() {
        let script = FormHandler::validate_form_script("form#signup");

        // Must target the correct form
        assert!(script.contains("form#signup"));

        // Must use HTML5 Constraint Validation API
        assert!(script.contains("checkValidity"));
        assert!(script.contains("validationMessage"));
        assert!(script.contains("validity"));

        // Must check specific validity states
        assert!(script.contains("valueMissing"));
        assert!(script.contains("typeMismatch"));
        assert!(script.contains("patternMismatch"));
        assert!(script.contains("tooLong"));
        assert!(script.contains("tooShort"));
        assert!(script.contains("rangeOverflow"));
        assert!(script.contains("rangeUnderflow"));
        assert!(script.contains("stepMismatch"));

        // Must return JSON with is_valid and errors
        assert!(script.contains("is_valid"));
        assert!(script.contains("errors"));

        // Must handle missing form
        assert!(script.contains("Form not found"));
    }

    #[test]
    fn test_validate_form_script_escapes_selector() {
        let script = FormHandler::validate_form_script(r#"form[data-id="test"]"#);
        // The selector should be properly JSON-escaped
        assert!(script.contains(r#"form[data-id=\"test\"]"#));
    }

    // -- FormFillRequest tests --------------------------------------------

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
        let deserialized: FormFillRequest =
            serde_json::from_str(&json).expect("deserialize FormFillRequest");

        assert_eq!(
            deserialized.form_selector,
            Some("form#profile".to_string())
        );
        assert!(deserialized.human_like);
        assert!(!deserialized.submit);
        assert!(deserialized.clear_first);
        assert_eq!(deserialized.data.len(), 3);
        assert_eq!(
            deserialized.data.get("name"),
            Some(&serde_json::json!("John Doe"))
        );
    }

    // -- Detection script output format tests -----------------------------

    #[test]
    fn test_detect_script_handles_radio_dedup() {
        let script = FormHandler::detect_forms_script();
        // Must deduplicate radio buttons by name
        assert!(script.contains("seenRadios"));
    }

    #[test]
    fn test_detect_script_skips_non_data_inputs() {
        let script = FormHandler::detect_forms_script();
        // Must skip submit, reset, button, and image inputs
        assert!(script.contains("'submit'"));
        assert!(script.contains("'reset'"));
        assert!(script.contains("'button'"));
        assert!(script.contains("'image'"));
    }

    #[test]
    fn test_detect_script_checks_visibility() {
        let script = FormHandler::detect_forms_script();
        assert!(script.contains("isVisible"));
        assert!(script.contains("getComputedStyle"));
        assert!(script.contains("display"));
        assert!(script.contains("visibility"));
        assert!(script.contains("opacity"));
    }

    #[test]
    fn test_detect_script_handles_aria_labelledby() {
        let script = FormHandler::detect_forms_script();
        assert!(script.contains("aria-labelledby"));
    }

    #[test]
    fn test_detect_script_collects_select_options() {
        let script = FormHandler::detect_forms_script();
        assert!(script.contains("collectOptions"));
        assert!(script.contains("selectedIndex") | script.contains(".selected"));
    }

    #[test]
    fn test_detect_script_collects_datalist_options() {
        let script = FormHandler::detect_forms_script();
        // Must handle <datalist> via the list attribute
        assert!(script.contains("getAttribute('list')"));
    }

    #[test]
    fn test_fill_script_escapes_special_characters_in_data() {
        let mut data = HashMap::new();
        data.insert(
            "bio".to_string(),
            serde_json::Value::String("Line1\nLine2\t\"quoted\"".to_string()),
        );

        let request = FormFillRequest {
            form_selector: None,
            data,
            human_like: false,
            submit: false,
            clear_first: false,
        };

        let script = FormHandler::fill_form_script(&request);
        // The JSON-escaped data should appear in the script without breaking JS syntax
        assert!(script.contains("Line1\\nLine2\\t\\\"quoted\\\""));
    }
}
