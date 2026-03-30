//! Navigation and interaction route handlers for page navigation, clicking,
//! dragging, typing, scrolling, evaluating JavaScript, and capturing screenshots.

use axum::{
    extract::{Query, State},
    http::{StatusCode, header},
    response::IntoResponse,
    Json,
};
use tracing::error;

use crate::api::server::AppState;
use crate::api::ipc::{IpcCommand, IpcMessage};
use super::types::*;

/// POST /navigate - Navigate to URL
#[utoipa::path(
    post,
    path = "/navigate",
    tag = "navigation",
    request_body = NavigateRequest,
    responses(
        (status = 200, description = "Navigation started"),
        (status = 400, description = "No tab specified or navigation failed"),
        (status = 503, description = "API is disabled")
    )
)]
pub async fn navigate(
    State(state): State<AppState>,
    Json(request): Json<NavigateRequest>,
) -> impl IntoResponse {
    if !state.is_enabled().await {
        return (
            StatusCode::SERVICE_UNAVAILABLE,
            Json(ApiResponse::<()>::error("API is disabled")),
        ).into_response();
    }

    let tab_id = match request.tab_id.or({
        let browser_state = state.browser_state.read().await;
        browser_state.active_tab_id.clone()
    }) {
        Some(id) => id,
        None => {
            return (
                StatusCode::BAD_REQUEST,
                Json(ApiResponse::<()>::error("No tab specified and no active tab")),
            ).into_response();
        }
    };

    let command = IpcCommand::Navigate {
        tab_id: tab_id.clone(),
        url: request.url.clone(),
    };

    match state.ipc_channel.send_command(IpcMessage::Command(command)).await {
        Ok(response) => {
            if response.success {
                // Update local state
                let mut browser_state = state.browser_state.write().await;
                if let Some(tab) = browser_state.tabs.get_mut(&tab_id) {
                    tab.url = request.url.clone();
                    tab.is_loading = true;
                }

                tracing::info!("Navigating tab {} to {}", tab_id, request.url);

                // Return immediately — include a hint that agent should check for CAPTCHA
                // after waiting for page load. The lightweight check in navigate was too slow
                // (2s delay made the response sluggish without guaranteeing the page was loaded).
                // Instead: agent uses /debug/captcha/detect after confirming page is loaded.
                Json(ApiResponse::success(serde_json::json!({
                    "hint": "After page loads, POST /debug/captcha/detect to check for CAPTCHAs"
                }))).into_response()
            } else {
                (
                    StatusCode::BAD_REQUEST,
                    Json(ApiResponse::<()>::error(response.error.unwrap_or_else(|| "Navigation failed".to_string()))),
                ).into_response()
            }
        }
        Err(e) => {
            error!("Failed to navigate: {}", e);
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ApiResponse::<()>::error(format!("Failed to navigate: {}", e))),
            ).into_response()
        }
    }
}

/// POST /click - Click at coordinates or on element
///
/// Unterstuetzt frame_id fuer iFrame-Isolation via CDP. Cross-Origin Frames benoetigen CDP.
#[utoipa::path(
    post,
    path = "/click",
    tag = "navigation",
    request_body = ClickRequest,
    responses(
        (status = 200, description = "Click performed"),
        (status = 400, description = "Invalid click target or no active tab"),
        (status = 503, description = "API is disabled")
    )
)]
pub async fn click(
    State(state): State<AppState>,
    Json(request): Json<ClickRequest>,
) -> impl IntoResponse {
    if !state.is_enabled().await {
        return (
            StatusCode::SERVICE_UNAVAILABLE,
            Json(ApiResponse::<()>::error("API is disabled")),
        ).into_response();
    }

    let tab_id = match request.tab_id.or({
        let browser_state = state.browser_state.read().await;
        browser_state.active_tab_id.clone()
    }) {
        Some(id) => id,
        None => {
            return (
                StatusCode::BAD_REQUEST,
                Json(ApiResponse::<()>::error("No tab specified and no active tab")),
            ).into_response();
        }
    };

    let command = if let Some(selector) = request.selector {
        IpcCommand::ClickElement {
            tab_id,
            selector,
            button: request.button,
            modifiers: request.modifiers,
            frame_id: request.frame_id,
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
            Json(ApiResponse::<()>::error("Must specify either coordinates (x, y) or selector")),
        ).into_response();
    };

    match state.ipc_channel.send_command(IpcMessage::Command(command)).await {
        Ok(response) => {
            if response.success {
                Json(ApiResponse::success(())).into_response()
            } else {
                (
                    StatusCode::BAD_REQUEST,
                    Json(ApiResponse::<()>::error(response.error.unwrap_or_else(|| "Click failed".to_string()))),
                ).into_response()
            }
        }
        Err(e) => {
            error!("Failed to click: {}", e);
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ApiResponse::<()>::error(format!("Failed to click: {}", e))),
            ).into_response()
        }
    }
}

