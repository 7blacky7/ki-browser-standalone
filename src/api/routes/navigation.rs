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
    if !state.is_enabled().await {
        return (
            StatusCode::SERVICE_UNAVAILABLE,
            Json(ApiResponse::<ScreenshotResponse>::error("API is disabled")),
        ).into_response();
    }

    let tab_id = match query.tab_id.or({
        let browser_state = state.browser_state.read().await;
        browser_state.active_tab_id.clone()
    }) {
        Some(id) => id,
        None => {
            return (
                StatusCode::BAD_REQUEST,
                Json(ApiResponse::<ScreenshotResponse>::error("No tab specified and no active tab")),
            ).into_response();
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

    let raw = query.raw.unwrap_or(false);

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
                                        [(header::CONTENT_TYPE, content_type)],
                                        bytes,
                                    ).into_response();
                                }
                                Err(e) => {
                                    error!("Failed to decode base64 screenshot: {}", e);
                                    return (
                                        StatusCode::INTERNAL_SERVER_ERROR,
                                        Json(ApiResponse::<ScreenshotResponse>::error("Failed to decode screenshot data")),
                                    ).into_response();
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
                (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    Json(ApiResponse::<ScreenshotResponse>::error("Invalid screenshot response")),
                ).into_response()
            } else {
                (
                    StatusCode::BAD_REQUEST,
                    Json(ApiResponse::<ScreenshotResponse>::error(response.error.unwrap_or_else(|| "Screenshot failed".to_string()))),
                ).into_response()
            }
        }
        Err(e) => {
            error!("Failed to capture screenshot: {}", e);
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ApiResponse::<ScreenshotResponse>::error(format!("Failed to capture screenshot: {}", e))),
            ).into_response()
        }
    }
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
