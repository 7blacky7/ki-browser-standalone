//! Miscellaneous API route handlers: health check, API toggle, and status.
//!
//! Handles the /health, /api/toggle, and /api/status endpoints used for
//! liveness probing and runtime API enable/disable control.

use axum::{extract::State, response::IntoResponse, Json};
use tracing::info;

use crate::api::server::AppState;

use super::types::{ApiResponse, ApiStatusResponse, ApiToggleRequest, HealthResponse};

/// GET /health - Health check endpoint for liveness probing.
///
/// Returns server version and current API enabled state without auth requirements.
pub async fn health_check(State(state): State<AppState>) -> impl IntoResponse {
    let api_enabled = state.is_enabled().await;

    Json(ApiResponse::success(HealthResponse {
        status: "healthy".to_string(),
        version: env!("CARGO_PKG_VERSION").to_string(),
        api_enabled,
    }))
}

/// POST /api/toggle - Toggle API enabled state at runtime.
///
/// Enables or disables the REST API without restarting the server.
/// Always succeeds and reflects the new state in the response.
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
        port: 0, // Port info not available here
        connected_clients: state.ws_handler.client_count().await,
    }))
}

/// GET /api/status - Get current API status including WebSocket client count.
pub async fn api_status(State(state): State<AppState>) -> impl IntoResponse {
    let enabled = state.is_enabled().await;
    let connected_clients = state.ws_handler.client_count().await;

    Json(ApiResponse::success(ApiStatusResponse {
        enabled,
        port: 0, // Port info not available here
        connected_clients,
    }))
}
