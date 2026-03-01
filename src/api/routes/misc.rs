//! Miscellaneous route handlers: health check, API toggle/status, and CDP
//! remote debugging info endpoints.

use axum::{
    extract::{Path, State},
    http::StatusCode,
    response::IntoResponse,
    Json,
};
use tracing::{info, warn};
use uuid::Uuid;

use crate::api::cdp_mapping::{CdpTargetInfo, CdpTargetLookupResponse, CdpTargetsResponse};
use crate::api::server::AppState;
use super::types::*;

/// GET /health - Health check endpoint
#[utoipa::path(
    get,
    path = "/health",
    tag = "health",
    responses(
        (status = 200, description = "Server is healthy", body = HealthResponse)
    )
)]
pub async fn health_check(State(state): State<AppState>) -> impl IntoResponse {
    let api_enabled = state.is_enabled().await;

    Json(ApiResponse::success(HealthResponse {
        status: "healthy".to_string(),
        version: env!("CARGO_PKG_VERSION").to_string(),
        api_enabled,
    }))
}

/// POST /api/toggle - Toggle API enabled state
#[utoipa::path(
    post,
    path = "/api/toggle",
    tag = "api",
    request_body = ApiToggleRequest,
    responses(
        (status = 200, description = "API state toggled", body = ApiStatusResponse)
    )
)]
pub async fn toggle_api(
    State(state): State<AppState>,
    Json(request): Json<ApiToggleRequest>,
) -> impl IntoResponse {
    state.set_enabled(request.enabled).await;

    info!("API {} by request", if request.enabled { "enabled" } else { "disabled" });

    Json(ApiResponse::success(ApiStatusResponse {
        enabled: request.enabled,
        port: 0, // Port info not available here
        connected_clients: state.ws_handler.client_count().await,
    }))
}

/// GET /api/status - Get current API status
#[utoipa::path(
    get,
    path = "/api/status",
    tag = "api",
    responses(
        (status = 200, description = "Current API status", body = ApiStatusResponse)
    )
)]
pub async fn api_status(State(state): State<AppState>) -> impl IntoResponse {
    let enabled = state.is_enabled().await;
    let connected_clients = state.ws_handler.client_count().await;

    Json(ApiResponse::success(ApiStatusResponse {
        enabled,
        port: 0, // Port info not available here
        connected_clients,
    }))
}

/// GET /cdp - Returns CDP remote debugging connection info for Playwright/DevTools integration
pub(crate) async fn cdp_info(
    State(state): State<AppState>,
) -> impl IntoResponse {
    let cdp_port = state.cdp_port.unwrap_or(9222);
    let base = format!("http://127.0.0.1:{}", cdp_port);
    Json(serde_json::json!({
        "success": true,
        "data": {
            "base_url": base,
            "json_list": format!("{}/json/list", base),
            "json_version": format!("{}/json/version", base),
            "ws_base": format!("ws://127.0.0.1:{}", cdp_port),
            "port": cdp_port
        }
    }))
}

/// GET /cdp/targets - List all CDP targets with their mapped ki-browser tab UUIDs.
///
/// Returns remote debugging connection info and all known tab-to-target mappings,
/// enabling external CDP clients to discover which WebSocket URL corresponds to
/// which ki-browser tab.
pub async fn cdp_targets(State(state): State<AppState>) -> impl IntoResponse {
    if !state.is_enabled().await {
        return (
            StatusCode::SERVICE_UNAVAILABLE,
            Json(ApiResponse::<CdpTargetsResponse>::error("API is disabled")),
        )
            .into_response();
    }

    let mapping = &state.cdp_mapping;
    let browser_state = state.browser_state.read().await;

    let targets: Vec<CdpTargetInfo> = mapping
        .all_mappings()
        .into_iter()
        .map(|(tab_uuid, target_id)| {
            let tab_id_str = tab_uuid.to_string();
            let (url, title) = browser_state
                .tabs
                .get(&tab_id_str)
                .map(|t| (t.url.clone(), t.title.clone()))
                .unwrap_or_else(|| ("unknown".to_string(), "Unknown".to_string()));

            CdpTargetInfo {
                tab_id: tab_id_str,
                target_id: target_id.clone(),
                target_type: "page".to_string(),
                ws_url: mapping.target_ws_url(&target_id),
                url,
                title,
            }
        })
        .collect();

    Json(ApiResponse::success(CdpTargetsResponse {
        remote_debugging_port: mapping.remote_debugging_port(),
        browser_ws_url: mapping.browser_ws_url(),
        targets,
    }))
    .into_response()
}

/// GET /cdp/target/:tab_id - Look up the CDP TargetId for a specific ki-browser tab UUID.
///
/// Returns the CDP target identifier and WebSocket URL for connecting to the
/// specified tab via Chrome DevTools Protocol.
pub async fn cdp_target_by_tab(
    State(state): State<AppState>,
    Path(tab_id): Path<String>,
) -> impl IntoResponse {
    if !state.is_enabled().await {
        return (
            StatusCode::SERVICE_UNAVAILABLE,
            Json(ApiResponse::<CdpTargetLookupResponse>::error("API is disabled")),
        )
            .into_response();
    }

    let uuid = match Uuid::parse_str(&tab_id) {
        Ok(u) => u,
        Err(_) => {
            return (
                StatusCode::BAD_REQUEST,
                Json(ApiResponse::<CdpTargetLookupResponse>::error(
                    "Invalid tab UUID format",
                )),
            )
                .into_response();
        }
    };

    let mapping = &state.cdp_mapping;

    match mapping.get_target_id(&uuid) {
        Some(target_id) => {
            let ws_url = mapping.target_ws_url(&target_id);
            Json(ApiResponse::success(CdpTargetLookupResponse {
                tab_id: tab_id.clone(),
                target_id,
                ws_url,
            }))
            .into_response()
        }
        None => {
            warn!("CDP target lookup failed: no mapping for tab {}", tab_id);
            (
                StatusCode::NOT_FOUND,
                Json(ApiResponse::<CdpTargetLookupResponse>::error(format!(
                    "No CDP target mapping found for tab: {}",
                    tab_id
                ))),
            )
                .into_response()
        }
    }
}
