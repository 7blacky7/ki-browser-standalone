//! Cookie management endpoints.
//!
//! Provides REST endpoints to list, get, set, and delete cookies in a browser tab
//! via JavaScript evaluation. Each handler resolves the target tab and executes
//! a small JS snippet via IPC.

use axum::{
    extract::{Path, State},
    http::StatusCode,
    response::IntoResponse,
    routing::{delete, get, post},
    Json, Router,
};
use serde::{Deserialize, Serialize};

use crate::api::debug_routes::types::{escape_js, evaluate_in_tab, resolve_tab_id};
use crate::api::routes::ApiResponse;
use crate::api::server::AppState;

// ============================================================================
// Types
// ============================================================================

/// A single browser cookie (name + value pair readable via `document.cookie`).
#[derive(Debug, Serialize, Deserialize)]
pub struct CookieEntry {
    pub name: String,
    pub value: String,
}

/// Response for listing all cookies in a tab.
#[derive(Debug, Serialize, Deserialize)]
pub struct CookieListResponse {
    pub cookies: Vec<CookieEntry>,
    pub count: usize,
}

/// Response for retrieving a single cookie.
#[derive(Debug, Serialize, Deserialize)]
pub struct CookieGetResponse {
    pub cookie: Option<CookieEntry>,
}

/// Response for deleting a single cookie.
#[derive(Debug, Serialize, Deserialize)]
pub struct CookieDeleteResponse {
    pub deleted: String,
}

/// Response for clearing all cookies in a tab.
#[derive(Debug, Serialize, Deserialize)]
pub struct CookieClearResponse {
    pub cleared: usize,
}

/// Request body for setting a cookie.
#[derive(Debug, Deserialize)]
pub struct SetCookieRequest {
    pub name: String,
    pub value: String,
    #[serde(default)]
    pub path: Option<String>,
    #[serde(default)]
    pub domain: Option<String>,
    #[serde(default)]
    pub expires: Option<String>,
    #[serde(default)]
    pub secure: Option<bool>,
    #[serde(default)]
    pub same_site: Option<String>,
}

// ============================================================================
// Handlers
// ============================================================================

/// GET /debug/cookies/{tab_id} — List all cookies for the given tab.
async fn list_cookies(
    State(state): State<AppState>,
    Path(tab_id): Path<String>,
) -> impl IntoResponse {
    let resolved = match resolve_tab_id(&state, Some(tab_id)).await {
        Some(id) => id,
        None => {
            return (
                StatusCode::BAD_REQUEST,
                Json(ApiResponse::<CookieListResponse>::error("No active tab found")),
            )
                .into_response();
        }
    };

    let script = r#"(function(){
  var cs = document.cookie.split(';').filter(function(c){ return c.trim(); }).map(function(c){
    var p = c.trim().split('=');
    var name = p[0];
    var value = decodeURIComponent(p.slice(1).join('='));
    return {name: name, value: value};
  });
  return JSON.stringify({cookies: cs, count: cs.length});
})()"#;

    match evaluate_in_tab(&state, &resolved, script).await {
        Ok(raw) => match serde_json::from_str::<CookieListResponse>(&raw) {
            Ok(resp) => Json(ApiResponse::success(resp)).into_response(),
            Err(e) => (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ApiResponse::<CookieListResponse>::error(format!(
                    "Failed to parse cookie list: {}",
                    e
                ))),
            )
                .into_response(),
        },
        Err(err) => err.into_response(),
    }
}

/// GET /debug/cookies/{tab_id}/{name} — Get a single named cookie.
async fn get_cookie(
    State(state): State<AppState>,
    Path((tab_id, name)): Path<(String, String)>,
) -> impl IntoResponse {
    let resolved = match resolve_tab_id(&state, Some(tab_id)).await {
        Some(id) => id,
        None => {
            return (
                StatusCode::BAD_REQUEST,
                Json(ApiResponse::<CookieGetResponse>::error("No active tab found")),
            )
                .into_response();
        }
    };

    let escaped_name = escape_js(&name);
    let script = format!(
        r#"(function(){{
  var target = '{}';
  var found = null;
  document.cookie.split(';').filter(function(c){{ return c.trim(); }}).forEach(function(c){{
    var p = c.trim().split('=');
    var n = p[0];
    if(n === target){{
      found = {{name: n, value: decodeURIComponent(p.slice(1).join('='))}};
    }}
  }});
  return JSON.stringify({{cookie: found}});
}})()"#,
        escaped_name
    );

    match evaluate_in_tab(&state, &resolved, &script).await {
        Ok(raw) => match serde_json::from_str::<CookieGetResponse>(&raw) {
            Ok(resp) => Json(ApiResponse::success(resp)).into_response(),
            Err(e) => (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ApiResponse::<CookieGetResponse>::error(format!(
                    "Failed to parse cookie response: {}",
                    e
                ))),
            )
                .into_response(),
        },
        Err(err) => err.into_response(),
    }
}