/// POST /drag - Drag from one position to another
pub async fn drag(
    State(state): State<AppState>,
    Json(request): Json<DragRequest>,
) -> impl IntoResponse {
    if !state.is_enabled().await {
        return (
            StatusCode::SERVICE_UNAVAILABLE,
            Json(ApiResponse::<()>::error("API is disabled")),
        ).into_response();
    }

    let tab_id = match request.tab_id.or({
        let browser_state = state.browser_state.read().await;
        browser_state.active_tab_id.clone()
    }) {
        Some(id) => id,
        None => {
            return (
                StatusCode::BAD_REQUEST,
                Json(ApiResponse::<()>::error("No tab specified and no active tab")),
            ).into_response();
        }
    };

    let command = IpcCommand::Drag {
        tab_id,
        from_x: request.from_x,
        from_y: request.from_y,
        to_x: request.to_x,
        to_y: request.to_y,
        steps: request.steps,
        duration_ms: request.duration_ms,
    };

    match state.ipc_channel.send_command(IpcMessage::Command(command)).await {
        Ok(response) => {
            if response.success {
                Json(ApiResponse::success(())).into_response()
            } else {
                (
                    StatusCode::BAD_REQUEST,
                    Json(ApiResponse::<()>::error(response.error.unwrap_or_else(|| "Drag failed".to_string()))),
                ).into_response()
            }
        }
        Err(e) => {
            error!("Failed to drag: {}", e);
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ApiResponse::<()>::error(format!("Failed to drag: {}", e))),
            ).into_response()
        }
    }
}

/// POST /type - Type text into focused element or specified selector
///
/// Unterstuetzt frame_id fuer iFrame-Isolation via CDP. Cross-Origin Frames benoetigen CDP.
#[utoipa::path(
    post,
    path = "/type",
    tag = "navigation",
    request_body = TypeRequest,
    responses(
        (status = 200, description = "Text typed successfully"),
        (status = 400, description = "No tab specified or type failed"),
        (status = 503, description = "API is disabled")
    )
)]
pub async fn type_text(
    State(state): State<AppState>,
    Json(request): Json<TypeRequest>,
) -> impl IntoResponse {
    if !state.is_enabled().await {
        return (
            StatusCode::SERVICE_UNAVAILABLE,
            Json(ApiResponse::<()>::error("API is disabled")),
        ).into_response();
    }

    let tab_id = match request.tab_id.or({
        let browser_state = state.browser_state.read().await;
        browser_state.active_tab_id.clone()
    }) {
        Some(id) => id,
        None => {
            return (
                StatusCode::BAD_REQUEST,
                Json(ApiResponse::<()>::error("No tab specified and no active tab")),
            ).into_response();
        }
    };

    let command = IpcCommand::TypeText {
        tab_id,
        text: request.text,
        selector: request.selector,
        clear_first: request.clear_first.unwrap_or(false),
        frame_id: request.frame_id,
    };

    match state.ipc_channel.send_command(IpcMessage::Command(command)).await {
        Ok(response) => {
            if response.success {
                Json(ApiResponse::success(())).into_response()
            } else {
                (
                    StatusCode::BAD_REQUEST,
                    Json(ApiResponse::<()>::error(response.error.unwrap_or_else(|| "Type failed".to_string()))),
                ).into_response()
            }
        }
        Err(e) => {
            error!("Failed to type: {}", e);
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ApiResponse::<()>::error(format!("Failed to type: {}", e))),
            ).into_response()
        }
    }
}

