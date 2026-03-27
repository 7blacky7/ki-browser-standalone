//! JavaScript generation utilities for extracting and restoring browser state.
//!
//! Generates self-invoking JavaScript functions that capture or modify
//! cookies, localStorage, and sessionStorage via the browser's JS engine.

use std::collections::HashMap;

use super::types::CookieInfo;
use super::manager::SessionManager;

/// JavaScript generation utilities for extracting and restoring
/// browser state (cookies, localStorage, sessionStorage).
impl SessionManager {
    /// Generate JavaScript that extracts all cookies visible to the page.
    ///
    /// Returns a self-invoking function that produces a JSON string array
    /// of cookie objects. Note: `httpOnly` cookies are not accessible from
    /// JS; use the CDP `Network.getCookies` command for those.
    pub fn get_cookies_script() -> &'static str {
        r#"(() => {
    const cookies = [];
    if (!document.cookie) return JSON.stringify(cookies);

    document.cookie.split(';').forEach(pair => {
        const trimmed = pair.trim();
        if (!trimmed) return;
        const eqIdx = trimmed.indexOf('=');
        if (eqIdx < 0) return;

        const name = decodeURIComponent(trimmed.substring(0, eqIdx).trim());
        const value = decodeURIComponent(trimmed.substring(eqIdx + 1).trim());

        cookies.push({
            name: name,
            value: value,
            domain: window.location.hostname,
            path: '/',
            expires: null,
            http_only: false,
            secure: window.location.protocol === 'https:',
            same_site: null
        });
    });