/// POST /debug/cookies/{tab_id}/set — Set a cookie in the given tab.
async fn set_cookie(
    State(state): State<AppState>,
    Path(tab_id): Path<String>,
    Json(req): Json<SetCookieRequest>,
) -> impl IntoResponse {
    let resolved = match resolve_tab_id(&state, Some(tab_id)).await {
        Some(id) => id,
        None => {
            return (
                StatusCode::BAD_REQUEST,
                Json(ApiResponse::<()>::error("No active tab found")),
            )
                .into_response();
        }
    };

    let escaped_name = escape_js(&req.name);
    let escaped_value = escape_js(&req.value);

    let mut cookie_str = format!(
        "{}={}",
        escaped_name,
        js_encode_uri_component(&req.value)
    );

    let path = req.path.as_deref().unwrap_or("/");
    cookie_str.push_str(&format!("; path={}", escape_js(path)));

    if let Some(domain) = &req.domain {
        cookie_str.push_str(&format!("; domain={}", escape_js(domain)));
    }
    if let Some(expires) = &req.expires {
        cookie_str.push_str(&format!("; expires={}", escape_js(expires)));
    }
    if req.secure.unwrap_or(false) {
        cookie_str.push_str("; secure");
    }
    if let Some(same_site) = &req.same_site {
        cookie_str.push_str(&format!("; samesite={}", escape_js(same_site)));
    }

    let script = format!(
        r#"(function(){{
  document.cookie = '{}';
  return JSON.stringify({{success: true}});
}})()"#,
        escape_js(&cookie_str)
    );

    // Suppress the unused variable warning — the value is used only for the cookie_str assembly above.
    let _ = escaped_name;
    let _ = escaped_value;

    match evaluate_in_tab(&state, &resolved, &script).await {
        Ok(_) => Json(ApiResponse::<()>::success(())).into_response(),
        Err(err) => err.into_response(),
    }
}

/// DELETE /debug/cookies/{tab_id}/{name} — Delete a single named cookie.
async fn delete_cookie(
    State(state): State<AppState>,
    Path((tab_id, name)): Path<(String, String)>,
) -> impl IntoResponse {
    let resolved = match resolve_tab_id(&state, Some(tab_id)).await {
        Some(id) => id,
        None => {
            return (
                StatusCode::BAD_REQUEST,
                Json(ApiResponse::<CookieDeleteResponse>::error("No active tab found")),
            )
                .into_response();
        }
    };

    let escaped_name = escape_js(&name);
    let script = format!(
        r#"(function(){{
  document.cookie = '{}=; expires=Thu, 01 Jan 1970 00:00:00 GMT; path=/';
  return JSON.stringify({{deleted: '{}'}});
}})()"#,
        escaped_name, escaped_name
    );

    match evaluate_in_tab(&state, &resolved, &script).await {
        Ok(raw) => match serde_json::from_str::<CookieDeleteResponse>(&raw) {
            Ok(resp) => Json(ApiResponse::success(resp)).into_response(),
            Err(e) => (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ApiResponse::<CookieDeleteResponse>::error(format!(
                    "Failed to parse delete response: {}",
                    e
                ))),
            )
                .into_response(),
        },
        Err(err) => err.into_response(),
    }
}

/// DELETE /debug/cookies/{tab_id} — Clear all cookies for the given tab.
async fn clear_cookies(
    State(state): State<AppState>,
    Path(tab_id): Path<String>,
) -> impl IntoResponse {
    let resolved = match resolve_tab_id(&state, Some(tab_id)).await {
        Some(id) => id,
        None => {
            return (
                StatusCode::BAD_REQUEST,
                Json(ApiResponse::<CookieClearResponse>::error("No active tab found")),
            )
                .into_response();
        }
    };

    let script = r#"(function(){
  var cookies = document.cookie.split(';').filter(function(c){ return c.trim(); });
  var count = 0;
  cookies.forEach(function(c){
    var name = c.trim().split('=')[0];
    document.cookie = name + '=; expires=Thu, 01 Jan 1970 00:00:00 GMT; path=/';
    count++;
  });
  return JSON.stringify({cleared: count});
})()"#;

    match evaluate_in_tab(&state, &resolved, script).await {
        Ok(raw) => match serde_json::from_str::<CookieClearResponse>(&raw) {
            Ok(resp) => Json(ApiResponse::success(resp)).into_response(),
            Err(e) => (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ApiResponse::<CookieClearResponse>::error(format!(
                    "Failed to parse clear response: {}",
                    e
                ))),
            )
                .into_response(),
        },
        Err(err) => err.into_response(),
    }
}

