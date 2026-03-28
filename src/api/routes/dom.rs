//! DOM operation route handlers for element queries, annotation overlays,
//! frame tree inspection, and DOM snapshot capture.

use axum::{
    extract::{Query, State},
    http::StatusCode,
    response::IntoResponse,
    Json,
};
use tracing::error;

use crate::api::server::AppState;
use crate::api::ipc::{IpcCommand, IpcMessage};
use super::types::*;

/// GET /dom/element - Find a DOM element by CSS selector
#[utoipa::path(
    get,
    path = "/dom/element",
    tag = "dom",
    params(FindElementQuery),
    responses(
        (status = 200, description = "Element search result", body = ElementInfo),
        (status = 400, description = "No tab specified"),
        (status = 503, description = "API is disabled")
    )
)]
pub async fn find_element(
    State(state): State<AppState>,
    Query(query): Query<FindElementQuery>,
) -> impl IntoResponse {
    if !state.is_enabled().await {
        return (
            StatusCode::SERVICE_UNAVAILABLE,
            Json(ApiResponse::<ElementInfo>::error("API is disabled")),
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
                Json(ApiResponse::<ElementInfo>::error("No tab specified and no active tab")),
            ).into_response();
        }
    };

    let command = IpcCommand::FindElement {
        tab_id,
        selector: query.selector,
        timeout: query.timeout,
    };

    match state.ipc_channel.send_command(IpcMessage::Command(command)).await {
        Ok(response) => {
            if response.success {
                if let Some(data) = response.data {
                    let element = ElementInfo {
                        found: true,
                        tag_name: data.get("tagName").and_then(|v| v.as_str()).map(String::from),
                        text_content: data.get("textContent").and_then(|v| v.as_str()).map(String::from),
                        attributes: data.get("attributes").and_then(|v| v.as_object()).cloned(),
                        bounding_box: data.get("boundingBox").and_then(|v| {
                            Some(BoundingBox {
                                x: v.get("x")?.as_f64()?,
                                y: v.get("y")?.as_f64()?,
                                width: v.get("width")?.as_f64()?,
                                height: v.get("height")?.as_f64()?,
                            })
                        }),
                        is_visible: data.get("isVisible").and_then(|v| v.as_bool()),
                    };
                    return Json(ApiResponse::success(element)).into_response();
                }
                Json(ApiResponse::success(ElementInfo {
                    found: false,
                    tag_name: None,
                    text_content: None,
                    attributes: None,
                    bounding_box: None,
                    is_visible: None,
                })).into_response()
            } else {
                Json(ApiResponse::success(ElementInfo {
                    found: false,
                    tag_name: None,
                    text_content: None,
                    attributes: None,
                    bounding_box: None,
                    is_visible: None,
                })).into_response()
            }
        }
        Err(e) => {
            error!("Failed to find element: {}", e);
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ApiResponse::<ElementInfo>::error(format!("Failed to find element: {}", e))),
            ).into_response()
        }
    }
}

/// POST /dom/annotate - Annotate screenshot with element overlays
pub async fn annotate_elements(
    State(state): State<AppState>,
    Json(request): Json<AnnotateRequest>,
) -> impl IntoResponse {
    if !state.is_enabled().await {
        return (
            StatusCode::SERVICE_UNAVAILABLE,
            Json(ApiResponse::<AnnotateResponse>::error("API is disabled")),
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
                Json(ApiResponse::<AnnotateResponse>::error("No tab specified and no active tab")),
            ).into_response();
        }
    };

    let types = request.types.unwrap_or_default();
    let command = IpcCommand::AnnotateElements {
        tab_id,
        types,
        selector: request.selector,
        ocr: request.ocr.unwrap_or(false),
        ocr_lang: request.ocr_lang,
    };

    match state.ipc_channel.send_command(IpcMessage::Command(command)).await {
        Ok(response) => {
            if response.success {
                if let Some(data) = response.data {
                    let screenshot = data.get("screenshot")
                        .and_then(|v| v.as_str())
                        .unwrap_or("")
                        .to_string();
                    let elements = data.get("elements")
                        .cloned()
                        .unwrap_or(serde_json::json!([]));
                    let ocr_text = data.get("ocr_text")
                        .and_then(|v| v.as_str())
                        .map(String::from);

                    return Json(ApiResponse::success(AnnotateResponse {
                        screenshot,
                        elements,
                        ocr_text,
                    })).into_response();
                }
                (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    Json(ApiResponse::<AnnotateResponse>::error("Invalid annotation response")),
                ).into_response()
            } else {
                (
                    StatusCode::BAD_REQUEST,
                    Json(ApiResponse::<AnnotateResponse>::error(
                        response.error.unwrap_or_else(|| "Annotation failed".to_string()),
                    )),
                ).into_response()
            }
        }
        Err(e) => {
            error!("Failed to annotate: {}", e);
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ApiResponse::<AnnotateResponse>::error(format!("Failed to annotate: {}", e))),
            ).into_response()
        }
    }
}