/// POST /evaluate - Execute JavaScript in the browser context
///
/// Unterstuetzt frame_id fuer iFrame-Isolation via CDP. Cross-Origin Frames benoetigen CDP.
#[utoipa::path(
    post,
    path = "/evaluate",
    tag = "navigation",
    request_body = EvaluateRequest,
    responses(
        (status = 200, description = "Script evaluated successfully", body = EvaluateResponse),
        (status = 400, description = "No tab specified or evaluation failed"),
        (status = 503, description = "API is disabled")
    )
)]
pub async fn evaluate(
    State(state): State<AppState>,
    Json(request): Json<EvaluateRequest>,
) -> impl IntoResponse {
    if !state.is_enabled().await {
        return (
            StatusCode::SERVICE_UNAVAILABLE,
            Json(ApiResponse::<EvaluateResponse>::error("API is disabled")),
        ).into_response();
    }

    let tab_id = match request.tab_id.or({
        let browser_state = state.browser_state.read().await;
        browser_state.active_tab_id.clone()
    }) {
        Some(id) => id,
        None => {
            return (
                StatusCode::BAD_REQUEST,
                Json(ApiResponse::<EvaluateResponse>::error("No tab specified and no active tab")),
            ).into_response();
        }
    };

    let command = IpcCommand::EvaluateScript {
        tab_id,
        script: request.script,
        await_promise: request.await_promise.unwrap_or(true),
        frame_id: request.frame_id,
    };

    match state.ipc_channel.send_command(IpcMessage::Command(command)).await {
        Ok(response) => {
            if response.success {
                let result = response.data.unwrap_or(serde_json::Value::Null);
                Json(ApiResponse::success(EvaluateResponse { result })).into_response()
            } else {
                (
                    StatusCode::BAD_REQUEST,
                    Json(ApiResponse::<EvaluateResponse>::error(response.error.unwrap_or_else(|| "Evaluation failed".to_string()))),
                ).into_response()
            }
        }
        Err(e) => {
            error!("Failed to evaluate: {}", e);
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ApiResponse::<EvaluateResponse>::error(format!("Failed to evaluate: {}", e))),
            ).into_response()
        }
    }
}

/// GET /screenshot - Capture a screenshot of the current page
#[utoipa::path(
    get,
    path = "/screenshot",
    tag = "navigation",
    params(ScreenshotQuery),
    responses(
        (status = 200, description = "Screenshot captured", body = ScreenshotResponse),
        (status = 400, description = "No tab specified or screenshot failed"),
        (status = 503, description = "API is disabled")
    )
)]
pub async fn screenshot(
    State(state): State<AppState>,
    Query(query): Query<ScreenshotQuery>,
) -> impl IntoResponse {
    let raw = query.raw.unwrap_or(true);

    if !state.is_enabled().await {
        return raw_error_or_json(raw, StatusCode::SERVICE_UNAVAILABLE, "API is disabled", &query.format);
    }

    let tab_id = match query.tab_id.or({
        let browser_state = state.browser_state.read().await;
        browser_state.active_tab_id.clone()
    }) {
        Some(id) => id,
        None => {
            return raw_error_or_json(raw, StatusCode::BAD_REQUEST, "No tab specified and no active tab", &query.format);
        }
    };

    let command = IpcCommand::CaptureScreenshot {
        tab_id,
        format: query.format.clone(),
        quality: query.quality,
        full_page: query.full_page.unwrap_or(false),
        selector: query.selector,
        clip_x: query.clip_x,
        clip_y: query.clip_y,
        clip_width: query.clip_width,
        clip_height: query.clip_height,
        clip_scale: query.clip_scale,
    };

    match state.ipc_channel.send_command(IpcMessage::Command(command)).await {
        Ok(response) => {
            if response.success {
                if let Some(data) = response.data {
                    // Parse screenshot data from response
                    if let Some(screenshot) = data.get("screenshot").and_then(|v| v.as_str()) {
                        let width = data.get("width").and_then(|v| v.as_u64()).unwrap_or(0) as u32;
                        let height = data.get("height").and_then(|v| v.as_u64()).unwrap_or(0) as u32;

                        if raw {
                            // Return raw binary image
                            use base64::Engine;
                            match base64::engine::general_purpose::STANDARD.decode(screenshot) {
                                Ok(bytes) => {
                                    let content_type = match query.format.as_str() {
                                        "jpeg" | "jpg" => "image/jpeg",
                                        _ => "image/png",
                                    };
                                    return (
                                        [
                                            (header::CONTENT_TYPE, content_type.to_string()),
                                            (header::HeaderName::from_static("x-screenshot-status"), "success".to_string()),
                                        ],
                                        bytes,
                                    ).into_response();
                                }
                                Err(e) => {
                                    error!("Failed to decode base64 screenshot: {}", e);
                                    return raw_error_or_json(raw, StatusCode::INTERNAL_SERVER_ERROR, "Failed to decode screenshot data", &query.format);
                                }
                            }
                        }

                        return Json(ApiResponse::success(ScreenshotResponse {
                            data: screenshot.to_string(),
                            format: query.format,
                            width,
                            height,
                        })).into_response();
                    }
                }
                raw_error_or_json(raw, StatusCode::INTERNAL_SERVER_ERROR, "Invalid screenshot response", &query.format)
            } else {
                let msg = response.error.unwrap_or_else(|| "Screenshot failed".to_string());
                raw_error_or_json(raw, StatusCode::BAD_REQUEST, &msg, &query.format)
            }
        }
        Err(e) => {
            error!("Failed to capture screenshot: {}", e);
            raw_error_or_json(raw, StatusCode::INTERNAL_SERVER_ERROR, &format!("Failed to capture screenshot: {}", e), &query.format)
        }
    }
}

