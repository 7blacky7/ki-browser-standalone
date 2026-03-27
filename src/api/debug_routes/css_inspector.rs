//! CSS inspection endpoints for debug tooling.
//!
//! Provides three POST endpoints that use `getComputedStyle`, stylesheet
//! traversal, and `getBoundingClientRect` to expose CSS details for any
//! element identified by a CSS selector:
//!
//! - `POST /debug/css/computed`  — resolved computed styles
//! - `POST /debug/css/matched`   — stylesheet rules that apply to an element
//! - `POST /debug/css/box-model` — margin / border / padding / content geometry

use axum::{
    extract::State,
    http::StatusCode,
    response::IntoResponse,
    routing::post,
    Json, Router,
};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

use crate::api::debug_routes::types::{escape_js, evaluate_in_tab, resolve_tab_id};
use crate::api::routes::ApiResponse;
use crate::api::server::AppState;

// ============================================================================
// Request structs
// ============================================================================

/// Request for `POST /debug/css/computed`.
#[derive(Debug, Deserialize)]
pub struct ComputedStylesRequest {
    /// Target tab ID. Falls back to the active tab when omitted.
    #[serde(default)]
    pub tab_id: Option<String>,

    /// CSS selector for the element to inspect.
    pub selector: String,

    /// CSS property names to include. When empty (or absent), all ~300
    /// computed properties are returned.
    #[serde(default)]
    pub properties: Option<Vec<String>>,
}

/// Request for `POST /debug/css/matched`.
#[derive(Debug, Deserialize)]
pub struct MatchedRulesRequest {
    /// Target tab ID. Falls back to the active tab when omitted.
    #[serde(default)]
    pub tab_id: Option<String>,

    /// CSS selector for the element to inspect.
    pub selector: String,
}

/// Request for `POST /debug/css/box-model`.
#[derive(Debug, Deserialize)]
pub struct BoxModelRequest {
    /// Target tab ID. Falls back to the active tab when omitted.
    #[serde(default)]
    pub tab_id: Option<String>,

    /// CSS selector for the element to inspect.
    pub selector: String,
}

// ============================================================================
// Response structs
// ============================================================================

/// Response for `POST /debug/css/computed`.
#[derive(Debug, Serialize, Deserialize)]
pub struct ComputedStylesResponse {
    /// The CSS selector that was queried.
    pub selector: String,
    /// Tag name of the matched element (lower-cased), e.g. `"div"`.
    pub element_tag: String,
    /// Map of property name → computed value.
    pub styles: HashMap<String, String>,
    /// Number of properties returned.
    pub property_count: usize,
}

/// A single matched stylesheet rule.
#[derive(Debug, Serialize, Deserialize)]
pub struct MatchedRule {
    /// The `selectorText` from the `CSSStyleRule`.
    pub selector_text: String,
    /// Map of CSS property name → value for this rule.
    pub properties: HashMap<String, String>,
    /// Href of the owning stylesheet, if accessible (cross-origin rules are
    /// skipped by the browser sandbox).
    pub source: Option<String>,
}

/// Response for `POST /debug/css/matched`.
#[derive(Debug, Serialize, Deserialize)]
pub struct MatchedRulesResponse {
    /// The CSS selector that was queried.
    pub selector: String,
    /// Tag name of the matched element (lower-cased), e.g. `"div"`.
    pub element_tag: String,
    /// Inline styles declared directly on the element.
    pub inline_styles: HashMap<String, String>,
    /// Rules from all accessible stylesheets whose selector matches the element.
    pub stylesheet_rules: Vec<MatchedRule>,
}

/// A CSS box-model edge (top / right / bottom / left values in pixels).
#[derive(Debug, Serialize, Deserialize)]
pub struct BoxEdge {
    pub top: f64,
    pub right: f64,
    pub bottom: f64,
    pub left: f64,
}

/// The `getBoundingClientRect()` result for an element.
#[derive(Debug, Serialize, Deserialize)]
pub struct BoundingBox {
    pub x: f64,
    pub y: f64,
    pub width: f64,
    pub height: f64,
}

/// Response for `POST /debug/css/box-model`.
#[derive(Debug, Serialize, Deserialize)]
pub struct BoxModelResponse {
    /// The CSS selector that was queried.
    pub selector: String,
    /// The content area (equivalent to `getBoundingClientRect` minus padding/border,
    /// expressed here as the inner bounding edge offsets).
    pub content: BoxEdge,
    /// Computed padding widths.
    pub padding: BoxEdge,
    /// Computed border widths.
    pub border: BoxEdge,
    /// Computed margin widths.
    pub margin: BoxEdge,
    /// The raw `getBoundingClientRect()` result.
    pub bounding_box: BoundingBox,
}

// ============================================================================
// Handlers
// ============================================================================

