//! Form detection via JavaScript injection into the browser context.
//!
//! This module provides [`FormHandler::detect_forms_script`], which generates
//! a self-invoking JavaScript function that scans the live DOM for all
//! `<form>` elements, enumerates their fields, resolves label associations
//! (via `for` attribute, `aria-labelledby`, or wrapping `<label>`), detects
//! form purpose heuristics (login, search, registration), and identifies the
//! primary submit button.  The script returns a JSON array of `FormInfo`
//! objects that can be deserialized on the Rust side.

use super::handler::FormHandler;

impl FormHandler {
    /// Generates JavaScript that detects and analyzes all forms on the page.
    ///
    /// The returned script, when evaluated in a browser context, produces a
    /// JSON string representing a `Vec<FormInfo>`. Steps performed:
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
        if (field.id) {
            var lbl = document.querySelector('label[for=' + JSON.stringify(field.id) + ']');
            if (lbl) return lbl.textContent.trim();
        }
        var ariaLblBy = field.getAttribute('aria-labelledby');
        if (ariaLblBy) {
            var parts = ariaLblBy.split(/\s+/);
            var text = parts.map(function(id) {
                var el = document.getElementById(id);
                return el ? el.textContent.trim() : '';
            }).filter(Boolean).join(' ');
            if (text) return text;
        }
        var parent = field.closest('label');
        if (parent) {
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
                opts.push({ value: opt.value, label: opt.textContent.trim(), selected: opt.selected, disabled: opt.disabled });
            });
        } else if (field.getAttribute('list')) {
            var dl = document.getElementById(field.getAttribute('list'));
            if (dl) {
                Array.from(dl.querySelectorAll('option')).forEach(function(opt) {
                    opts.push({ value: opt.value, label: opt.textContent.trim() || opt.value, selected: false, disabled: opt.disabled || false });
                });
            }
        }
        return opts;
    }

    /** Collect radio options when the field is a radio button. */
    function collectRadioOptions(field, form) {
        if (!field.name) return [];
        var radios = form.querySelectorAll('input[type="radio"][name=' + JSON.stringify(field.name) + ']');
        return Array.from(radios).map(function(r) {
            var lbl = findLabel(r);
            return { value: r.value, label: lbl || r.value, selected: r.checked, disabled: r.disabled };
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
        var candidates = Array.from(form.querySelectorAll(
            'button[type="submit"], input[type="submit"], button:not([type])'
        ));
        if (form.id) {
            var external = document.querySelectorAll(
                'button[form=' + JSON.stringify(form.id) + '], ' +
                'input[type="submit"][form=' + JSON.stringify(form.id) + ']'
            );
            candidates = candidates.concat(Array.from(external));
        }
        if (candidates.length === 0) return null;
        var btn = candidates[0];
        var text = btn.tagName.toLowerCase() === 'input' ? (btn.value || 'Submit') : (btn.textContent.trim() || 'Submit');
        return { selector: uniqueSelector(btn), text: text, button_type: btn.getAttribute('type') || 'submit' };
    }

    /** Detect form purpose heuristics (login vs search vs registration). */
    function detectFormPurpose(form, fields) {
        var hasPassword = false, passwordCount = 0, hasEmail = false, hasSearch = false, hasFile = false;
        var fieldNames = [], fieldAutocompletes = [];
        fields.forEach(function(f) {
            var typeName = (typeof f.field_type === 'string') ? f.field_type : '';
            if (typeName === 'Password') { hasPassword = true; passwordCount++; }
            if (typeName === 'Email') hasEmail = true;
            if (typeName === 'Search') hasSearch = true;
            if (typeName === 'File') hasFile = true;
            if (f.name) fieldNames.push(f.name.toLowerCase());
            if (f.autocomplete) fieldAutocompletes.push(f.autocomplete.toLowerCase());
        });
        var formRole = form.getAttribute('role') || '';
        var formAction = (form.getAttribute('action') || '').toLowerCase();
        var isSearch = hasSearch || formRole === 'search' || formAction.indexOf('search') !== -1 ||
            fieldNames.some(function(n) { return n === 'q' || n === 'query' || n === 'search'; }) ||
            fieldAutocompletes.some(function(a) { return a === 'off' && fields.length <= 2; });
        var isLogin = hasPassword && passwordCount === 1 && !isSearch &&
            (hasEmail ||
             fieldNames.some(function(n) { return n === 'username' || n === 'user' || n === 'login' || n === 'email' || n === 'user_login'; }) ||
             fieldAutocompletes.some(function(a) { return a === 'username' || a === 'current-password'; }));
        var isRegistration = hasPassword && passwordCount >= 2 && !isSearch;
        if (!isRegistration && hasPassword && !isLogin) {
            isRegistration = fieldNames.some(function(n) {
                return n.indexOf('confirm') !== -1 || n.indexOf('register') !== -1 ||
                       n.indexOf('signup') !== -1 || n === 'password2' || n === 'password_confirmation';
            }) || fieldAutocompletes.some(function(a) { return a === 'new-password'; });
        }
        return { is_login_form: isLogin, is_search_form: isSearch, is_registration_form: isRegistration, has_file_upload: hasFile };
    }

    var forms = document.querySelectorAll('form');
    var results = [];
    forms.forEach(function(form, idx) {
        var formSelector = uniqueSelector(form);
        var fieldElements = form.querySelectorAll('input, select, textarea');
        var seenRadios = {};
        var fields = [];
        fieldElements.forEach(function(field) {
            var tag = field.tagName.toLowerCase();
            var typeAttr = (field.getAttribute('type') || '').toLowerCase();
            if (tag === 'input' && (typeAttr === 'submit' || typeAttr === 'reset' || typeAttr === 'button' || typeAttr === 'image')) return;
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
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_detect_forms_script_returns_valid_js() {
        let script = FormHandler::detect_forms_script();
        assert!(script.contains("(function()"));
        assert!(script.contains("return JSON.stringify(results)"));
        assert!(script.contains("document.querySelectorAll('form')"));
        assert!(script.contains("findLabel"));
        assert!(script.contains("detectFormPurpose"));
        assert!(script.contains("is_login_form"));
        assert!(script.contains("is_search_form"));
        assert!(script.contains("is_registration_form"));
        assert!(script.contains("findSubmitButton"));
        assert!(script.contains("uniqueSelector"));
    }

    #[test]
    fn test_detect_script_handles_radio_dedup() {
        let script = FormHandler::detect_forms_script();
        assert!(script.contains("seenRadios"));
    }

    #[test]
    fn test_detect_script_skips_non_data_inputs() {
        let script = FormHandler::detect_forms_script();
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
        assert!(script.contains("getAttribute('list')"));
    }
}