/// When raw=true and an error occurs, return a minimal 1x1 red error image
/// instead of JSON. This prevents crashes when the caller saves the response
/// as .jpg and tries to read it as an image (e.g. Claude Code Read tool).
/// The error message is passed via X-Screenshot-Error header.
fn raw_error_or_json(raw: bool, status: StatusCode, error_msg: &str, format: &str) -> axum::response::Response {
    if raw {
        // Minimal 1x1 red pixel JPEG (632 bytes, generated with PIL) — valid image that won't crash readers
        const ERROR_JPEG: &[u8] = &[
            0xFF, 0xD8, 0xFF, 0xE0, 0x00, 0x10, 0x4A, 0x46, 0x49, 0x46, 0x00, 0x01,
            0x01, 0x00, 0x00, 0x01, 0x00, 0x01, 0x00, 0x00, 0xFF, 0xDB, 0x00, 0x43,
            0x00, 0x10, 0x0B, 0x0C, 0x0E, 0x0C, 0x0A, 0x10, 0x0E, 0x0D, 0x0E, 0x12,
            0x11, 0x10, 0x13, 0x18, 0x28, 0x1A, 0x18, 0x16, 0x16, 0x18, 0x31, 0x23,
            0x25, 0x1D, 0x28, 0x3A, 0x33, 0x3D, 0x3C, 0x39, 0x33, 0x38, 0x37, 0x40,
            0x48, 0x5C, 0x4E, 0x40, 0x44, 0x57, 0x45, 0x37, 0x38, 0x50, 0x6D, 0x51,
            0x57, 0x5F, 0x62, 0x67, 0x68, 0x67, 0x3E, 0x4D, 0x71, 0x79, 0x70, 0x64,
            0x78, 0x5C, 0x65, 0x67, 0x63, 0xFF, 0xDB, 0x00, 0x43, 0x01, 0x11, 0x12,
            0x12, 0x18, 0x15, 0x18, 0x2F, 0x1A, 0x1A, 0x2F, 0x63, 0x42, 0x38, 0x42,
            0x63, 0x63, 0x63, 0x63, 0x63, 0x63, 0x63, 0x63, 0x63, 0x63, 0x63, 0x63,
            0x63, 0x63, 0x63, 0x63, 0x63, 0x63, 0x63, 0x63, 0x63, 0x63, 0x63, 0x63,
            0x63, 0x63, 0x63, 0x63, 0x63, 0x63, 0x63, 0x63, 0x63, 0x63, 0x63, 0x63,
            0x63, 0x63, 0x63, 0x63, 0x63, 0x63, 0x63, 0x63, 0x63, 0x63, 0x63, 0x63,
            0x63, 0x63, 0xFF, 0xC0, 0x00, 0x11, 0x08, 0x00, 0x01, 0x00, 0x01, 0x03,
            0x01, 0x22, 0x00, 0x02, 0x11, 0x01, 0x03, 0x11, 0x01, 0xFF, 0xC4, 0x00,
            0x1F, 0x00, 0x00, 0x01, 0x05, 0x01, 0x01, 0x01, 0x01, 0x01, 0x01, 0x00,
            0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x01, 0x02, 0x03, 0x04, 0x05,
            0x06, 0x07, 0x08, 0x09, 0x0A, 0x0B, 0xFF, 0xC4, 0x00, 0xB5, 0x10, 0x00,
            0x02, 0x01, 0x03, 0x03, 0x02, 0x04, 0x03, 0x05, 0x05, 0x04, 0x04, 0x00,
            0x00, 0x01, 0x7D, 0x01, 0x02, 0x03, 0x00, 0x04, 0x11, 0x05, 0x12, 0x21,
            0x31, 0x41, 0x06, 0x13, 0x51, 0x61, 0x07, 0x22, 0x71, 0x14, 0x32, 0x81,
            0x91, 0xA1, 0x08, 0x23, 0x42, 0xB1, 0xC1, 0x15, 0x52, 0xD1, 0xF0, 0x24,
            0x33, 0x62, 0x72, 0x82, 0x09, 0x0A, 0x16, 0x17, 0x18, 0x19, 0x1A, 0x25,
            0x26, 0x27, 0x28, 0x29, 0x2A, 0x34, 0x35, 0x36, 0x37, 0x38, 0x39, 0x3A,
            0x43, 0x44, 0x45, 0x46, 0x47, 0x48, 0x49, 0x4A, 0x53, 0x54, 0x55, 0x56,
            0x57, 0x58, 0x59, 0x5A, 0x63, 0x64, 0x65, 0x66, 0x67, 0x68, 0x69, 0x6A,
            0x73, 0x74, 0x75, 0x76, 0x77, 0x78, 0x79, 0x7A, 0x83, 0x84, 0x85, 0x86,
            0x87, 0x88, 0x89, 0x8A, 0x92, 0x93, 0x94, 0x95, 0x96, 0x97, 0x98, 0x99,
            0x9A, 0xA2, 0xA3, 0xA4, 0xA5, 0xA6, 0xA7, 0xA8, 0xA9, 0xAA, 0xB2, 0xB3,
            0xB4, 0xB5, 0xB6, 0xB7, 0xB8, 0xB9, 0xBA, 0xC2, 0xC3, 0xC4, 0xC5, 0xC6,
            0xC7, 0xC8, 0xC9, 0xCA, 0xD2, 0xD3, 0xD4, 0xD5, 0xD6, 0xD7, 0xD8, 0xD9,
            0xDA, 0xE1, 0xE2, 0xE3, 0xE4, 0xE5, 0xE6, 0xE7, 0xE8, 0xE9, 0xEA, 0xF1,
            0xF2, 0xF3, 0xF4, 0xF5, 0xF6, 0xF7, 0xF8, 0xF9, 0xFA, 0xFF, 0xC4, 0x00,
            0x1F, 0x01, 0x00, 0x03, 0x01, 0x01, 0x01, 0x01, 0x01, 0x01, 0x01, 0x01,
            0x01, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x01, 0x02, 0x03, 0x04, 0x05,
            0x06, 0x07, 0x08, 0x09, 0x0A, 0x0B, 0xFF, 0xC4, 0x00, 0xB5, 0x11, 0x00,
            0x02, 0x01, 0x02, 0x04, 0x04, 0x03, 0x04, 0x07, 0x05, 0x04, 0x04, 0x00,
            0x01, 0x02, 0x77, 0x00, 0x01, 0x02, 0x03, 0x11, 0x04, 0x05, 0x21, 0x31,
            0x06, 0x12, 0x41, 0x51, 0x07, 0x61, 0x71, 0x13, 0x22, 0x32, 0x81, 0x08,
            0x14, 0x42, 0x91, 0xA1, 0xB1, 0xC1, 0x09, 0x23, 0x33, 0x52, 0xF0, 0x15,
            0x62, 0x72, 0xD1, 0x0A, 0x16, 0x24, 0x34, 0xE1, 0x25, 0xF1, 0x17, 0x18,
            0x19, 0x1A, 0x26, 0x27, 0x28, 0x29, 0x2A, 0x35, 0x36, 0x37, 0x38, 0x39,
            0x3A, 0x43, 0x44, 0x45, 0x46, 0x47, 0x48, 0x49, 0x4A, 0x53, 0x54, 0x55,
            0x56, 0x57, 0x58, 0x59, 0x5A, 0x63, 0x64, 0x65, 0x66, 0x67, 0x68, 0x69,
            0x6A, 0x73, 0x74, 0x75, 0x76, 0x77, 0x78, 0x79, 0x7A, 0x82, 0x83, 0x84,
            0x85, 0x86, 0x87, 0x88, 0x89, 0x8A, 0x92, 0x93, 0x94, 0x95, 0x96, 0x97,
            0x98, 0x99, 0x9A, 0xA2, 0xA3, 0xA4, 0xA5, 0xA6, 0xA7, 0xA8, 0xA9, 0xAA,
            0xB2, 0xB3, 0xB4, 0xB5, 0xB6, 0xB7, 0xB8, 0xB9, 0xBA, 0xC2, 0xC3, 0xC4,
            0xC5, 0xC6, 0xC7, 0xC8, 0xC9, 0xCA, 0xD2, 0xD3, 0xD4, 0xD5, 0xD6, 0xD7,
            0xD8, 0xD9, 0xDA, 0xE2, 0xE3, 0xE4, 0xE5, 0xE6, 0xE7, 0xE8, 0xE9, 0xEA,
            0xF2, 0xF3, 0xF4, 0xF5, 0xF6, 0xF7, 0xF8, 0xF9, 0xFA, 0xFF, 0xDA, 0x00,
            0x0C, 0x03, 0x01, 0x00, 0x02, 0x11, 0x03, 0x11, 0x00, 0x3F, 0x00, 0xC5,
            0xA2, 0x8A, 0x2B, 0xCB, 0x3E, 0xF0, 0xFF, 0xD9,
        ];
        // Minimal 1x1 red pixel PNG (69 bytes, generated with PIL)
        const ERROR_PNG: &[u8] = &[
            0x89, 0x50, 0x4E, 0x47, 0x0D, 0x0A, 0x1A, 0x0A, 0x00, 0x00, 0x00, 0x0D,
            0x49, 0x48, 0x44, 0x52, 0x00, 0x00, 0x00, 0x01, 0x00, 0x00, 0x00, 0x01,
            0x08, 0x02, 0x00, 0x00, 0x00, 0x90, 0x77, 0x53, 0xDE, 0x00, 0x00, 0x00,
            0x0C, 0x49, 0x44, 0x41, 0x54, 0x78, 0x9C, 0x63, 0xF8, 0xCF, 0xC0, 0x00,
            0x00, 0x03, 0x01, 0x01, 0x00, 0xC9, 0xFE, 0x92, 0xEF, 0x00, 0x00, 0x00,
            0x00, 0x49, 0x45, 0x4E, 0x44, 0xAE, 0x42, 0x60, 0x82,
        ];

        let (bytes, content_type): (&[u8], &str) = match format {
            "jpeg" | "jpg" => (ERROR_JPEG, "image/jpeg"),
            _ => (ERROR_PNG, "image/png"),
        };

        // Truncate error for header safety (no newlines, max 200 chars)
        let safe_error = error_msg.replace('\n', " ");
        let safe_error = if safe_error.len() > 200 { &safe_error[..200] } else { &safe_error };

        return (
            status,
            [
                (header::CONTENT_TYPE, content_type.to_string()),
                (header::HeaderName::from_static("x-screenshot-status"), "error".to_string()),
                (header::HeaderName::from_static("x-screenshot-error"), safe_error.to_string()),
            ],
            bytes.to_vec(),
        ).into_response();
    }

    // Non-raw: return JSON error as before
    (
        status,
        Json(ApiResponse::<ScreenshotResponse>::error(error_msg)),
    ).into_response()
}