/// `POST /debug/css/computed`
///
/// Returns the computed CSS styles for the first element matching `selector`.
/// When `properties` is provided, only the listed properties are returned;
/// otherwise all ~300 computed properties are included.
async fn computed_styles(
    State(state): State<AppState>,
    Json(request): Json<ComputedStylesRequest>,
) -> impl IntoResponse {
    let tab_id = match resolve_tab_id(&state, request.tab_id).await {
        Some(id) => id,
        None => {
            return (
                StatusCode::BAD_REQUEST,
                Json(ApiResponse::<()>::error("No active tab")),
            )
                .into_response();
        }
    };

    let escaped_selector = escape_js(&request.selector);

    // Build the property-filter part of the script.
    // If no explicit list is given, enumerate the `CSSStyleDeclaration` by index.
    let props_js = match &request.properties {
        Some(props) if !props.is_empty() => {
            // Convert the Rust Vec into a JS array literal.
            let joined = props
                .iter()
                .map(|p| format!("\"{}\"", escape_js(p)))
                .collect::<Vec<_>>()
                .join(",");
            format!("[{}]", joined)
        }
        _ => {
            // An empty array signals the script to enumerate all properties.
            "[]".to_string()
        }
    };

    let script = format!(
        r#"(function() {{
            var el = document.querySelector("{selector}");
            if (!el) {{
                return JSON.stringify({{ error: "Element not found: {selector}" }});
            }}
            var cs = window.getComputedStyle(el);
            var requested = {props};
            var styles = {{}};
            if (requested.length === 0) {{
                for (var i = 0; i < cs.length; i++) {{
                    var prop = cs[i];
                    styles[prop] = cs.getPropertyValue(prop);
                }}
            }} else {{
                requested.forEach(function(p) {{
                    styles[p] = cs.getPropertyValue(p);
                }});
            }}
            return JSON.stringify({{
                selector: "{selector}",
                element_tag: el.tagName.toLowerCase(),
                styles: styles,
                property_count: Object.keys(styles).length
            }});
        }})()"#,
        selector = escaped_selector,
        props = props_js,
    );

    match evaluate_in_tab(&state, &tab_id, &script).await {
        Ok(json_str) => match serde_json::from_str::<ComputedStylesResponse>(&json_str) {
            Ok(data) => Json(ApiResponse::success(data)).into_response(),
            Err(e) => (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ApiResponse::<()>::error(format!("Parse error: {}", e))),
            )
                .into_response(),
        },
        Err(err_response) => err_response.into_response(),
    }
}

/// `POST /debug/css/matched`
///
/// Returns all stylesheet rules whose selector matches the target element,
/// as well as any inline styles set directly on it.
///
/// Cross-origin stylesheets are silently skipped (the browser sandbox blocks
/// `cssRules` access for them, so each sheet access is wrapped in try/catch).
async fn matched_rules(
    State(state): State<AppState>,
    Json(request): Json<MatchedRulesRequest>,
) -> impl IntoResponse {
    let tab_id = match resolve_tab_id(&state, request.tab_id).await {
        Some(id) => id,
        None => {
            return (
                StatusCode::BAD_REQUEST,
                Json(ApiResponse::<()>::error("No active tab")),
            )
                .into_response();
        }
    };

    let escaped_selector = escape_js(&request.selector);

    let script = format!(
        r#"(function() {{
            var el = document.querySelector("{selector}");
            if (!el) {{
                return JSON.stringify({{ error: "Element not found: {selector}" }});
            }}

            // Collect inline styles.
            var inlineStyles = {{}};
            var inlineDecl = el.style;
            for (var k = 0; k < inlineDecl.length; k++) {{
                var iprop = inlineDecl[k];
                inlineStyles[iprop] = inlineDecl.getPropertyValue(iprop);
            }}

            // Walk all stylesheets.
            var matchedRules = [];
            var sheets = document.styleSheets;
            for (var i = 0; i < sheets.length; i++) {{
                var sheet = sheets[i];
                var rules;
                try {{
                    rules = sheet.cssRules || sheet.rules;
                }} catch (corsErr) {{
                    // Cross-origin sheet — skip silently.
                    continue;
                }}
                if (!rules) {{ continue; }}
                for (var j = 0; j < rules.length; j++) {{
                    var rule = rules[j];
                    if (rule.type !== 1) {{ continue; }} // Only CSSStyleRule
                    var selectorText = rule.selectorText;
                    var matches = false;
                    try {{
                        matches = el.matches(selectorText);
                    }} catch (selectorErr) {{
                        // Invalid / unsupported selector — skip.
                        continue;
                    }}
                    if (!matches) {{ continue; }}
                    var props = {{}};
                    var decl = rule.style;
                    for (var p = 0; p < decl.length; p++) {{
                        var rp = decl[p];
                        props[rp] = decl.getPropertyValue(rp);
                    }}
                    matchedRules.push({{
                        selector_text: selectorText,
                        properties: props,
                        source: sheet.href || null
                    }});
                }}
            }}

            return JSON.stringify({{
                selector: "{selector}",
                element_tag: el.tagName.toLowerCase(),
                inline_styles: inlineStyles,
                stylesheet_rules: matchedRules
            }});
        }})()"#,
        selector = escaped_selector,
    );

    match evaluate_in_tab(&state, &tab_id, &script).await {
        Ok(json_str) => match serde_json::from_str::<MatchedRulesResponse>(&json_str) {
            Ok(data) => Json(ApiResponse::success(data)).into_response(),
            Err(e) => (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ApiResponse::<()>::error(format!("Parse error: {}", e))),
            )
                .into_response(),
        },
        Err(err_response) => err_response.into_response(),
    }
}

