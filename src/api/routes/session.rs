//! Session inheritance routes: import, export, list and delete encrypted
//! login-session bundles.
//!
//! These routes are intentionally NOT in the auth whitelist — when an API
//! token is configured they require `Authorization: Bearer <token>` (a browser
//! extension can attach that header, unlike a plain page). Cookie values are
//! never logged and never returned by `GET /session/list`.

use axum::{
    extract::{Path, State},
    http::StatusCode,
    response::IntoResponse,
    Json,
};
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

use crate::api::server::AppState;
use crate::api::session_store::{Bundle, SessionMeta};
use super::types::ApiResponse;

/// Response of `POST /session/import` and `POST /session/export`.
#[derive(Debug, Serialize, ToSchema)]
pub struct SessionIdResponse {
    pub session_id: String,
    pub origin: String,
}

/// Request body of `POST /session/export`.
#[derive(Debug, Deserialize, ToSchema)]
pub struct ExportSessionRequest {
    /// The running tab to snapshot into a bundle.
    pub tab_id: String,
}

fn store_or_503(state: &AppState) -> Result<&crate::api::session_store::SessionStore, axum::response::Response> {
    state.session_store.as_ref().ok_or_else(|| {
        (
            StatusCode::SERVICE_UNAVAILABLE,
            Json(ApiResponse::<()>::error("Session store unavailable")),
        ).into_response()
    })
}

/// POST /session/import — persist a session bundle (encrypted), return its id.
#[utoipa::path(
    post,
    path = "/login-session/import",
    tag = "session",
    request_body = Object,
    responses(
        (status = 200, description = "Session stored", body = SessionIdResponse),
        (status = 503, description = "Session store unavailable")
    )
)]
pub async fn import_session(
    State(state): State<AppState>,
    Json(bundle): Json<Bundle>,
) -> impl IntoResponse {
    if !state.is_enabled().await {
        return (StatusCode::SERVICE_UNAVAILABLE, Json(ApiResponse::<()>::error("API is disabled"))).into_response();
    }
    let store = match store_or_503(&state) {
        Ok(s) => s,
        Err(r) => return r,
    };
    if bundle.origin.trim().is_empty() {
        return (StatusCode::BAD_REQUEST, Json(ApiResponse::<()>::error("bundle.origin is required"))).into_response();
    }
    let origin = bundle.origin.clone();
    match store.save(&bundle).await {
        Ok(session_id) => {
            // Never log cookie values — only counts.
            tracing::info!(
                "Imported session for origin {} ({} cookies, {} storage origins)",
                origin, bundle.cookies.len(), bundle.storage.len()
            );
            Json(ApiResponse::success(SessionIdResponse { session_id, origin })).into_response()
        }
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ApiResponse::<()>::error(format!("Failed to store session: {}", e))),
        ).into_response(),
    }
}

/// GET /session/list — metadata for all stored sessions (no cookie values).
#[utoipa::path(
    get,
    path = "/login-session/list",
    tag = "session",
    responses(
        (status = 200, description = "List of stored sessions (no secrets)"),
        (status = 503, description = "Session store unavailable")
    )
)]
pub async fn list_sessions(State(state): State<AppState>) -> impl IntoResponse {
    if !state.is_enabled().await {
        return (StatusCode::SERVICE_UNAVAILABLE, Json(ApiResponse::<()>::error("API is disabled"))).into_response();
    }
    let store = match store_or_503(&state) {
        Ok(s) => s,
        Err(r) => return r,
    };
    match store.list().await {
        Ok(sessions) => Json(ApiResponse::success(sessions)).into_response(),
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ApiResponse::<Vec<SessionMeta>>::error(format!("Failed to list sessions: {}", e))),
        ).into_response(),
    }
}

/// DELETE /session/{id} — delete a stored session.
#[utoipa::path(
    delete,
    path = "/login-session/{id}",
    tag = "session",
    params(("id" = String, Path, description = "Session id")),
    responses(
        (status = 200, description = "Session deleted"),
        (status = 404, description = "Unknown session id"),
        (status = 503, description = "Session store unavailable")
    )
)]
pub async fn delete_session(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> impl IntoResponse {
    if !state.is_enabled().await {
        return (StatusCode::SERVICE_UNAVAILABLE, Json(ApiResponse::<()>::error("API is disabled"))).into_response();
    }
    let store = match store_or_503(&state) {
        Ok(s) => s,
        Err(r) => return r,
    };
    match store.delete(&id).await {
        Ok(true) => Json(ApiResponse::success(())).into_response(),
        Ok(false) => (
            StatusCode::NOT_FOUND,
            Json(ApiResponse::<()>::error(format!("Unknown session id: {}", id))),
        ).into_response(),
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ApiResponse::<()>::error(format!("Failed to delete session: {}", e))),
        ).into_response(),
    }
}