/// GET /dom/snapshot - Capture DOM snapshot with bounding-box information for KI agent vision
pub async fn dom_snapshot(
    State(state): State<AppState>,
    Query(query): Query<DomSnapshotQuery>,
) -> impl IntoResponse {
    if !state.is_enabled().await {
        return (
            StatusCode::SERVICE_UNAVAILABLE,
            Json(ApiResponse::<serde_json::Value>::error("API is disabled")),
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
                Json(ApiResponse::<serde_json::Value>::error("No tab specified and no active tab")),
            ).into_response();
        }
    };

    let command = IpcCommand::DomSnapshot {
        tab_id,
        max_nodes: query.max_nodes,
        include_text: query.include_text,
    };

    match state.ipc_channel.send_command(IpcMessage::Command(command)).await {
        Ok(response) => {
            if response.success {
                if let Some(data) = response.data {
                    return Json(ApiResponse::success(data)).into_response();
                }
                (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    Json(ApiResponse::<serde_json::Value>::error("Invalid snapshot response")),
                ).into_response()
            } else {
                (
                    StatusCode::BAD_REQUEST,
                    Json(ApiResponse::<serde_json::Value>::error(
                        response.error.unwrap_or_else(|| "DOM snapshot failed".to_string()),
                    )),
                ).into_response()
            }
        }
        Err(e) => {
            error!("Failed to capture DOM snapshot: {}", e);
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ApiResponse::<serde_json::Value>::error(format!("Failed to capture DOM snapshot: {}", e))),
            ).into_response()
        }
    }
}

/// GET /frames - Get frame tree for a tab
///
/// Frame-IDs invalidieren nach Navigation. Dieser Endpoint muss nach jeder Navigation erneut
/// aufgerufen werden.
pub async fn get_frames(
    State(state): State<AppState>,
    Query(query): Query<FramesQuery>,
) -> impl IntoResponse {
    if !state.is_enabled().await {
        return (
            StatusCode::SERVICE_UNAVAILABLE,
            Json(ApiResponse::<serde_json::Value>::error("API is disabled")),
        ).into_response();
    }

    let command = IpcCommand::GetFrameTree {
        tab_id: query.tab_id.clone(),
    };

    match state.ipc_channel.send_command(IpcMessage::Command(command)).await {
        Ok(response) => {
            if response.success {
                if let Some(data) = response.data {
                    // The data should contain a "frames" array
                    if let Some(frames) = data.get("frames") {
                        let response_data = serde_json::json!({
                            "frames": frames
                        });
                        return Json(ApiResponse::success(response_data)).into_response();
                    }
                }
                (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    Json(ApiResponse::<serde_json::Value>::error("Invalid frame tree response")),
                ).into_response()
            } else {
                (
                    StatusCode::BAD_REQUEST,
                    Json(ApiResponse::<serde_json::Value>::error(
                        response.error.unwrap_or_else(|| "Failed to get frame tree".to_string()),
                    )),
                ).into_response()
            }
        }
        Err(e) => {
            error!("Failed to get frame tree: {}", e);
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ApiResponse::<serde_json::Value>::error(format!("Failed to get frame tree: {}", e))),
            ).into_response()
        }
    }
}
