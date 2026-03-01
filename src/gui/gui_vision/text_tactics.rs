//! Vision text tactic execution (labels, DOM snapshot, structured data, etc.).
//!
//! Dispatches text-based vision tactics to appropriate JavaScript or DOM
//! snapshot handlers and returns results as pretty-printed JSON strings.
//! Supported tactics: `labels`, `dom_snapshot`, `structured_data`,
//! `content_extract`, `structure_analysis`, `forms`.

use std::sync::Arc;

use uuid::Uuid;

use crate::browser::cef_engine::CefBrowserEngine;

use super::js_execution::{capture_dom_snapshot, execute_js_blocking};

/// Runs a vision text tactic (labels/dom_snapshot/etc.) directly using
/// the CEF engine JavaScript execution. No REST API needed.
///
/// Logs warnings on error and delegates to `run_vision_text_direct_inner`
/// for the actual tactic dispatch.
pub(in crate::gui) fn run_vision_text_direct(
    tactic: &str,
    tab_id: Uuid,
    engine: &Arc<CefBrowserEngine>,
) -> Result<String, String> {
    tracing::debug!("run_vision_text_direct: tactic={}, tab={}", tactic, tab_id);
    let result = run_vision_text_direct_inner(tactic, tab_id, engine);
    if let Err(ref e) = result {
        tracing::warn!("run_vision_text_direct: {}", e);
    }
    result
}

/// Dispatches vision text tactics to the appropriate JS or DOM snapshot handler.
///
/// Supported tactics: `labels`, `dom_snapshot`, `structured_data`,
/// `content_extract`, `structure_analysis`, `forms`. Each tactic runs
/// JavaScript via `execute_js_blocking` or captures a DOM snapshot, then
/// returns the result as a pretty-printed JSON string.
fn run_vision_text_direct_inner(
    tactic: &str,
    tab_id: Uuid,
    engine: &Arc<CefBrowserEngine>,
) -> Result<String, String> {
    match tactic {
        "labels" => {
            let snapshot = capture_dom_snapshot(engine, tab_id)?;
            let labels = crate::browser::vision::generate_labels(&snapshot);
            let response = serde_json::json!({
                "count": labels.len(),
                "labels": labels,
            });
            serde_json::to_string_pretty(&response)
                .map_err(|e| format!("JSON serialization failed: {}", e))
        }
        "dom_snapshot" => {
            let snapshot = capture_dom_snapshot(engine, tab_id)?;
            serde_json::to_string_pretty(&snapshot)
                .map_err(|e| format!("JSON serialization failed: {}", e))
        }
        "structured_data" => {
            let script = r#"(function() {
                var result = { jsonLd: [], openGraph: {}, meta: {}, microdata: [] };
                document.querySelectorAll('script[type="application/ld+json"]').forEach(function(s) {
                    try { result.jsonLd.push(JSON.parse(s.textContent)); } catch(e) {}
                });
                document.querySelectorAll('meta[property^="og:"]').forEach(function(m) {
                    result.openGraph[m.getAttribute('property')] = m.getAttribute('content');
                });
                document.querySelectorAll('meta[name]').forEach(function(m) {
                    result.meta[m.getAttribute('name')] = m.getAttribute('content');
                });
                return JSON.stringify(result);
            })()"#;
            let json_str = execute_js_blocking(engine, tab_id, script)?;
            // Pretty-print the JSON
            match serde_json::from_str::<serde_json::Value>(&json_str) {
                Ok(val) => serde_json::to_string_pretty(&val)
                    .map_err(|e| format!("JSON error: {}", e)),
                Err(_) => Ok(json_str),
            }
        }
        "content_extract" => {
            let script = r#"(function() {
                var article = document.querySelector('article') || document.querySelector('main') || document.body;
                var clone = article.cloneNode(true);
                clone.querySelectorAll('script,style,nav,footer,header,aside,.ad,.ads,.advertisement').forEach(function(el) { el.remove(); });
                var text = clone.innerText || clone.textContent || '';
                return JSON.stringify({
                    title: document.title,
                    url: window.location.href,
                    content: text.trim().substring(0, 50000),
                    length: text.trim().length
                });
            })()"#;
            let json_str = execute_js_blocking(engine, tab_id, script)?;
            match serde_json::from_str::<serde_json::Value>(&json_str) {
                Ok(val) => serde_json::to_string_pretty(&val)
                    .map_err(|e| format!("JSON error: {}", e)),
                Err(_) => Ok(json_str),
            }
        }
        "structure_analysis" => {
            let script = r#"(function() {
                var headings = [];
                document.querySelectorAll('h1,h2,h3,h4,h5,h6').forEach(function(h) {
                    headings.push({ level: parseInt(h.tagName[1]), text: h.textContent.trim().substring(0, 200) });
                });
                var links = document.querySelectorAll('a[href]').length;
                var images = document.querySelectorAll('img').length;
                var forms = document.querySelectorAll('form').length;
                var buttons = document.querySelectorAll('button,input[type="submit"],input[type="button"]').length;
                var inputs = document.querySelectorAll('input,textarea,select').length;
                var sections = [];
                document.querySelectorAll('section,article,nav,aside,main,header,footer').forEach(function(s) {
                    sections.push({ tag: s.tagName.toLowerCase(), id: s.id || null, className: s.className || null });
                });
                return JSON.stringify({
                    title: document.title,
                    url: window.location.href,
                    headings: headings,
                    counts: { links: links, images: images, forms: forms, buttons: buttons, inputs: inputs },
                    sections: sections,
                    pageType: document.querySelector('article') ? 'article' : (forms > 0 ? 'form' : 'general')
                });
            })()"#;
            let json_str = execute_js_blocking(engine, tab_id, script)?;
            match serde_json::from_str::<serde_json::Value>(&json_str) {
                Ok(val) => serde_json::to_string_pretty(&val)
                    .map_err(|e| format!("JSON error: {}", e)),
                Err(_) => Ok(json_str),
            }
        }
        "forms" => {
            let script = r#"(function() {
                var forms = [];
                document.querySelectorAll('form').forEach(function(f, fi) {
                    var fields = [];
                    f.querySelectorAll('input,textarea,select,button').forEach(function(el) {
                        fields.push({
                            tag: el.tagName.toLowerCase(),
                            type: el.type || null,
                            name: el.name || null,
                            id: el.id || null,
                            placeholder: el.placeholder || null,
                            required: el.required || false,
                            value: el.type === 'password' ? '***' : (el.value || '').substring(0, 100)
                        });
                    });
                    forms.push({
                        index: fi,
                        action: f.action || null,
                        method: (f.method || 'GET').toUpperCase(),
                        id: f.id || null,
                        name: f.name || null,
                        fields: fields
                    });
                });
                return JSON.stringify({ count: forms.length, forms: forms });
            })()"#;
            let json_str = execute_js_blocking(engine, tab_id, script)?;
            match serde_json::from_str::<serde_json::Value>(&json_str) {
                Ok(val) => serde_json::to_string_pretty(&val)
                    .map_err(|e| format!("JSON error: {}", e)),
                Err(_) => Ok(json_str),
            }
        }
        other => Err(format!("Unknown text tactic: {}", other)),
    }
}
