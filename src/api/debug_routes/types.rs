//! Shared types and helpers for debug route handlers.

use axum::{http::StatusCode, Json};
use serde::Deserialize;

use crate::api::ipc::{IpcCommand, IpcMessage};
use crate::api::routes::ApiResponse;
use crate::api::server::AppState;

/// Common query parameters for debug endpoints that only need a tab_id.
#[derive(Debug, Deserialize)]
pub struct TabQuery {
    #[serde(default)]
    pub tab_id: Option<String>,
}

/// Resolve tab_id from request or fall back to active tab.
pub async fn resolve_tab_id(state: &AppState, request_tab_id: Option<String>) -> Option<String> {
    request_tab_id.or({
        let browser_state = state.browser_state.read().await;
        browser_state.active_tab_id.clone()
    })
}

/// Evaluate a JS script in a tab and return the raw JSON string.
pub async fn evaluate_in_tab(
    state: &AppState,
    tab_id: &str,
    script: &str,
) -> Result<String, (StatusCode, Json<ApiResponse<()>>)> {
    let command = IpcCommand::EvaluateScript {
        tab_id: tab_id.to_string(),
        script: script.to_string(),
        await_promise: true,
        frame_id: None,
    };

    match state
        .ipc_channel
        .send_command(IpcMessage::Command(command))
        .await
    {
        Ok(response) => {
            if response.success {
                if let Some(data) = response.data {
                    let result_value = match &data {
                        serde_json::Value::Object(map) => {
                            map.get("result").cloned().unwrap_or(data.clone())
                        }
                        _ => data.clone(),
                    };
                    match result_value {
                        serde_json::Value::String(s) => Ok(s),
                        serde_json::Value::Null => Err((
                            StatusCode::INTERNAL_SERVER_ERROR,
                            Json(ApiResponse::<()>::error("Script returned null")),
                        )),
                        other => Ok(other.to_string()),
                    }
                } else {
                    Err((
                        StatusCode::INTERNAL_SERVER_ERROR,
                        Json(ApiResponse::<()>::error("Script returned no data")),
                    ))
                }
            } else {
                Err((
                    StatusCode::BAD_REQUEST,
                    Json(ApiResponse::<()>::error(
                        response
                            .error
                            .unwrap_or_else(|| "Script evaluation failed".to_string()),
                    )),
                ))
            }
        }
        Err(e) => Err((
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ApiResponse::<()>::error(format!("IPC error: {}", e))),
        )),
    }
}

/// Escape a string for safe embedding in JavaScript.
pub fn escape_js(s: &str) -> String {
    s.replace('\\', "\\\\")
        .replace('\'', "\\'")
        .replace('"', "\\\"")
        .replace('\n', "\\n")
        .replace('\r', "\\r")
}
