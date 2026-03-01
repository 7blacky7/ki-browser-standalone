//! Navigation route handlers: navigate, go back, go forward, reload, evaluate JS.
//!
//! Handles URL navigation and history traversal commands by forwarding IPC
//! commands to the browser core and reflecting state changes locally.

use axum::{extract::State, http::StatusCode, response::IntoResponse, Json};
use tracing::{error, info};

use crate::api::ipc::{IpcCommand, IpcMessage};
use crate::api::server::AppState;

use super::types::{ApiResponse, EvaluateRequest, EvaluateResponse, NavigateRequest};

/// POST /navigate - Navigate the active or specified tab to a URL.
///
/// Falls back to the active tab when no tab_id is provided. Updates local
/// tab URL and loading state on success.
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

    let tab_id = match request.tab_id.or_else(|| {
        let browser_state = futures::executor::block_on(state.browser_state.read());
        browser_state.active_tab_id.clone()
    }) {
        Some(id) => id,
        None => {
            return (
                StatusCode::BAD_REQUEST,
                Json(ApiResponse::<()>::error(
                    "No tab specified and no active tab",
                )),
            )
                .into_response();
        }
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

/// POST /evaluate - Execute arbitrary JavaScript inside a tab context.
///
/// Sends an EvaluateScript IPC command and returns the serialised result value.
/// Optionally awaits a returned Promise before resolving.
pub async fn evaluate(
    State(state): State<AppState>,
    Json(request): Json<EvaluateRequest>,
) -> impl IntoResponse {
    if !state.is_enabled().await {
        return (
            StatusCode::SERVICE_UNAVAILABLE,
            Json(ApiResponse::<EvaluateResponse>::error("API is disabled")),
        )
            .into_response();
    }

    let tab_id = match request.tab_id.or_else(|| {
        let browser_state = futures::executor::block_on(state.browser_state.read());
        browser_state.active_tab_id.clone()
    }) {
        Some(id) => id,
        None => {
            return (
                StatusCode::BAD_REQUEST,
                Json(ApiResponse::<EvaluateResponse>::error(
                    "No tab specified and no active tab",
                )),
            )
                .into_response();
        }
    };

    let command = IpcCommand::EvaluateScript {
        tab_id,
        script: request.script,
        await_promise: request.await_promise.unwrap_or(true),
    };

    match state
        .ipc_channel
        .send_command(IpcMessage::Command(command))
        .await
    {
        Ok(response) => {
            if response.success {
                let result = response.data.unwrap_or(serde_json::Value::Null);
                Json(ApiResponse::success(EvaluateResponse { result })).into_response()
            } else {
                (
                    StatusCode::BAD_REQUEST,
                    Json(ApiResponse::<EvaluateResponse>::error(
                        response
                            .error
                            .unwrap_or_else(|| "Evaluation failed".to_string()),
                    )),
                )
                    .into_response()
            }
        }
        Err(e) => {
            error!("Failed to evaluate: {}", e);
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ApiResponse::<EvaluateResponse>::error(format!(
                    "Failed to evaluate: {}",
                    e
                ))),
            )
                .into_response()
        }
    }
}
