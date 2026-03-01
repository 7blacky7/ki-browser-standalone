//! Input route handlers: click, type text, and scroll.
//!
//! Handles mouse click (coordinate and selector-based), keyboard text input,
//! and page scrolling via IPC commands to the browser core.

use axum::{extract::State, http::StatusCode, response::IntoResponse, Json};
use tracing::error;

use crate::api::ipc::{IpcCommand, IpcMessage};
use crate::api::server::AppState;

use super::types::{ApiResponse, ClickRequest, ScrollRequest, TypeRequest};

/// POST /click - Click at coordinates or on a CSS-selected element.
///
/// Dispatches either a ClickCoordinates or ClickElement IPC command depending
/// on whether (x, y) or a selector is provided. Returns 400 if neither is given.
pub async fn click(
    State(state): State<AppState>,
    Json(request): Json<ClickRequest>,
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

    let command = if let Some(selector) = request.selector {
        IpcCommand::ClickElement {
            tab_id,
            selector,
            button: request.button,
            modifiers: request.modifiers,
        }
    } else if let (Some(x), Some(y)) = (request.x, request.y) {
        IpcCommand::ClickCoordinates {
            tab_id,
            x,
            y,
            button: request.button,
            modifiers: request.modifiers,
        }
    } else {
        return (
            StatusCode::BAD_REQUEST,
            Json(ApiResponse::<()>::error(
                "Must specify either coordinates (x, y) or selector",
            )),
        )
            .into_response();
    };

    match state
        .ipc_channel
        .send_command(IpcMessage::Command(command))
        .await
    {
        Ok(response) => {
            if response.success {
                Json(ApiResponse::success(())).into_response()
            } else {
                (
                    StatusCode::BAD_REQUEST,
                    Json(ApiResponse::<()>::error(
                        response
                            .error
                            .unwrap_or_else(|| "Click failed".to_string()),
                    )),
                )
                    .into_response()
            }
        }
        Err(e) => {
            error!("Failed to click: {}", e);
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ApiResponse::<()>::error(format!("Failed to click: {}", e))),
            )
                .into_response()
        }
    }
}

/// POST /type - Type text into the active element or a CSS-selected element.
///
/// Optionally clears the target element's value before typing when clear_first is true.
pub async fn type_text(
    State(state): State<AppState>,
    Json(request): Json<TypeRequest>,
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

    let command = IpcCommand::TypeText {
        tab_id,
        text: request.text,
        selector: request.selector,
        clear_first: request.clear_first.unwrap_or(false),
    };

    match state
        .ipc_channel
        .send_command(IpcMessage::Command(command))
        .await
    {
        Ok(response) => {
            if response.success {
                Json(ApiResponse::success(())).into_response()
            } else {
                (
                    StatusCode::BAD_REQUEST,
                    Json(ApiResponse::<()>::error(
                        response
                            .error
                            .unwrap_or_else(|| "Type failed".to_string()),
                    )),
                )
                    .into_response()
            }
        }
        Err(e) => {
            error!("Failed to type: {}", e);
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ApiResponse::<()>::error(format!("Failed to type: {}", e))),
            )
                .into_response()
        }
    }
}

/// POST /scroll - Scroll the page at a coordinate, a selector, or the viewport origin.
///
/// Accepts optional delta_x/delta_y for scroll amount and behavior for animation mode.
pub async fn scroll(
    State(state): State<AppState>,
    Json(request): Json<ScrollRequest>,
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

    let command = IpcCommand::Scroll {
        tab_id,
        x: request.x,
        y: request.y,
        delta_x: request.delta_x,
        delta_y: request.delta_y,
        selector: request.selector,
        behavior: request.behavior,
    };

    match state
        .ipc_channel
        .send_command(IpcMessage::Command(command))
        .await
    {
        Ok(response) => {
            if response.success {
                Json(ApiResponse::success(())).into_response()
            } else {
                (
                    StatusCode::BAD_REQUEST,
                    Json(ApiResponse::<()>::error(
                        response
                            .error
                            .unwrap_or_else(|| "Scroll failed".to_string()),
                    )),
                )
                    .into_response()
            }
        }
        Err(e) => {
            error!("Failed to scroll: {}", e);
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ApiResponse::<()>::error(format!(
                    "Failed to scroll: {}",
                    e
                ))),
            )
                .into_response()
        }
    }
}
