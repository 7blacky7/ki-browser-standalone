//! Form-filling JavaScript generation for browser automation.
//!
//! This module provides two fill strategies on [`FormHandler`]:
//!
//! - [`FormHandler::fill_form_script`] — instant fill that dispatches proper
//!   DOM events (`focus`, `input`, `change`, `blur`) and uses the native
//!   HTMLInputElement value setter to bypass React / framework wrappers.
//! - [`FormHandler::fill_form_human_like_script`] — async fill that types
//!   each character individually with random 30–120 ms delays to mimic human
//!   typing cadence and evade bot-detection heuristics based on typing speed.
//!
//! Both scripts implement the same field-matching strategy: name → id →
//! label text (substring) → placeholder (substring) → aria-label (substring)
//! → autocomplete → CSS selector fallback.

use super::handler::FormHandler;
use super::types::FormFillRequest;

impl FormHandler {
    /// Generates JavaScript that fills a form with the given data.
    ///
    /// The matching strategy tries each data key against fields in this order:
    /// `name`, `id`, `label` (case-insensitive substring), `placeholder`
    /// (case-insensitive substring), `aria-label` (case-insensitive substring),
    /// `autocomplete` attribute, and finally as a literal CSS selector.
    ///
    /// For each matched field the script dispatches proper DOM events and uses
    /// the native HTMLInputElement setter to bypass React / Vue wrappers.
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
        let form_selector = request.form_selector.as_deref().unwrap_or("form");
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
        var nativeInputValueSetter = Object.getOwnPropertyDescriptor(window.HTMLInputElement.prototype, 'value');
        var nativeTextareaValueSetter = Object.getOwnPropertyDescriptor(window.HTMLTextAreaElement.prototype, 'value');
        var setter = null;
        if (el.tagName.toLowerCase() === 'textarea' && nativeTextareaValueSetter) {{
            setter = nativeTextareaValueSetter.set;
        }} else if (nativeInputValueSetter) {{
            setter = nativeInputValueSetter.set;
        }}
        if (setter) {{ setter.call(el, value); }} else {{ el.value = value; }}
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

    function fillCheckbox(field, value) {{
        var shouldCheck = false;
        if (typeof value === 'boolean') {{ shouldCheck = value; }}
        else if (typeof value === 'string') {{ shouldCheck = value.toLowerCase() === 'true' || value === '1' || value === 'yes'; }}
        else {{ shouldCheck = !!value; }}
        if (field.checked !== shouldCheck) {{
            field.focus();
            field.checked = shouldCheck;
            dispatchEvents(field, ['input', 'change', 'blur']);
        }}
    }}

    function fillRadio(field, value) {{
        var name = field.name;
        if (!name) return false;
        var radios = form.querySelectorAll('input[type="radio"][name=' + JSON.stringify(name) + ']');
        var valStr = String(value).toLowerCase();
        var target = null;
        radios.forEach(function(r) {{ if (r.value.toLowerCase() === valStr) target = r; }});
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

    function fillSelect(field, value) {{
        var valStr = String(value).toLowerCase();
        var matched = false;
        var options = field.options;
        for (var i = 0; i < options.length; i++) {{
            if (options[i].value.toLowerCase() === valStr) {{ field.selectedIndex = i; matched = true; break; }}
        }}
        if (!matched) {{
            for (var i = 0; i < options.length; i++) {{
                if (options[i].textContent.trim().toLowerCase() === valStr) {{ field.selectedIndex = i; matched = true; break; }}
            }}
        }}
        if (!matched) {{
            for (var i = 0; i < options.length; i++) {{
                if (options[i].textContent.trim().toLowerCase().indexOf(valStr) !== -1) {{ field.selectedIndex = i; matched = true; break; }}
            }}
        }}
        if (matched) {{ dispatchEvents(field, ['input', 'change', 'blur']); }}
        return matched;
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
                fillTextField(field, value);
            }}
            filledFields.push(key);
        }} catch(e) {{
            failedFields.push({{ field: key, reason: 'Error: ' + e.message }});
        }}
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
            human = if human_like { "true" } else { "false" },
            submit = if submit { "true" } else { "false" },
            clear = if clear_first { "true" } else { "false" },
        )
    }

    /// Generates JavaScript that fills a form with human-like typing delays.
    ///
    /// Unlike [`fill_form_script`], this variant returns an async IIFE that types
    /// characters one at a time with random 30–120 ms delays. This is useful for
    /// bypassing bot detection that monitors typing cadence.
    ///
    /// # Arguments
    ///
    /// * `request` - The fill instructions (the `human_like` field is ignored;
    ///   this method always uses human-like typing).
    ///
    /// # Returns
    ///
    /// A `String` containing the async JavaScript to evaluate.
    pub fn fill_form_human_like_script(request: &FormFillRequest) -> String {
        let data_json =
            serde_json::to_string(&request.data).unwrap_or_else(|_| "{}".to_string());
        let form_selector = request.form_selector.as_deref().unwrap_or("form");
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
        if (desc && desc.set) {{ desc.set.call(el, value); }} else {{ el.value = value; }}
    }}

    function sleep(ms) {{
        return new Promise(function(resolve) {{ setTimeout(resolve, ms); }});
    }}

    /** Type text character by character with random delays to mimic human input. */
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
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    #[test]
    fn test_fill_form_script_with_basic_data() {
        let mut data = HashMap::new();
        data.insert("username".to_string(), serde_json::Value::String("testuser".to_string()));
        data.insert("password".to_string(), serde_json::Value::String("secret123".to_string()));
        let request = FormFillRequest {
            form_selector: Some("form#login".to_string()),
            data,
            human_like: false,
            submit: true,
            clear_first: true,
        };
        let script = FormHandler::fill_form_script(&request);
        assert!(script.contains("form#login"));
        assert!(script.contains("testuser"));
        assert!(script.contains("secret123"));
        assert!(script.contains("doSubmit"));
        assert!(script.contains("var doSubmit = true"));
        assert!(script.contains("var clearFirst = true"));
        assert!(script.contains("dispatchEvents"));
        assert!(script.contains("InputEvent"));
        assert!(script.contains("setNativeValue"));
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
        assert!(script.contains(".name"));
        assert!(script.contains(".id"));
        assert!(script.contains("label"));
        assert!(script.contains("placeholder"));
        assert!(script.contains("aria-label"));
        assert!(script.contains("autocomplete"));
        assert!(script.contains("querySelector"));
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
        assert!(script.contains("fillSelect"));
        assert!(script.contains("fillCheckbox"));
        assert!(script.contains(".checked"));
        assert!(script.contains("fillRadio"));
        assert!(script.contains("type=\"radio\""));
        assert!(script.contains("File inputs cannot be filled"));
        assert!(script.contains("fillTextField"));
    }

    #[test]
    fn test_human_like_script_generates_async() {
        let mut data = HashMap::new();
        data.insert("email".to_string(), serde_json::Value::String("user@example.com".to_string()));
        let request = FormFillRequest {
            form_selector: Some("form".to_string()),
            data,
            human_like: true,
            submit: false,
            clear_first: true,
        };
        let script = FormHandler::fill_form_human_like_script(&request);
        assert!(script.contains("async function"));
        assert!(script.contains("typeHumanLike"));
        assert!(script.contains("sleep"));
        assert!(script.contains("Math.random()"));
        assert!(script.contains("KeyboardEvent"));
        assert!(script.contains("keydown"));
        assert!(script.contains("keyup"));
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
        assert!(script.contains("Line1\\nLine2\\t\\\"quoted\\\""));
    }
}
