//! Screenshot route handler: capture a PNG or JPEG image of a browser tab.
//!
//! The handler forwards a `CaptureScreenshot` IPC command to the browser core
//! and returns the image as a base64-encoded string inside the standard
//! `ApiResponse` envelope. Supports full-page capture and element-scoped
//! capture via CSS selector.

use axum::{extract::{Query, State}, http::StatusCode, response::IntoResponse, Json};
use serde::{Deserialize, Serialize};
use tracing::error;

use crate::api::ipc::{IpcCommand, IpcMessage};
use crate::api::server::AppState;
use crate::api::routes::types::ApiResponse;

// ============================================================================
// Request / Response Types
// ============================================================================

/// Query parameters for `GET /screenshot` – configure the screenshot capture.
#[derive(Debug, Deserialize)]
pub struct ScreenshotQuery {
    /// Target tab ID. Defaults to the active tab when omitted.
    #[serde(default)]
    pub tab_id: Option<String>,
    /// Image format: `"png"` (default) or `"jpeg"`.
    #[serde(default = "default_screenshot_format")]
    pub format: String,
    /// JPEG quality in the range 0–100. Ignored for PNG.
    #[serde(default)]
    pub quality: Option<u8>,
    /// When `true` the full scrollable page is captured instead of the
    /// visible viewport. Defaults to `false`.
    #[serde(default)]
    pub full_page: Option<bool>,
    /// CSS selector to crop the screenshot to a single element's bounding box.
    #[serde(default)]
    pub selector: Option<String>,
}

fn default_screenshot_format() -> String {
    "png".to_string()
}

/// Response body for `GET /screenshot` – base64-encoded image data.
#[derive(Debug, Serialize)]
pub struct ScreenshotResponse {
    /// Base64-encoded image bytes (no data-URL prefix).
    pub data: String,
    /// Actual format of the returned image (`"png"` or `"jpeg"`).
    pub format: String,
    /// Width of the captured image in pixels.
    pub width: u32,
    /// Height of the captured image in pixels.
    pub height: u32,
}

// ============================================================================
// Helpers
// ============================================================================

/// Resolve the effective tab ID or return a 400 error response.
async fn resolve_tab_id(
    requested: Option<String>,
    state: &AppState,
) -> Result<String, (StatusCode, Json<ApiResponse<()>>)> {
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
// Handler
// ============================================================================

/// Handler for `GET /screenshot` – capture a screenshot of the specified (or active) tab.
///
/// Returns HTTP 200 with base64-encoded image data on success. Returns HTTP 500
/// when the IPC response does not include valid screenshot data or when the
/// transport fails.
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

    let tab_id = match resolve_tab_id(query.tab_id, &state).await {
        Ok(id) => id,
        Err(err) => return err.into_response(),
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
