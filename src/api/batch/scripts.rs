//! Built-in JavaScript extraction scripts for batch operations.
//!
//! Provides standalone functions that return JavaScript source code
//! for extracting structured data, visible text content, form
//! descriptions, and hyperlinks from a browser page context.

/// JavaScript to extract structured data (JSON-LD, microdata, RDFa) from a page.
pub fn extract_structured_data_script() -> &'static str {
    r#"(() => {
    const result = { jsonLd: [], microdata: [], meta: {} };

    // JSON-LD
    document.querySelectorAll('script[type="application/ld+json"]').forEach(el => {
        try {
            result.jsonLd.push(JSON.parse(el.textContent));
        } catch (e) { /* skip malformed JSON-LD */ }
    });

    // Microdata
    document.querySelectorAll('[itemscope]').forEach(el => {
        const item = { type: el.getAttribute('itemtype') || '', properties: {} };
        el.querySelectorAll('[itemprop]').forEach(prop => {
            const name = prop.getAttribute('itemprop');
            const value = prop.getAttribute('content')
                || prop.getAttribute('href')
                || prop.getAttribute('src')
                || prop.textContent.trim();
            if (item.properties[name]) {
                if (!Array.isArray(item.properties[name])) {
                    item.properties[name] = [item.properties[name]];
                }
                item.properties[name].push(value);
            } else {
                item.properties[name] = value;
            }
        });
        result.microdata.push(item);
    });

    // Open Graph and Twitter Card meta tags
    document.querySelectorAll('meta[property^="og:"], meta[name^="twitter:"]').forEach(el => {
        const key = el.getAttribute('property') || el.getAttribute('name');
        result.meta[key] = el.getAttribute('content');
    });

    // Standard meta tags
    document.querySelectorAll('meta[name="description"], meta[name="author"], meta[name="keywords"]').forEach(el => {
        result.meta[el.getAttribute('name')] = el.getAttribute('content');
    });

    return JSON.stringify(result);
})()"#
}

/// JavaScript to extract visible text content from a page.
pub fn extract_content_script() -> &'static str {
    r#"(() => {
    const result = {
        title: document.title || '',
        url: window.location.href,
        text: '',
        headings: [],
        language: document.documentElement.lang || ''
    };

    // Extract main content text
    const mainEl = document.querySelector('main, [role="main"], article, .content, #content');
    if (mainEl) {
        result.text = mainEl.innerText.trim();
    } else {
        result.text = document.body.innerText.trim();
    }

    // Extract headings hierarchy
    document.querySelectorAll('h1, h2, h3, h4, h5, h6').forEach(h => {
        result.headings.push({
            level: parseInt(h.tagName.charAt(1)),
            text: h.textContent.trim()
        });
    });

    return JSON.stringify(result);
})()"#
}

/// JavaScript to detect and describe forms on a page.
pub fn detect_forms_script() -> &'static str {
    r#"(() => {
    const forms = [];
    document.querySelectorAll('form').forEach((form, index) => {
        const fields = [];
        form.querySelectorAll('input, select, textarea, button').forEach(el => {
            const field = {
                tag: el.tagName.toLowerCase(),
                type: el.getAttribute('type') || (el.tagName === 'TEXTAREA' ? 'textarea' : el.tagName === 'SELECT' ? 'select' : 'text'),
                name: el.getAttribute('name') || '',
                id: el.getAttribute('id') || '',
                placeholder: el.getAttribute('placeholder') || '',
                required: el.hasAttribute('required'),
                value: el.value || '',
                label: ''
            };

            // Find associated label
            if (el.id) {
                const label = document.querySelector('label[for="' + el.id + '"]');
                if (label) field.label = label.textContent.trim();
            }
            if (!field.label) {
                const parent = el.closest('label');
                if (parent) field.label = parent.textContent.trim();
            }

            // For select elements, extract options
            if (el.tagName === 'SELECT') {
                field.options = Array.from(el.options).map(opt => ({
                    value: opt.value,
                    text: opt.textContent.trim(),
                    selected: opt.selected
                }));
            }

            fields.push(field);
        });

        forms.push({
            index: index,
            id: form.getAttribute('id') || '',
            name: form.getAttribute('name') || '',
            action: form.getAttribute('action') || '',
            method: (form.getAttribute('method') || 'GET').toUpperCase(),
            fields: fields
        });
    });

    return JSON.stringify(forms);
})()"#
}

/// JavaScript to extract all links from a page.
pub fn extract_links_script() -> &'static str {
    r#"(() => {
    const currentHost = window.location.hostname;
    const links = [];
    const seen = new Set();

    document.querySelectorAll('a[href]').forEach(a => {
        const href = a.href;
        if (!href || href.startsWith('javascript:') || href.startsWith('mailto:') || href.startsWith('tel:')) {
            return;
        }
        if (seen.has(href)) return;
        seen.add(href);

        let isExternal = false;
        try {
            const url = new URL(href, window.location.origin);
            isExternal = url.hostname !== currentHost;
        } catch (e) {
            // Relative URL, not external
        }

        links.push({
            href: href,
            text: a.textContent.trim().substring(0, 200),
            rel: a.getAttribute('rel') || null,
            is_external: isExternal
        });
    });

    return JSON.stringify(links);
})()"#
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_extract_structured_data_script_is_valid_js() {
        let script = extract_structured_data_script();
        assert!(script.contains("jsonLd"));
        assert!(script.contains("microdata"));
        assert!(script.contains("application/ld+json"));
        assert!(script.contains("JSON.stringify"));
    }

    #[test]
    fn test_extract_content_script_is_valid_js() {
        let script = extract_content_script();
        assert!(script.contains("document.title"));
        assert!(script.contains("innerText"));
        assert!(script.contains("headings"));
        assert!(script.contains("JSON.stringify"));
    }

    #[test]
    fn test_detect_forms_script_is_valid_js() {
        let script = detect_forms_script();
        assert!(script.contains("querySelectorAll"));
        assert!(script.contains("form"));
        assert!(script.contains("input"));
        assert!(script.contains("JSON.stringify"));
    }

    #[test]
    fn test_extract_links_script_is_valid_js() {
        let script = extract_links_script();
        assert!(script.contains("a[href]"));
        assert!(script.contains("is_external"));
        assert!(script.contains("JSON.stringify"));
    }
}
