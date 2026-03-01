//! DOM interaction route handlers: find element and evaluate JavaScript.
//!
//! Handlers query the live DOM of the active browser tab via IPC commands.
//! `find_element` resolves a CSS selector to element metadata including tag,
//! text content, attributes, bounding box, and visibility. `evaluate` executes
//! arbitrary JavaScript in the tab's page context and returns the serialised
//! result.

use axum::{extract::{Query, State}, http::StatusCode, response::IntoResponse, Json};
use serde::{Deserialize, Serialize};
use tracing::error;

use crate::api::ipc::{IpcCommand, IpcMessage};
use crate::api::server::AppState;
use crate::api::routes::types::ApiResponse;

// ============================================================================
// Request / Response Types
// ============================================================================

/// Query parameters for `GET /dom/element` – CSS selector-based element lookup.
#[derive(Debug, Deserialize)]
pub struct FindElementQuery {
    /// Target tab ID. Defaults to the active tab when omitted.
    #[serde(default)]
    pub tab_id: Option<String>,
    /// CSS selector string used to locate the DOM element.
    pub selector: String,
    /// Optional timeout in milliseconds to wait for the element to appear.
    #[serde(default)]
    pub timeout: Option<u64>,
}

/// Axis-aligned bounding box of a DOM element in CSS pixels.
#[derive(Debug, Serialize)]
pub struct BoundingBox {
    pub x: f64,
    pub y: f64,
    pub width: f64,
    pub height: f64,
}

/// Metadata about a DOM element returned by `GET /dom/element`.
///
/// When `found` is `false` all other fields are `None`.
#[derive(Debug, Serialize)]
pub struct ElementInfo {
    /// Whether the selector matched an element in the current document.
    pub found: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tag_name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub text_content: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub attributes: Option<serde_json::Map<String, serde_json::Value>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub bounding_box: Option<BoundingBox>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub is_visible: Option<bool>,
}

impl ElementInfo {
    /// Convenience constructor for the "not found" case.
    fn not_found() -> Self {
        Self {
            found: false,
            tag_name: None,
            text_content: None,
            attributes: None,
            bounding_box: None,
            is_visible: None,
        }
    }
}

/// Request body for `POST /evaluate` – JavaScript evaluation in a tab's page context.
#[derive(Debug, Deserialize)]
pub struct EvaluateRequest {
    /// Target tab ID. Defaults to the active tab when omitted.
    #[serde(default)]
    pub tab_id: Option<String>,
    /// JavaScript source to execute. May be a statement or an expression.
    pub script: String,
    /// When `true` (default), the handler awaits a returned Promise before
    /// serialising the result.
    #[serde(default)]
    pub await_promise: Option<bool>,
}

/// Response body for `POST /evaluate` – the serialised JavaScript return value.
#[derive(Debug, Serialize)]
pub struct EvaluateResponse {
    /// JSON-serialised return value of the evaluated script. `null` when the
    /// script returns `undefined` or when `await_promise` resolves to nothing.
    pub result: serde_json::Value,
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
// Handlers
// ============================================================================

/// Handler for `GET /dom/element` – locate a DOM element by CSS selector.
///
/// Returns element metadata when found. Returns `{ found: false }` (HTTP 200)
/// when the selector matches nothing; only returns an error status on IPC
/// transport failures.
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

    let tab_id = match resolve_tab_id(query.tab_id, &state).await {
        Ok(id) => id,
        Err(err) => return err.into_response(),
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
                        is_visible: data.get("isVisible").and_then(|v| v.as_bool()),
                    };
                    return Json(ApiResponse::success(element)).into_response();
                }
                Json(ApiResponse::success(ElementInfo::not_found())).into_response()
            } else {
                Json(ApiResponse::success(ElementInfo::not_found())).into_response()
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

/// Handler for `POST /evaluate` – execute JavaScript in the specified (or active) tab.
///
/// The script runs in the tab's main frame. When `await_promise` is `true`
/// (default) a returned Promise is awaited before the result is serialised.
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

    let tab_id = match resolve_tab_id(request.tab_id, &state).await {
        Ok(id) => id,
        Err(err) => return err.into_response(),
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
