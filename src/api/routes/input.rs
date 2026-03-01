//! Input route handlers: click, type text, and scroll.
//!
//! Handlers translate REST requests into IPC commands that are forwarded to
//! the browser core for synthetic input injection. Click supports both
//! absolute CSS-pixel coordinates and CSS selector targeting. Type supports
//! optional selector focus and clearing the field before typing. Scroll
//! supports coordinates, selector, and delta offsets.

use axum::{extract::State, http::StatusCode, response::IntoResponse, Json};
use serde::Deserialize;
use tracing::error;

use crate::api::ipc::{IpcCommand, IpcMessage};
use crate::api::server::AppState;
use crate::api::routes::types::ApiResponse;

// ============================================================================
// Request Types
// ============================================================================

/// Request body for `POST /click` – synthetic mouse click via coordinates or CSS selector.
///
/// Exactly one of (`x`, `y`) or `selector` must be provided; otherwise the
/// handler returns HTTP 400.
#[derive(Debug, Deserialize)]
pub struct ClickRequest {
    /// Target tab ID. Defaults to the active tab when omitted.
    #[serde(default)]
    pub tab_id: Option<String>,
    /// Horizontal coordinate in CSS pixels (used when `selector` is absent).
    #[serde(default)]
    pub x: Option<i32>,
    /// Vertical coordinate in CSS pixels (used when `selector` is absent).
    #[serde(default)]
    pub y: Option<i32>,
    /// CSS selector of the element to click (alternative to coordinates).
    #[serde(default)]
    pub selector: Option<String>,
    /// Mouse button to press: `"left"` (default), `"middle"`, or `"right"`.
    #[serde(default = "default_click_button")]
    pub button: String,
    /// Keyboard modifiers held during the click (e.g. `["shift", "ctrl"]`).
    #[serde(default)]
    pub modifiers: Option<Vec<String>>,
}

fn default_click_button() -> String {
    "left".to_string()
}

/// Request body for `POST /type` – inject keyboard input into a tab.
#[derive(Debug, Deserialize)]
pub struct TypeRequest {
    /// Target tab ID. Defaults to the active tab when omitted.
    #[serde(default)]
    pub tab_id: Option<String>,
    /// Text to type (Unicode, including special characters).
    pub text: String,
    /// CSS selector of the element to focus before typing. When omitted the
    /// currently focused element receives the input.
    #[serde(default)]
    pub selector: Option<String>,
    /// When `true` the element's current value is cleared before the new text
    /// is typed. Defaults to `false`.
    #[serde(default)]
    pub clear_first: Option<bool>,
}

/// Request body for `POST /scroll` – scroll the page or a specific element.
#[derive(Debug, Deserialize)]
pub struct ScrollRequest {
    /// Target tab ID. Defaults to the active tab when omitted.
    #[serde(default)]
    pub tab_id: Option<String>,
    /// Horizontal origin of the scroll gesture in CSS pixels.
    #[serde(default)]
    pub x: Option<i32>,
    /// Vertical origin of the scroll gesture in CSS pixels.
    #[serde(default)]
    pub y: Option<i32>,
    /// Horizontal scroll delta in CSS pixels (positive = right).
    #[serde(default)]
    pub delta_x: Option<i32>,
    /// Vertical scroll delta in CSS pixels (positive = down).
    #[serde(default)]
    pub delta_y: Option<i32>,
    /// CSS selector of the element to scroll. When omitted the page viewport
    /// is scrolled.
    #[serde(default)]
    pub selector: Option<String>,
    /// Scroll behaviour hint: `"auto"`, `"smooth"`, or `"instant"`.
    #[serde(default)]
    pub behavior: Option<String>,
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

/// Handler for `POST /click` – inject a synthetic mouse click into the browser tab.
///
/// Dispatches `ClickElement` when a selector is provided, or `ClickCoordinates`
/// when (x, y) are provided. Returns HTTP 400 when neither is present.
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

    let tab_id = match resolve_tab_id(request.tab_id, &state).await {
        Ok(id) => id,
        Err(err) => return err.into_response(),
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
                        response.error.unwrap_or_else(|| "Click failed".to_string()),
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

/// Handler for `POST /type` – inject keyboard text into the specified (or active) tab.
///
/// When `selector` is set the matching element is focused before typing.
/// When `clear_first` is `true` the element's value is cleared first.
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

    let tab_id = match resolve_tab_id(request.tab_id, &state).await {
        Ok(id) => id,
        Err(err) => return err.into_response(),
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
                        response.error.unwrap_or_else(|| "Type failed".to_string()),
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

/// Handler for `POST /scroll` – scroll the viewport or a DOM element in the specified tab.
///
/// Coordinates (`x`, `y`) set the scroll origin; `delta_x` / `delta_y` set
/// the scroll amount. When `selector` is set the matched element is scrolled
/// instead of the page viewport.
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

    let tab_id = match resolve_tab_id(request.tab_id, &state).await {
        Ok(id) => id,
        Err(err) => return err.into_response(),
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
                Json(ApiResponse::<()>::error(format!("Failed to scroll: {}", e))),
            )
                .into_response()
        }
    }
}