/// POST /scroll - Scroll the page by coordinates or to a selector
///
/// Unterstuetzt frame_id fuer iFrame-Isolation via CDP. Cross-Origin Frames benoetigen CDP.
#[utoipa::path(
    post,
    path = "/scroll",
    tag = "navigation",
    request_body = ScrollRequest,
    responses(
        (status = 200, description = "Scroll performed"),
        (status = 400, description = "No tab specified or scroll failed"),
        (status = 503, description = "API is disabled")
    )
)]
pub async fn scroll(
    State(state): State<AppState>,
    Json(request): Json<ScrollRequest>,
) -> impl IntoResponse {
    if !state.is_enabled().await {
        return (
            StatusCode::SERVICE_UNAVAILABLE,
            Json(ApiResponse::<()>::error("API is disabled")),
        ).into_response();
    }

    let tab_id = match request.tab_id.or({
        let browser_state = state.browser_state.read().await;
        browser_state.active_tab_id.clone()
    }) {
        Some(id) => id,
        None => {
            return (
                StatusCode::BAD_REQUEST,
                Json(ApiResponse::<()>::error("No tab specified and no active tab")),
            ).into_response();
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
        frame_id: request.frame_id,
    };

    match state.ipc_channel.send_command(IpcMessage::Command(command)).await {
        Ok(response) => {
            if response.success {
                Json(ApiResponse::success(())).into_response()
            } else {
                (
                    StatusCode::BAD_REQUEST,
                    Json(ApiResponse::<()>::error(response.error.unwrap_or_else(|| "Scroll failed".to_string()))),
                ).into_response()
            }
        }
        Err(e) => {
            error!("Failed to scroll: {}", e);
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ApiResponse::<()>::error(format!("Failed to scroll: {}", e))),
            ).into_response()
        }
    }
}
