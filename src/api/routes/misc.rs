//! Miscellaneous API route handlers: health check, API status, and API toggle.
//!
//! These endpoints are not tied to a specific browser resource (tabs, DOM,
//! navigation) but provide operational control and observability of the REST
//! server itself.

use axum::{extract::State, response::IntoResponse, Json};
use tracing::info;

use crate::api::server::AppState;
use crate::api::routes::types::{ApiResponse, ApiStatusResponse, ApiToggleRequest, HealthResponse};

/// Handler for `GET /health` – liveness probe returning server version and API state.
///
/// Always returns HTTP 200. The `api_enabled` field reflects whether the
/// browser-control API accepts commands right now.
pub async fn health_check(State(state): State<AppState>) -> impl IntoResponse {
    let api_enabled = state.is_enabled().await;

    Json(ApiResponse::success(HealthResponse {
        status: "healthy".to_string(),
        version: env!("CARGO_PKG_VERSION").to_string(),
        api_enabled,
    }))
}

/// Handler for `POST /api/toggle` – enable or disable the browser-control API at runtime.
///
/// The toggle is reflected immediately; in-flight requests are not cancelled.
pub async fn toggle_api(
    State(state): State<AppState>,
    Json(request): Json<ApiToggleRequest>,
) -> impl IntoResponse {
    state.set_enabled(request.enabled).await;

    info!(
        "API {} by request",
        if request.enabled { "enabled" } else { "disabled" }
    );

    Json(ApiResponse::success(ApiStatusResponse {
        enabled: request.enabled,
        port: 0, // Port info not available in handler context
        connected_clients: state.ws_handler.client_count().await,
    }))
}

/// Handler for `GET /api/status` – returns current API enabled state and WebSocket client count.
pub async fn api_status(State(state): State<AppState>) -> impl IntoResponse {
    let enabled = state.is_enabled().await;
    let connected_clients = state.ws_handler.client_count().await;

    Json(ApiResponse::success(ApiStatusResponse {
        enabled,
        port: 0, // Port info not available in handler context
        connected_clients,
    }))
}
