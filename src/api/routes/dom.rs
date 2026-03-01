//! DOM route handler: find elements by CSS selector and return layout/content info.
//!
//! Handles GET /dom/element by forwarding a FindElement IPC command to the
//! browser core and mapping the JSON response to ElementInfo with bounding box.

use axum::{
    extract::{Query, State},
    http::StatusCode,
    response::IntoResponse,
    Json,
};
use tracing::error;

use crate::api::ipc::{IpcCommand, IpcMessage};
use crate::api::server::AppState;

use super::types::{ApiResponse, BoundingBox, ElementInfo, FindElementQuery};

/// GET /dom/element - Find a DOM element by CSS selector and return its properties.
///
/// Sends a FindElement IPC command with an optional timeout. Returns tag name,
/// text content, attributes, bounding box, and visibility state when found.
/// Returns `found: false` when the element does not exist without an error status.
pub async fn find_element(
    State(state): State<AppState>,
    Query(query): Query<FindElementQuery>,
) -> impl IntoResponse {
    if !state.is_enabled().await {
        return (
            StatusCode::SERVICE_UNAVAILABLE,
            Json(ApiResponse::<ElementInfo>::error("API is disabled")),
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
                Json(ApiResponse::<ElementInfo>::error(
                    "No tab specified and no active tab",
                )),
            )
                .into_response();
        }
    };

    let command = IpcCommand::FindElement {
        tab_id,
        selector: query.selector,
        timeout: query.timeout,
    };

    match state
        .ipc_channel
        .send_command(IpcMessage::Command(command))
        .await
    {
        Ok(response) => {
            if response.success {
                if let Some(data) = response.data {
                    let element = ElementInfo {
                        found: true,
                        tag_name: data
                            .get("tagName")
                            .and_then(|v| v.as_str())
                            .map(String::from),
                        text_content: data
                            .get("textContent")
                            .and_then(|v| v.as_str())
                            .map(String::from),
                        attributes: data
                            .get("attributes")
                            .and_then(|v| v.as_object())
                            .cloned(),
                        bounding_box: data.get("boundingBox").and_then(|v| {
                            Some(BoundingBox {
                                x: v.get("x")?.as_f64()?,
                                y: v.get("y")?.as_f64()?,
                                width: v.get("width")?.as_f64()?,
                                height: v.get("height")?.as_f64()?,
                            })
                        }),
                        is_visible: data
                            .get("isVisible")
                            .and_then(|v| v.as_bool()),
                    };
                    return Json(ApiResponse::success(element)).into_response();
                }
                // Element not found — return success with found: false
                Json(ApiResponse::success(ElementInfo {
                    found: false,
                    tag_name: None,
                    text_content: None,
                    attributes: None,
                    bounding_box: None,
                    is_visible: None,
                }))
                .into_response()
            } else {
                // IPC reported failure — still return found: false rather than an error status
                Json(ApiResponse::success(ElementInfo {
                    found: false,
                    tag_name: None,
                    text_content: None,
                    attributes: None,
                    bounding_box: None,
                    is_visible: None,
                }))
                .into_response()
            }
        }
        Err(e) => {
            error!("Failed to find element: {}", e);
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ApiResponse::<ElementInfo>::error(format!(
                    "Failed to find element: {}",
                    e
                ))),
            )
                .into_response()
        }
    }
}
