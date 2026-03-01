//! Navigation route handlers: navigate, go back, go forward, and reload.
//!
//! All handlers forward browser navigation commands to the browser core via
//! the IPC channel and return a standard `ApiResponse` envelope. Each handler
//! resolves the active tab when `tab_id` is omitted from the request body.

use axum::{extract::State, http::StatusCode, response::IntoResponse, Json};
use serde::Deserialize;
use tracing::{error, info};

use crate::api::ipc::{IpcCommand, IpcMessage};
use crate::api::server::AppState;
use crate::api::routes::types::ApiResponse;

// ============================================================================
// Request Types
// ============================================================================

/// Request body for `POST /navigate` – navigate a tab to a given URL.
#[derive(Debug, Deserialize)]
pub struct NavigateRequest {
    /// Target tab ID. If omitted the active tab is used.
    #[serde(default)]
    pub tab_id: Option<String>,
    /// Absolute URL to navigate to (e.g. `https://example.com`).
    pub url: String,
}

// ============================================================================
// Helpers
// ============================================================================

/// Resolve the effective tab ID: use the provided value or fall back to the
/// currently active tab. Returns an error response tuple when no tab is available.
async fn resolve_tab_id(
    requested: Option<String>,
    state: &AppState,
) -> Result<String, (StatusCode, axum::Json<ApiResponse<()>>)> {
    if let Some(id) = requested {
        return Ok(id);
    }
    let browser_state = state.browser_state.read().await;
    browser_state.active_tab_id.clone().ok_or_else(|| {
        (
            StatusCode::BAD_REQUEST,
            Json(ApiResponse::<()>::error("No tab specified and no active tab")),
        )
    })
}

// ============================================================================
// Handlers
// ============================================================================

/// Handler for `POST /navigate` – load a URL in the specified (or active) tab.
///
/// Updates the in-memory tab URL and marks the tab as loading on success.
pub async fn navigate(
    State(state): State<AppState>,
    Json(request): Json<NavigateRequest>,
) -> impl IntoResponse {
    if !state.is_enabled().await {
        return (
            StatusCode::SERVICE_UNAVAILABLE,
            Json(ApiResponse::<()>::error("API is disabled")),
        )
            .into_response();
    }

    let tab_id = match resolve_tab_id(request.tab_id, &state).await {
        Ok(id) => id,
        Err(err) => return err.into_response(),
    };

    let command = IpcCommand::Navigate {
        tab_id: tab_id.clone(),
        url: request.url.clone(),
    };

    match state
        .ipc_channel
        .send_command(IpcMessage::Command(command))
        .await
    {
        Ok(response) => {
            if response.success {
                let mut browser_state = state.browser_state.write().await;
                if let Some(tab) = browser_state.tabs.get_mut(&tab_id) {
                    tab.url = request.url.clone();
                    tab.is_loading = true;
                }
                info!("Navigating tab {} to {}", tab_id, request.url);
                Json(ApiResponse::success(())).into_response()
            } else {
                (
                    StatusCode::BAD_REQUEST,
                    Json(ApiResponse::<()>::error(
                        response
                            .error
                            .unwrap_or_else(|| "Navigation failed".to_string()),
                    )),
                )
                    .into_response()
            }
        }
        Err(e) => {
            error!("Failed to navigate: {}", e);
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ApiResponse::<()>::error(format!(
                    "Failed to navigate: {}",
                    e
                ))),
            )
                .into_response()
        }
    }
}

/// Handler for `POST /go_back` – navigate the specified (or active) tab one step back in history.
pub async fn go_back(State(state): State<AppState>) -> impl IntoResponse {
    if !state.is_enabled().await {
        return (
            StatusCode::SERVICE_UNAVAILABLE,
            Json(ApiResponse::<()>::error("API is disabled")),
        )
            .into_response();
    }

    let tab_id = match resolve_tab_id(None, &state).await {
        Ok(id) => id,
        Err(err) => return err.into_response(),
    };

    let command = IpcCommand::GoBack {
        tab_id: tab_id.clone(),
    };

    match state
        .ipc_channel
        .send_command(IpcMessage::Command(command))
        .await
    {
        Ok(response) => {
            if response.success {
                info!("Going back in tab {}", tab_id);
                Json(ApiResponse::success(())).into_response()
            } else {
                (
                    StatusCode::BAD_REQUEST,
                    Json(ApiResponse::<()>::error(
                        response.error.unwrap_or_else(|| "Go back failed".to_string()),
                    )),
                )
                    .into_response()
            }
        }
        Err(e) => {
            error!("Failed to go back: {}", e);
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ApiResponse::<()>::error(format!("Failed to go back: {}", e))),
            )
                .into_response()
        }
    }
}

/// Handler for `POST /go_forward` – navigate the specified (or active) tab one step forward in history.
pub async fn go_forward(State(state): State<AppState>) -> impl IntoResponse {
    if !state.is_enabled().await {
        return (
            StatusCode::SERVICE_UNAVAILABLE,
            Json(ApiResponse::<()>::error("API is disabled")),
        )
            .into_response();
    }

    let tab_id = match resolve_tab_id(None, &state).await {
        Ok(id) => id,
        Err(err) => return err.into_response(),
    };

    let command = IpcCommand::GoForward {
        tab_id: tab_id.clone(),
    };

    match state
        .ipc_channel
        .send_command(IpcMessage::Command(command))
        .await
    {
        Ok(response) => {
            if response.success {
                info!("Going forward in tab {}", tab_id);
                Json(ApiResponse::success(())).into_response()
            } else {
                (
                    StatusCode::BAD_REQUEST,
                    Json(ApiResponse::<()>::error(
                        response
                            .error
                            .unwrap_or_else(|| "Go forward failed".to_string()),
                    )),
                )
                    .into_response()
            }
        }
        Err(e) => {
            error!("Failed to go forward: {}", e);
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ApiResponse::<()>::error(format!(
                    "Failed to go forward: {}",
                    e
                ))),
            )
                .into_response()
        }
    }
}

/// Handler for `POST /reload` – reload the active tab, optionally bypassing the cache.
pub async fn reload(State(state): State<AppState>) -> impl IntoResponse {
    if !state.is_enabled().await {
        return (
            StatusCode::SERVICE_UNAVAILABLE,
            Json(ApiResponse::<()>::error("API is disabled")),
        )
            .into_response();
    }

    let tab_id = match resolve_tab_id(None, &state).await {
        Ok(id) => id,
        Err(err) => return err.into_response(),
    };

    let command = IpcCommand::Reload {
        tab_id: tab_id.clone(),
        ignore_cache: false,
    };

    match state
        .ipc_channel
        .send_command(IpcMessage::Command(command))
        .await
    {
        Ok(response) => {
            if response.success {
                info!("Reloading tab {}", tab_id);
                Json(ApiResponse::success(())).into_response()
            } else {
                (
                    StatusCode::BAD_REQUEST,
                    Json(ApiResponse::<()>::error(
                        response.error.unwrap_or_else(|| "Reload failed".to_string()),
                    )),
                )
                    .into_response()
            }
        }
        Err(e) => {
            error!("Failed to reload: {}", e);
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ApiResponse::<()>::error(format!("Failed to reload: {}", e))),
            )
                .into_response()
        }
    }
}