/// POST /session/export — build a bundle from a running tab and store it.
#[utoipa::path(
    post,
    path = "/login-session/export",
    tag = "session",
    request_body = ExportSessionRequest,
    responses(
        (status = 200, description = "Session exported and stored", body = SessionIdResponse),
        (status = 404, description = "Tab not found"),
        (status = 503, description = "Session store / CDP unavailable")
    )
)]
pub async fn export_session(
    State(state): State<AppState>,
    Json(req): Json<ExportSessionRequest>,
) -> impl IntoResponse {
    if !state.is_enabled().await {
        return (StatusCode::SERVICE_UNAVAILABLE, Json(ApiResponse::<()>::error("API is disabled"))).into_response();
    }
    let store = match store_or_503(&state) {
        Ok(s) => s.clone(),
        Err(r) => return r,
    };
    let cdp = match &state.cdp_client {
        Some(c) => c.clone(),
        None => return (StatusCode::SERVICE_UNAVAILABLE, Json(ApiResponse::<()>::error("CDP client unavailable"))).into_response(),
    };

    // Resolve the tab's current URL (origin) from local browser state.
    let tab_url = {
        let bs = state.browser_state.read().await;
        match bs.tabs.get(&req.tab_id) {
            Some(t) => t.url.clone(),
            None => {
                return (
                    StatusCode::NOT_FOUND,
                    Json(ApiResponse::<()>::error(format!("Tab not found: {}", req.tab_id))),
                ).into_response();
            }
        }
    };
    let origin = origin_of(&tab_url);

    // Resolve the tab's CDP WebSocket URL by its current page URL.
    let ws_url = match cdp.find_target_ws_url(&tab_url).await {
        Ok(u) => u,
        Err(e) => return (StatusCode::INTERNAL_SERVER_ERROR, Json(ApiResponse::<()>::error(format!("CDP target lookup failed: {}", e)))).into_response(),
    };

    // 1. Cookies via CDP (origin-filtered).
    use crate::api::session_store::restore;
    let raw_cookies = cdp.get_all_cookies(&ws_url).await.unwrap_or_default();
    let cookies = restore::cookies_for_origin(&raw_cookies, &origin);

    // 2. Storage via evaluate on the current document.
    let mut storage = Vec::new();
    if let Ok(json_str) = cdp.evaluate(&ws_url, &restore::read_storage_script()).await {
        if let Ok(parsed) = serde_json::from_str::<serde_json::Value>(&json_str) {
            let local = json_map_to_btree(parsed.get("local"));
            let session = json_map_to_btree(parsed.get("session"));
            if !local.is_empty() || !session.is_empty() {
                storage.push(crate::api::session_store::StorageEntry { origin: origin.clone(), local, session });
            }
        }
    }

    // 3. Fingerprint from the tab's resolved stealth identity (CEF only).
    let fingerprint = tab_fingerprint(&state, &req.tab_id);

    let bundle = Bundle {
        version: crate::api::session_store::types::BUNDLE_VERSION,
        created_at: Some(chrono::Utc::now().to_rfc3339()),
        origin: origin.clone(),
        cookies,
        storage,
        fingerprint,
    };

    match store.save(&bundle).await {
        Ok(session_id) => {
            tracing::info!(
                "Exported session from tab {} for origin {} ({} cookies)",
                req.tab_id, origin, bundle.cookies.len()
            );
            Json(ApiResponse::success(SessionIdResponse { session_id, origin })).into_response()
        }
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ApiResponse::<()>::error(format!("Failed to store session: {}", e))),
        ).into_response(),
    }
}

/// Extracts `scheme://host[:port]` from a URL string (best effort).
fn origin_of(url: &str) -> String {
    if let Some(idx) = url.find("://") {
        let after = &url[idx + 3..];
        let host = after.split('/').next().unwrap_or(after);
        format!("{}{}", &url[..idx + 3], host)
    } else {
        url.to_string()
    }
}

fn json_map_to_btree(v: Option<&serde_json::Value>) -> std::collections::BTreeMap<String, String> {
    let mut out = std::collections::BTreeMap::new();
    if let Some(serde_json::Value::Object(map)) = v {
        for (k, val) in map {
            if let Some(s) = val.as_str() {
                out.insert(k.clone(), s.to_string());
            }
        }
    }
    out
}

/// Builds a [`FingerprintSpec`] from a tab's active stealth identity.
#[cfg(feature = "cef-browser")]
fn tab_fingerprint(state: &AppState, tab_id: &str) -> Option<crate::api::session_store::FingerprintSpec> {
    let engine = state.cef_engine.as_ref()?;
    let uuid = uuid::Uuid::parse_str(tab_id).ok()?;
    let stealth = engine.get_tab_stealth(&uuid)?;
    Some(crate::api::session_store::FingerprintSpec {
        user_agent: Some(stealth.fingerprint.user_agent.clone()),
        platform: Some(stealth.fingerprint.platform.clone()),
        languages: Some(stealth.fingerprint.languages.clone()),
        hardware_concurrency: Some(stealth.navigator.hardware_concurrency),
        device_memory: Some(stealth.navigator.device_memory),
        screen: Some(crate::api::session_store::ScreenSize {
            width: stealth.fingerprint.screen_resolution.width,
            height: stealth.fingerprint.screen_resolution.height,
        }),
        webgl_vendor: Some(stealth.webgl.vendor.clone()),
        webgl_renderer: Some(stealth.webgl.renderer.clone()),
        timezone: Some(stealth.fingerprint.timezone.clone()),
    })
}

#[cfg(not(feature = "cef-browser"))]
fn tab_fingerprint(_state: &AppState, _tab_id: &str) -> Option<crate::api::session_store::FingerprintSpec> {
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_origin_of() {
        assert_eq!(origin_of("https://x.test/a/b?c=1"), "https://x.test");
        assert_eq!(origin_of("http://host:8080/p"), "http://host:8080");
        assert_eq!(origin_of("about:blank"), "about:blank");
    }

    #[test]
    fn test_json_map_to_btree() {
        let v = serde_json::json!({"a":"1","b":"2","n":3});
        let m = json_map_to_btree(Some(&v));
        assert_eq!(m.get("a").map(String::as_str), Some("1"));
        assert_eq!(m.get("b").map(String::as_str), Some("2"));
        // Non-string values are skipped.
        assert!(m.get("n").is_none());
    }
}
