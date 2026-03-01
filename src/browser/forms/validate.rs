//! Form validation via the HTML5 Constraint Validation API.
//!
//! This module provides [`FormHandler::validate_form_script`], which generates
//! a JavaScript snippet that uses `checkValidity()` and the `ValidityState`
//! interface to inspect every field of a given form.  The script returns a
//! JSON object deserializable as [`FormValidationResult`], containing a boolean
//! `is_valid` flag and a per-field list of [`ValidationError`] values with
//! human-readable messages for each constraint violation (required, pattern,
//! type mismatch, range overflow/underflow, length, step).

use super::handler::FormHandler;

impl FormHandler {
    /// Generates JavaScript that validates the current state of a form using
    /// the HTML5 Constraint Validation API (`checkValidity`, `ValidityState`).
    ///
    /// Returns a JSON string deserializable as [`FormValidationResult`] with
    /// `is_valid: true` when all fields pass, and a list of per-field errors
    /// describing the specific constraint violation when they do not.
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
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_validate_form_script() {
        let script = FormHandler::validate_form_script("form#signup");
        assert!(script.contains("form#signup"));
        assert!(script.contains("checkValidity"));
        assert!(script.contains("validationMessage"));
        assert!(script.contains("validity"));
        assert!(script.contains("valueMissing"));
        assert!(script.contains("typeMismatch"));
        assert!(script.contains("patternMismatch"));
        assert!(script.contains("tooLong"));
        assert!(script.contains("tooShort"));
        assert!(script.contains("rangeOverflow"));
        assert!(script.contains("rangeUnderflow"));
        assert!(script.contains("stepMismatch"));
        assert!(script.contains("is_valid"));
        assert!(script.contains("errors"));
        assert!(script.contains("Form not found"));
    }

    #[test]
    fn test_validate_form_script_escapes_selector() {
        let script = FormHandler::validate_form_script(r#"form[data-id="test"]"#);
        assert!(script.contains(r#"form[data-id=\"test\"]"#));
    }
}