/// `POST /debug/css/box-model`
///
/// Returns the complete CSS box-model (margin, border, padding, content) and
/// `getBoundingClientRect()` data for the first element matching `selector`.
async fn box_model(
    State(state): State<AppState>,
    Json(request): Json<BoxModelRequest>,
) -> impl IntoResponse {
    let tab_id = match resolve_tab_id(&state, request.tab_id).await {
        Some(id) => id,
        None => {
            return (
                StatusCode::BAD_REQUEST,
                Json(ApiResponse::<()>::error("No active tab")),
            )
                .into_response();
        }
    };

    let escaped_selector = escape_js(&request.selector);

    let script = format!(
        r#"(function() {{
            var el = document.querySelector("{selector}");
            if (!el) {{
                return JSON.stringify({{ error: "Element not found: {selector}" }});
            }}
            var cs = window.getComputedStyle(el);
            var rect = el.getBoundingClientRect();

            function parseFloat2(v) {{
                return parseFloat(v) || 0;
            }}

            var margin = {{
                top:    parseFloat2(cs.getPropertyValue("margin-top")),
                right:  parseFloat2(cs.getPropertyValue("margin-right")),
                bottom: parseFloat2(cs.getPropertyValue("margin-bottom")),
                left:   parseFloat2(cs.getPropertyValue("margin-left"))
            }};
            var padding = {{
                top:    parseFloat2(cs.getPropertyValue("padding-top")),
                right:  parseFloat2(cs.getPropertyValue("padding-right")),
                bottom: parseFloat2(cs.getPropertyValue("padding-bottom")),
                left:   parseFloat2(cs.getPropertyValue("padding-left"))
            }};
            var border = {{
                top:    parseFloat2(cs.getPropertyValue("border-top-width")),
                right:  parseFloat2(cs.getPropertyValue("border-right-width")),
                bottom: parseFloat2(cs.getPropertyValue("border-bottom-width")),
                left:   parseFloat2(cs.getPropertyValue("border-left-width"))
            }};

            // Content box edges are inset from the bounding rect by border + padding.
            var content = {{
                top:    rect.top    + border.top    + padding.top,
                right:  rect.right  - border.right  - padding.right,
                bottom: rect.bottom - border.bottom - padding.bottom,
                left:   rect.left   + border.left   + padding.left
            }};

            return JSON.stringify({{
                selector: "{selector}",
                content: content,
                padding: padding,
                border: border,
                margin: margin,
                bounding_box: {{
                    x: rect.x,
                    y: rect.y,
                    width: rect.width,
                    height: rect.height
                }}
            }});
        }})()"#,
        selector = escaped_selector,
    );

    match evaluate_in_tab(&state, &tab_id, &script).await {
        Ok(json_str) => match serde_json::from_str::<BoxModelResponse>(&json_str) {
            Ok(data) => Json(ApiResponse::success(data)).into_response(),
            Err(e) => (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ApiResponse::<()>::error(format!("Parse error: {}", e))),
            )
                .into_response(),
        },
        Err(err_response) => err_response.into_response(),
    }
}

// ============================================================================
// Router
// ============================================================================

