//! JavaScript-based element inspection triggered from the right-click context menu.
//!
//! Generates the JS snippet that runs `document.elementFromPoint()` and
//! extracts XPath, CSS selector, bounding box, ARIA role, and semantic
//! attributes via the live DOM. Parses the JSON result into `ElementDetails`.
//!
//! The JS runs inside a background thread with a short-lived tokio runtime
//! because `execute_js_with_result` is async but the GUI render loop is sync.

use crate::gui::element_inspector::ElementDetails;

/// JavaScript snippet to inspect the DOM element at the given viewport coordinates.
///
/// Evaluates `document.elementFromPoint(x, y)` and collects tag, type, title,
/// inner text, XPath (id-shortcut and full numeric), CSS selector, bounding box,
/// ARIA role, id, classes, href, src, placeholder, and interactivity flags.
/// Returns a JSON string; returns `{"error":"no element"}` when nothing is found.
pub(super) fn element_inspect_js(x: f64, y: f64) -> String {
    format!(
        r##"(function() {{
            var el = document.elementFromPoint({x}, {y});
            if (!el) return JSON.stringify({{error: "no element"}});
            function getXPath(el) {{
                if (!el.parentNode) return "";
                var siblings = el.parentNode.children;
                var tag = el.tagName.toLowerCase();
                var idx = Array.from(siblings).filter(function(s) {{ return s.tagName === el.tagName; }}).indexOf(el) + 1;
                return getXPath(el.parentNode) + "/" + tag + (idx > 1 ? "[" + idx + "]" : "");
            }}
            function getFullXPath(el) {{
                if (!el.parentNode) return "";
                var siblings = el.parentNode.children;
                var tag = el.tagName.toLowerCase();
                var idx = Array.from(siblings).filter(function(s) {{ return s.tagName === el.tagName; }}).indexOf(el) + 1;
                return getFullXPath(el.parentNode) + "/" + tag + "[" + idx + "]";
            }}
            function getCssSelector(el) {{
                if (el.id) return "#" + el.id;
                var path = [];
                while (el && el.nodeType === 1) {{
                    var sel = el.tagName.toLowerCase();
                    if (el.id) {{ path.unshift("#" + el.id); break; }}
                    var sib = el, nth = 1;
                    while (sib = sib.previousElementSibling) {{ if (sib.tagName === el.tagName) nth++; }}
                    if (nth > 1) sel += ":nth-of-type(" + nth + ")";
                    path.unshift(sel);
                    el = el.parentNode;
                }}
                return path.join(" > ");
            }}
            var rect = el.getBoundingClientRect();
            return JSON.stringify({{
                tag: el.tagName.toLowerCase(),
                type: el.type || el.tagName.toLowerCase(),
                title: el.title || "",
                text: (el.innerText || el.value || "").substring(0, 200),
                xpath: getXPath(el),
                fullXpath: getFullXPath(el),
                role: el.getAttribute("role") || "",
                id: el.id || "",
                classes: el.className || "",
                href: el.href || "",
                src: el.src || "",
                placeholder: el.placeholder || "",
                cssSelector: getCssSelector(el),
                visible: rect.width > 0 && rect.height > 0,
                interactive: el.matches("a,button,input,select,textarea,[tabindex],[onclick]"),
                x: rect.x, y: rect.y, w: rect.width, h: rect.height
            }});
        }})()"##,
        x = x,
        y = y
    )
}

