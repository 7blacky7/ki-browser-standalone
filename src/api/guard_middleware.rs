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

    response
}
