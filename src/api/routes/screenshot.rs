//! Screenshot route handler: capture browser tab as base64-encoded image.
//!
//! Handles GET /screenshot by forwarding a CaptureScreenshot IPC command to
//! the browser core and returning the base64-encoded PNG or JPEG result.

use axum::{
    extract::{Query, State},
    http::StatusCode,
    response::IntoResponse,
    Json,
};
use tracing::error;

use crate::api::ipc::{IpcCommand, IpcMessage};
use crate::api::server::AppState;

use super::types::{ApiResponse, ScreenshotQuery, ScreenshotResponse};

/// GET /screenshot - Capture a screenshot of the active or specified tab.
///
/// Sends a CaptureScreenshot IPC command with format, quality, full_page, and
/// optional selector parameters. Returns base64-encoded image data with dimensions.
pub async fn screenshot(
    State(state): State<AppState>,
    Query(query): Query<ScreenshotQuery>,
) -> impl IntoResponse {
    if !state.is_enabled().await {
        return (
            StatusCode::SERVICE_UNAVAILABLE,
            Json(ApiResponse::<ScreenshotResponse>::error("API is disabled")),
        )
            .into_response();
    }

    let tab_id = match query.tab_id.or_else(|| {
        let browser_state = futures::executor::block_on(state.browser_state.read());
        browser_state.active_tab_id.clone()
    }) {
        Some(id) => id,
        None => {
            return (
                StatusCode::BAD_REQUEST,
                Json(ApiResponse::<ScreenshotResponse>::error(
                    "No tab specified and no active tab",
                )),
            )
                .into_response();
        }
    };

    let command = IpcCommand::CaptureScreenshot {
        tab_id,
        format: query.format.clone(),
        quality: query.quality,
        full_page: query.full_page.unwrap_or(false),
        selector: query.selector,
    };

    match state
        .ipc_channel
        .send_command(IpcMessage::Command(command))
        .await
    {
        Ok(response) => {
            if response.success {
                if let Some(data) = response.data {
                    if let Some(screenshot) =
                        data.get("screenshot").and_then(|v| v.as_str())
                    {
                        let width = data
                            .get("width")
                            .and_then(|v| v.as_u64())
                            .unwrap_or(0) as u32;
                        let height = data
                            .get("height")
                            .and_then(|v| v.as_u64())
                            .unwrap_or(0) as u32;

                        return Json(ApiResponse::success(ScreenshotResponse {
                            data: screenshot.to_string(),
                            format: query.format,
                            width,
                            height,
                        }))
                        .into_response();
                    }
                }
                (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    Json(ApiResponse::<ScreenshotResponse>::error(
                        "Invalid screenshot response",
                    )),
                )
                    .into_response()
            } else {
                (
                    StatusCode::BAD_REQUEST,
                    Json(ApiResponse::<ScreenshotResponse>::error(
                        response
                            .error
                            .unwrap_or_else(|| "Screenshot failed".to_string()),
                    )),
                )
                    .into_response()
            }
        }
        Err(e) => {
            error!("Failed to capture screenshot: {}", e);
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ApiResponse::<ScreenshotResponse>::error(format!(
                    "Failed to capture screenshot: {}",
                    e
                ))),
            )
                .into_response()
        }
    }
}