/// Parse the JSON result from `element_inspect_js` into `ElementDetails`.
///
/// CEF returns JS results as JSON-escaped strings; this function strips the
/// outer quotes and unescapes inner sequences before parsing.
/// Returns `None` if the JSON is malformed or the script returned an error object.
pub(super) fn parse_element_details(raw_result: &str) -> Option<ElementDetails> {
    // CEF may wrap the JSON value in an extra layer of string-escaping.
    let json_str = raw_result.trim_matches('"');
    let json_str = json_str.replace("\\\"", "\"");
    let json_str = json_str.replace("\\\\", "\\");

    let val = serde_json::from_str::<serde_json::Value>(&json_str).ok()?;

    // Script signals "no element at these coordinates" via an error key.
    if val.get("error").is_some() {
        return None;
    }

    Some(ElementDetails {
        tag: val["tag"].as_str().unwrap_or("").to_string(),
        element_type: val["type"].as_str().unwrap_or("").to_string(),
        title: val["title"].as_str().unwrap_or("").to_string(),
        text_value: val["text"].as_str().unwrap_or("").to_string(),
        xpath: val["xpath"].as_str().unwrap_or("").to_string(),
        full_xpath: val["fullXpath"].as_str().unwrap_or("").to_string(),
        role: val["role"].as_str().unwrap_or("").to_string(),
        id: val["id"].as_str().unwrap_or("").to_string(),
        classes: val["classes"].as_str().unwrap_or("").to_string(),
        href: val["href"].as_str().unwrap_or("").to_string(),
        src: val["src"].as_str().unwrap_or("").to_string(),
        placeholder: val["placeholder"].as_str().unwrap_or("").to_string(),
        css_selector: val["cssSelector"].as_str().unwrap_or("").to_string(),
        is_visible: Some(val["visible"].as_bool().unwrap_or(true)),
        is_interactive: Some(val["interactive"].as_bool().unwrap_or(false)),
        x: val["x"].as_f64().unwrap_or(0.0) as f32,
        y: val["y"].as_f64().unwrap_or(0.0) as f32,
        w: val["w"].as_f64().unwrap_or(0.0) as f32,
        h: val["h"].as_f64().unwrap_or(0.0) as f32,
        ..Default::default()
    })
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_element_inspect_js_contains_coordinates() {
        let js = element_inspect_js(42.0, 84.5);
        assert!(js.contains("elementFromPoint(42, 84.5)"));
    }

    #[test]
    fn test_element_inspect_js_contains_required_fields() {
        let js = element_inspect_js(0.0, 0.0);
        assert!(js.contains("getXPath"));
        assert!(js.contains("getFullXPath"));
        assert!(js.contains("getCssSelector"));
        assert!(js.contains("getBoundingClientRect"));
        assert!(js.contains("cssSelector"));
        assert!(js.contains("xpath"));
        assert!(js.contains("fullXpath"));
    }

    #[test]
    fn test_parse_element_details_valid_json() {
        let json = r##"{"tag":"button","type":"submit","title":"Click me","text":"OK",
            "xpath":"/html/body/button","fullXpath":"/html[1]/body[1]/button[1]",
            "role":"button","id":"ok-btn","classes":"btn primary","href":"","src":"",
            "placeholder":"","cssSelector":"#ok-btn","visible":true,"interactive":true,
            "x":100.0,"y":200.0,"w":80.0,"h":30.0}"##;

        let details = parse_element_details(json).expect("should parse valid JSON");
        assert_eq!(details.tag, "button");
        assert_eq!(details.element_type, "submit");
        assert_eq!(details.title, "Click me");
        assert_eq!(details.text_value, "OK");
        assert_eq!(details.xpath, "/html/body/button");
        assert_eq!(details.full_xpath, "/html[1]/body[1]/button[1]");
        assert_eq!(details.role, "button");
        assert_eq!(details.id, "ok-btn");
        assert_eq!(details.css_selector, "#ok-btn");
        assert_eq!(details.is_visible, Some(true));
        assert_eq!(details.is_interactive, Some(true));
        assert_eq!(details.x, 100.0);
        assert_eq!(details.y, 200.0);
        assert_eq!(details.w, 80.0);
        assert_eq!(details.h, 30.0);
    }

    #[test]
    fn test_parse_element_details_error_returns_none() {
        let json = r#"{"error":"no element"}"#;
        assert!(parse_element_details(json).is_none());
    }

    #[test]
    fn test_parse_element_details_malformed_returns_none() {
        assert!(parse_element_details("not json at all").is_none());
        assert!(parse_element_details("").is_none());
    }

    #[test]
    fn test_parse_element_details_cef_escaped_string() {
        // CEF wraps the JSON value in outer quotes and escapes inner ones.
        let escaped = r##""{\"tag\":\"div\",\"type\":\"div\",\"title\":\"\",\"text\":\"\",\"xpath\":\"/div\",\"fullXpath\":\"/div[1]\",\"role\":\"\",\"id\":\"\",\"classes\":\"\",\"href\":\"\",\"src\":\"\",\"placeholder\":\"\",\"cssSelector\":\"div\",\"visible\":true,\"interactive\":false,\"x\":0,\"y\":0,\"w\":0,\"h\":0}""##;
        let details = parse_element_details(escaped).expect("should handle CEF escaping");
        assert_eq!(details.tag, "div");
    }
}