    return JSON.stringify(cookies);
})()"#
    }

    /// Generate JavaScript that sets a single cookie from a `CookieInfo`.
    ///
    /// The returned script calls `document.cookie = ...` with the
    /// appropriate attributes.
    pub fn set_cookie_script(cookie: &CookieInfo) -> String {
        let mut parts = vec![format!(
            "{}={}",
            js_encode_uri_component(&cookie.name),
            js_encode_uri_component(&cookie.value)
        )];

        if !cookie.domain.is_empty() {
            parts.push(format!("domain={}", cookie.domain));
        }
        if !cookie.path.is_empty() {
            parts.push(format!("path={}", cookie.path));
        }
        if cookie.secure {
            parts.push("secure".to_string());
        }
        if let Some(ref same_site) = cookie.same_site {
            parts.push(format!("samesite={}", same_site));
        }
        if let Some(ref expires) = cookie.expires {
            parts.push(format!("expires={}", expires));
        }

        let cookie_str = parts.join("; ");
        format!(
            r#"(() => {{
    document.cookie = "{}";
    return true;
}})()"#,
            cookie_str.replace('\\', "\\\\").replace('"', "\\\"")
        )
    }

    /// Generate JavaScript that reads all `localStorage` entries.
    ///
    /// Returns a JSON object mapping keys to values.
    pub fn get_local_storage_script() -> &'static str {
        r#"(() => {
    const data = {};
    try {
        for (let i = 0; i < localStorage.length; i++) {
            const key = localStorage.key(i);
            if (key !== null) {
                data[key] = localStorage.getItem(key);
            }
        }
    } catch (e) {
        // localStorage may be blocked by security policy
        return JSON.stringify({ __error: e.message });
    }
    return JSON.stringify(data);
})()"#
    }

    /// Generate JavaScript that restores `localStorage` from a map.
    pub fn set_local_storage_script(entries: &HashMap<String, String>) -> String {
        let json = serde_json::to_string(entries).unwrap_or_else(|_| "{}".to_string());
        format!(
            r#"(() => {{
    try {{
        const entries = JSON.parse('{}');
        for (const [key, value] of Object.entries(entries)) {{
            localStorage.setItem(key, value);
        }}
        return true;
    }} catch (e) {{
        return false;
    }}
}})()"#,
            json.replace('\\', "\\\\").replace('\'', "\\'")
        )
    }

    /// Generate JavaScript that reads all `sessionStorage` entries.
    ///
    /// Returns a JSON object mapping keys to values.
    pub fn get_session_storage_script() -> &'static str {
        r#"(() => {
    const data = {};
    try {
        for (let i = 0; i < sessionStorage.length; i++) {
            const key = sessionStorage.key(i);
            if (key !== null) {
                data[key] = sessionStorage.getItem(key);
            }
        }
    } catch (e) {
        // sessionStorage may be blocked by security policy
        return JSON.stringify({ __error: e.message });
    }
    return JSON.stringify(data);
})()"#
    }

    /// Generate JavaScript that restores `sessionStorage` from a map.
    pub fn set_session_storage_script(entries: &HashMap<String, String>) -> String {
        let json = serde_json::to_string(entries).unwrap_or_else(|_| "{}".to_string());
        format!(
            r#"(() => {{
    try {{
        const entries = JSON.parse('{}');
        for (const [key, value] of Object.entries(entries)) {{
            sessionStorage.setItem(key, value);
        }}
        return true;
    }} catch (e) {{
        return false;
    }}
}})()"#,
            json.replace('\\', "\\\\").replace('\'', "\\'")
        )
    }

    /// Generate JavaScript that clears all cookies for the current domain.
    pub fn clear_cookies_script() -> &'static str {
        r#"(() => {
    const cookies = document.cookie.split(';');
    const paths = ['/', window.location.pathname];
    const domain = window.location.hostname;
    const domainParts = domain.split('.');

    // Build list of domain variations to try
    const domains = ['', domain];
    for (let i = 1; i < domainParts.length; i++) {
        domains.push('.' + domainParts.slice(i).join('.'));
    }

    let cleared = 0;
    cookies.forEach(cookie => {
        const eqIdx = cookie.indexOf('=');
        if (eqIdx < 0) return;
        const name = cookie.substring(0, eqIdx).trim();
        if (!name) return;

        // Try clearing with various domain/path combinations
        domains.forEach(d => {
            paths.forEach(p => {
                let str = name + '=; expires=Thu, 01 Jan 1970 00:00:00 GMT; path=' + p;
                if (d) str += '; domain=' + d;
                document.cookie = str;
            });
        });
        cleared++;
    });

    return JSON.stringify({ cleared: cleared });
})()"#
    }

    /// Generate JavaScript that captures a complete tab state snapshot
    /// (cookies, localStorage, sessionStorage) in one call.
    pub fn capture_tab_state_script() -> &'static str {
        r#"(() => {
    const state = {
        url: window.location.href,
        title: document.title,
        cookies: [],
        local_storage: {},
        session_storage: {}
    };

    // Cookies
    if (document.cookie) {
        document.cookie.split(';').forEach(pair => {
            const trimmed = pair.trim();
            if (!trimmed) return;
            const eqIdx = trimmed.indexOf('=');
            if (eqIdx < 0) return;
            state.cookies.push({
                name: decodeURIComponent(trimmed.substring(0, eqIdx).trim()),
                value: decodeURIComponent(trimmed.substring(eqIdx + 1).trim()),
                domain: window.location.hostname,
                path: '/',
                expires: null,
                http_only: false,
                secure: window.location.protocol === 'https:',
                same_site: null
            });
        });
    }

    // localStorage
    try {
        for (let i = 0; i < localStorage.length; i++) {
            const key = localStorage.key(i);
            if (key !== null) {
                state.local_storage[key] = localStorage.getItem(key);
            }
        }
    } catch (e) { /* blocked */ }

    // sessionStorage
    try {
        for (let i = 0; i < sessionStorage.length; i++) {
            const key = sessionStorage.key(i);
            if (key !== null) {
                state.session_storage[key] = sessionStorage.getItem(key);
            }
        }
    } catch (e) { /* blocked */ }

    return JSON.stringify(state);
})()"#
    }
}

/// Minimal URI-component encoding for cookie values.
///
/// Encodes characters that are problematic in `document.cookie`
/// assignments: `=`, `;`, space, and `%`.
pub(super) fn js_encode_uri_component(s: &str) -> String {
    let mut result = String::with_capacity(s.len());
    for ch in s.chars() {
        match ch {
            '%' => result.push_str("%25"),
            '=' => result.push_str("%3D"),
            ';' => result.push_str("%3B"),
            ' ' => result.push_str("%20"),
            _ => result.push(ch),
        }
    }
    result
}
