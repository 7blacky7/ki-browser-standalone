//! API Guard Middleware — catches common agent mistakes and returns helpful error messages.
//!
//! Instead of silent failures or cryptic errors, this middleware:
//! - Suggests correct endpoints for common typos (e.g. `/tabs/create` → `/tabs/new`)
//! - Validates request bodies for known parameter mistakes
//! - Adds warning headers when usage patterns are risky (e.g. evaluate right after navigate)

use axum::{
    body::Body,
    extract::State,
    http::{Request, Response, StatusCode, Uri},
    middleware::Next,
    response::IntoResponse,
    Json,
};
use serde_json::json;
use std::sync::Arc;
use tokio::sync::RwLock;

/// Tracks recent API calls for pattern detection (e.g. navigate → evaluate without wait).
#[derive(Clone, Default)]
pub struct GuardState {
    /// Last navigate timestamp (global — any tab)
    last_navigate: Arc<RwLock<Option<std::time::Instant>>>,
}

/// Known endpoint typos and their corrections.
fn endpoint_suggestions(path: &str) -> Option<&'static str> {
    match path {
        "/tabs/create" | "/tab/new" | "/tab/create" | "/newtab" => Some("/tabs/new"),
        "/tabs/delete" | "/tab/close" | "/tab/delete" => Some("/tabs/close"),
        "/nav" | "/goto" | "/load" => Some("/navigate"),
        "/exec" | "/eval" | "/js" | "/run" => Some("/evaluate"),
        "/capture" | "/snap" | "/screen" => Some("/screenshot"),
        "/type_text" | "/input" | "/sendkeys" => Some("/type"),
        "/dom/elements" | "/dom/find" | "/dom/query" => Some("/dom/element"),
        "/dom/annotated" | "/annotate" => Some("/dom/annotate"),
        "/dom/tree" | "/dom/html" | "/dom/dom" => Some("/dom/snapshot"),
        "/vision/annotated" | "/vision/screenshot" => Some("/vision/annotated"),
        "/vision/label" => Some("/vision/labels"),
        "/frame" | "/iframes" => Some("/frames"),
        _ => None,
    }
}

/// Middleware: catch unknown endpoints and suggest corrections.
pub async fn guard_fallback(uri: Uri) -> impl IntoResponse {
    let path = uri.path();
    if let Some(suggestion) = endpoint_suggestions(path) {
        (
            StatusCode::NOT_FOUND,
            Json(json!({
                "success": false,
                "error": format!("Unknown endpoint: {}. Did you mean {}?", path, suggestion),
                "suggestion": suggestion
            })),
        )
    } else {
        (
            StatusCode::NOT_FOUND,
            Json(json!({
                "success": false,
                "error": format!("Unknown endpoint: {}. See /swagger-ui for available endpoints.", path),
            })),
        )
    }
}

/// Middleware: validate request patterns and add warning headers.
pub async fn guard_layer(
    State(guard): State<GuardState>,
    request: Request<Body>,
    next: Next,
) -> Response<Body> {
    let path = request.uri().path().to_string();
    let method = request.method().clone();

    // Track navigate calls for race-condition detection
    if path == "/navigate" && method == "POST" {
        let mut last = guard.last_navigate.write().await;
        *last = Some(std::time::Instant::now());
    }

    // Detect evaluate/click/type immediately after navigate (< 2s)
    let mut warning: Option<String> = None;
    if matches!(path.as_str(), "/evaluate" | "/click" | "/type" | "/dom/snapshot" | "/dom/annotate") {
        let last = guard.last_navigate.read().await;
        if let Some(nav_time) = *last {
            let elapsed = nav_time.elapsed().as_millis();
            if elapsed < 2000 {
                warning = Some(format!(
                    "Called {} only {}ms after /navigate. Page may still be loading. Poll readyState + location.href first.",
                    path, elapsed
                ));
            }
        }
    }

    // Run the actual handler
    let mut response = next.run(request).await;

    // Add warning header if detected
    if let Some(warn_msg) = warning {
        if let Ok(val) = warn_msg.parse() {
            response.headers_mut().insert("X-Agent-Warning", val);
        }
    }

    // Rewrite 422 (Unprocessable Entity) Serde errors into friendly messages
    if response.status() == StatusCode::UNPROCESSABLE_ENTITY {
        let (parts, body) = response.into_parts();
        if let Ok(bytes) = axum::body::to_bytes(body, 16384).await {
            let body_str = String::from_utf8_lossy(&bytes);
            let friendly = friendly_json_error(&path, &body_str);
            let new_body = serde_json::to_vec(&json!({
                "success": false,
                "error": friendly
            }))
            .unwrap_or_else(|_| bytes.to_vec());
            return Response::from_parts(parts, Body::from(new_body));
        }
        // If we couldn't read the body, reconstruct a generic error
        return (StatusCode::UNPROCESSABLE_ENTITY, Json(json!({
            "success": false,
            "error": "Ungueltige Request-Daten"
        }))).into_response();
    }

    response
}

/// Rewrites cryptic Serde/Axum JSON rejection messages into agent-friendly hints.
///
/// Called from route handlers via the `AgentJson` extractor.
pub fn friendly_json_error(path: &str, raw_error: &str) -> String {
    // Parameter-specific hints based on endpoint + missing field
    let hint = match (path, raw_error) {
        (_, e) if e.contains("missing field `url`") => {
            Some("Parameter 'url' ist erforderlich. Beispiel: {\"tab_id\":\"...\", \"url\":\"https://example.com\"}")
        }
        (_, e) if e.contains("missing field `text`") => {
            Some("Parameter 'text' ist erforderlich. Beispiel: {\"tab_id\":\"...\", \"text\":\"Hello\", \"selector\":\"#input\"}")
        }
        (_, e) if e.contains("missing field `operations`") => {
            Some("Nutze 'operations' statt 'commands'. Beispiel: {\"operations\":[{\"command\":{\"Navigate\":{\"url\":\"...\"}}}]}")
        }
        (_, e) if e.contains("missing field `script`") => {
            Some("Parameter 'script' ist erforderlich. Beispiel: {\"tab_id\":\"...\", \"script\":\"document.title\"}")
        }
        (_, e) if e.contains("missing field `selector`") => {
            Some("Parameter 'selector' ist erforderlich. Beispiel: {\"tab_id\":\"...\", \"selector\":\"#element\"}")
        }
        _ => None,
    };

    // Unknown field suggestions
    let field_hint = if raw_error.contains("unknown field `direction`") {
        Some("Nutze 'delta_y' statt 'direction'. Beispiel: {\"tab_id\":\"...\", \"delta_y\":300}")
    } else if raw_error.contains("unknown field `commands`") {
        Some("Nutze 'operations' statt 'commands'.")
    } else if raw_error.contains("unknown field `query`") || raw_error.contains("unknown field `code`") {
        Some("Nutze 'script' statt 'query'/'code'. Beispiel: {\"tab_id\":\"...\", \"script\":\"...\"}")
    } else {
        None
    };

    if let Some(h) = hint.or(field_hint) {
        format!("{} ({})", h, raw_error)
    } else {
        format!("Ungueltige Request-Daten: {}", raw_error)
    }
}