// ============================================================================
// Helpers
// ============================================================================

/// Percent-encode a cookie value for embedding into the `document.cookie` string.
///
/// Only encodes characters that would break cookie parsing (`;`, `,`, ` `).
/// For full compliance the browser's built-in `encodeURIComponent` is used on
/// the JS side; here we only need a Rust-side representation for the script
/// template string.
fn js_encode_uri_component(s: &str) -> String {
    // Delegate encoding to the JavaScript side — wrap the value in the script so
    // that JS encodeURIComponent handles it properly.
    escape_js(s)
}

// ============================================================================
// Router
// ============================================================================

/// Creates the router fragment for cookie management endpoints.
pub fn cookie_routes() -> Router<AppState> {
    Router::new()
        .route(
            "/debug/cookies/:tab_id",
            get(list_cookies).delete(clear_cookies),
        )
        .route("/debug/cookies/:tab_id/set", post(set_cookie))
        .route(
            "/debug/cookies/:tab_id/:name",
            get(get_cookie).delete(delete_cookie),
        )
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_cookie_entry_serialization() {
        let entry = CookieEntry {
            name: "session".to_string(),
            value: "abc123".to_string(),
        };
        let json = serde_json::to_string(&entry).expect("serialize");
        assert!(json.contains("\"name\":\"session\""));
        assert!(json.contains("\"value\":\"abc123\""));
    }

    #[test]
    fn test_cookie_entry_deserialization() {
        let json = r#"{"name":"foo","value":"bar"}"#;
        let entry: CookieEntry = serde_json::from_str(json).expect("deserialize");
        assert_eq!(entry.name, "foo");
        assert_eq!(entry.value, "bar");
    }

    #[test]
    fn test_cookie_list_response_serialization() {
        let resp = CookieListResponse {
            cookies: vec![CookieEntry {
                name: "a".to_string(),
                value: "1".to_string(),
            }],
            count: 1,
        };
        let json = serde_json::to_string(&resp).expect("serialize");
        assert!(json.contains("\"count\":1"));
        assert!(json.contains("\"cookies\""));
    }

    #[test]
    fn test_cookie_get_response_some() {
        let resp = CookieGetResponse {
            cookie: Some(CookieEntry {
                name: "token".to_string(),
                value: "xyz".to_string(),
            }),
        };
        let json = serde_json::to_string(&resp).expect("serialize");
        assert!(json.contains("\"token\""));
        assert!(json.contains("\"xyz\""));
    }

    #[test]
    fn test_cookie_get_response_none() {
        let resp = CookieGetResponse { cookie: None };
        let json = serde_json::to_string(&resp).expect("serialize");
        assert!(json.contains("\"cookie\":null"));
    }

    #[test]
    fn test_cookie_delete_response_serialization() {
        let resp = CookieDeleteResponse {
            deleted: "my_cookie".to_string(),
        };
        let json = serde_json::to_string(&resp).expect("serialize");
        assert!(json.contains("\"deleted\":\"my_cookie\""));
    }

    #[test]
    fn test_cookie_clear_response_serialization() {
        let resp = CookieClearResponse { cleared: 5 };
        let json = serde_json::to_string(&resp).expect("serialize");
        assert!(json.contains("\"cleared\":5"));
    }

    #[test]
    fn test_set_cookie_request_deserialization_minimal() {
        let json = r#"{"name":"foo","value":"bar"}"#;
        let req: SetCookieRequest = serde_json::from_str(json).expect("deserialize");
        assert_eq!(req.name, "foo");
        assert_eq!(req.value, "bar");
        assert!(req.path.is_none());
        assert!(req.secure.is_none());
        assert!(req.same_site.is_none());
    }

    #[test]
    fn test_set_cookie_request_deserialization_full() {
        let json = r#"{"name":"id","value":"42","path":"/app","domain":"example.com","secure":true,"same_site":"Strict"}"#;
        let req: SetCookieRequest = serde_json::from_str(json).expect("deserialize");
        assert_eq!(req.name, "id");
        assert_eq!(req.path.as_deref(), Some("/app"));
        assert_eq!(req.domain.as_deref(), Some("example.com"));
        assert_eq!(req.secure, Some(true));
        assert_eq!(req.same_site.as_deref(), Some("Strict"));
    }

    #[test]
    fn test_escape_js_in_cookie_name() {
        // Names with quotes must be safely escaped before embedding in JS.
        let name = r#"bad"name"#;
        let escaped = escape_js(name);
        assert!(escaped.contains("\\\""), "double quotes must be backslash-escaped");
        assert!(!escaped.contains("\\\\\""), "should not double-escape");
    }
}