/// Build the CSS-inspector sub-router.
pub fn css_routes() -> Router<AppState> {
    Router::new()
        .route("/debug/css/computed", post(computed_styles))
        .route("/debug/css/matched", post(matched_rules))
        .route("/debug/css/box-model", post(box_model))
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    // --- Request deserialization ---

    #[test]
    fn test_computed_styles_request_minimal() {
        let json = r#"{"selector": "div.main"}"#;
        let req: ComputedStylesRequest = serde_json::from_str(json).unwrap();
        assert_eq!(req.selector, "div.main");
        assert!(req.tab_id.is_none());
        assert!(req.properties.is_none());
    }

    #[test]
    fn test_computed_styles_request_with_properties() {
        let json = r#"{
            "tab_id": "tab_1",
            "selector": "body",
            "properties": ["color", "font-size", "margin-top"]
        }"#;
        let req: ComputedStylesRequest = serde_json::from_str(json).unwrap();
        assert_eq!(req.tab_id, Some("tab_1".to_string()));
        let expected: Vec<String> = vec!["color".into(), "font-size".into(), "margin-top".into()];
        assert_eq!(req.properties, Some(expected));
    }

    #[test]
    fn test_matched_rules_request_defaults() {
        let json = r##"{"selector": "#header"}"##;
        let req: MatchedRulesRequest = serde_json::from_str(json).unwrap();
        assert_eq!(req.selector, "#header");
        assert!(req.tab_id.is_none());
    }

    #[test]
    fn test_box_model_request_with_tab() {
        let json = r#"{"tab_id": "tab_2", "selector": "section.hero"}"#;
        let req: BoxModelRequest = serde_json::from_str(json).unwrap();
        assert_eq!(req.tab_id, Some("tab_2".to_string()));
        assert_eq!(req.selector, "section.hero");
    }

    // --- Response serialization ---

    #[test]
    fn test_computed_styles_response_roundtrip() {
        let mut styles = HashMap::new();
        styles.insert("color".to_string(), "rgb(0, 0, 0)".to_string());
        styles.insert("font-size".to_string(), "16px".to_string());

        let resp = ComputedStylesResponse {
            selector: "div.main".to_string(),
            element_tag: "div".to_string(),
            property_count: 2,
            styles,
        };
        let json = serde_json::to_string(&resp).unwrap();
        let back: ComputedStylesResponse = serde_json::from_str(&json).unwrap();
        assert_eq!(back.selector, "div.main");
        assert_eq!(back.element_tag, "div");
        assert_eq!(back.property_count, 2);
        assert_eq!(back.styles.get("color").map(String::as_str), Some("rgb(0, 0, 0)"));
    }

    #[test]
    fn test_matched_rules_response_roundtrip() {
        let resp = MatchedRulesResponse {
            selector: "p".to_string(),
            element_tag: "p".to_string(),
            inline_styles: HashMap::new(),
            stylesheet_rules: vec![MatchedRule {
                selector_text: "p".to_string(),
                properties: {
                    let mut m = HashMap::new();
                    m.insert("line-height".to_string(), "1.5".to_string());
                    m
                },
                source: Some("https://example.com/style.css".to_string()),
            }],
        };
        let json = serde_json::to_string(&resp).unwrap();
        let back: MatchedRulesResponse = serde_json::from_str(&json).unwrap();
        assert_eq!(back.stylesheet_rules.len(), 1);
        assert_eq!(back.stylesheet_rules[0].selector_text, "p");
        assert_eq!(
            back.stylesheet_rules[0].source.as_deref(),
            Some("https://example.com/style.css")
        );
    }

    #[test]
    fn test_box_model_response_roundtrip() {
        let resp = BoxModelResponse {
            selector: "div#app".to_string(),
            content: BoxEdge { top: 10.0, right: 200.0, bottom: 100.0, left: 10.0 },
            padding: BoxEdge { top: 8.0, right: 8.0, bottom: 8.0, left: 8.0 },
            border: BoxEdge { top: 1.0, right: 1.0, bottom: 1.0, left: 1.0 },
            margin: BoxEdge { top: 16.0, right: 0.0, bottom: 16.0, left: 0.0 },
            bounding_box: BoundingBox { x: 9.0, y: 9.0, width: 182.0, height: 82.0 },
        };
        let json = serde_json::to_string(&resp).unwrap();
        let back: BoxModelResponse = serde_json::from_str(&json).unwrap();
        assert_eq!(back.selector, "div#app");
        assert_eq!(back.padding.top, 8.0);
        assert_eq!(back.border.left, 1.0);
        assert_eq!(back.margin.top, 16.0);
        assert_eq!(back.bounding_box.width, 182.0);
    }

    #[test]
    fn test_matched_rule_without_source() {
        let rule = MatchedRule {
            selector_text: "h1".to_string(),
            properties: HashMap::new(),
            source: None,
        };
        let json = serde_json::to_string(&rule).unwrap();
        // `source` must be serialized as null (not omitted) for JS consumers.
        assert!(json.contains("\"source\":null"));
    }

    #[test]
    fn test_box_edge_all_zero() {
        let edge = BoxEdge { top: 0.0, right: 0.0, bottom: 0.0, left: 0.0 };
        let json = serde_json::to_string(&edge).unwrap();
        let back: BoxEdge = serde_json::from_str(&json).unwrap();
        assert_eq!(back.top, 0.0);
        assert_eq!(back.right, 0.0);
    }
}
