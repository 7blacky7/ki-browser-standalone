//! Session lifecycle and storage route handlers for creating, listing,
//! deleting sessions, and managing key-value session storage.

use std::collections::HashMap;

use axum::{
    extract::{Path, State},
    http::StatusCode,
    response::IntoResponse,
    Json,
};
use tracing::{error, info};

use crate::api::ipc::{IpcCommand, IpcMessage};
use crate::api::routes::ApiResponse;
use crate::api::server::AppState;
use crate::api::session::{CookieInfo, SessionManager, SessionSnapshot, TabSnapshot};
use super::helpers::{parse_cookies_from_response, parse_storage_from_response, parse_tab_snapshot};
use super::types::*;
use super::SESSION_MANAGER;

/// POST /session/start - Create a new session.
pub(super) async fn create_session(
    State(_state): State<AppState>,
    Json(request): Json<CreateSessionRequest>,
) -> impl IntoResponse {
    let session = SESSION_MANAGER.create_session(request.name).await;
    info!("Created session via API: {}", session.id);
    Json(ApiResponse::success(session)).into_response()
}

/// GET /session/list - List all active sessions.
pub(super) async fn list_sessions(State(_state): State<AppState>) -> impl IntoResponse {
    let sessions = SESSION_MANAGER.list_sessions().await;
    Json(ApiResponse::success(sessions)).into_response()
}

/// GET /session/{id} - Get session details.
pub(super) async fn get_session(
    State(_state): State<AppState>,
    Path(id): Path<String>,
) -> impl IntoResponse {
    match SESSION_MANAGER.get_session(&id).await {
        Some(session) => Json(ApiResponse::success(session)).into_response(),
        None => (
            StatusCode::NOT_FOUND,
            Json(ApiResponse::<()>::error(format!("Session '{}' not found", id))),
        )
            .into_response(),
    }
}

/// DELETE /session/{id} - Delete a session.
pub(super) async fn delete_session(
    State(_state): State<AppState>,
    Path(id): Path<String>,
) -> impl IntoResponse {
    if SESSION_MANAGER.delete_session(&id).await {
        info!("Deleted session via API: {}", id);
        Json(ApiResponse::success(())).into_response()
    } else {
        (
            StatusCode::NOT_FOUND,
            Json(ApiResponse::<()>::error(format!("Session '{}' not found", id))),
        )
            .into_response()
    }
}

/// POST /session/{id}/storage - Store a key-value pair in session storage.
pub(super) async fn set_storage(
    State(_state): State<AppState>,
    Path(id): Path<String>,
    Json(request): Json<SetStorageRequest>,
) -> impl IntoResponse {
    if SESSION_MANAGER
        .set_storage(&id, request.key.clone(), request.value.clone())
        .await
    {
        Json(ApiResponse::success(StorageValueResponse {
            key: request.key,
            value: request.value,
        }))
        .into_response()
    } else {
        (
            StatusCode::NOT_FOUND,
            Json(ApiResponse::<()>::error(format!("Session '{}' not found", id))),
        )
            .into_response()
    }
}

/// GET /session/{id}/storage/{key} - Retrieve a value from session storage.
pub(super) async fn get_storage(
    State(_state): State<AppState>,
    Path((id, key)): Path<(String, String)>,
) -> impl IntoResponse {
    match SESSION_MANAGER.get_storage(&id, &key).await {
        Some(value) => Json(ApiResponse::success(StorageValueResponse { key, value })).into_response(),
        None => {
            if SESSION_MANAGER.get_session(&id).await.is_some() {
                (
                    StatusCode::NOT_FOUND,
                    Json(ApiResponse::<()>::error(format!(
                        "Key '{}' not found in session '{}'",
                        key, id
                    ))),
                )
                    .into_response()
            } else {
                (
                    StatusCode::NOT_FOUND,
                    Json(ApiResponse::<()>::error(format!("Session '{}' not found", id))),
                )
                    .into_response()
            }
        }
    }
}

/// GET /tabs/{tab_id}/cookies - Get cookies for a tab via JavaScript.
///
/// Uses `document.cookie` to read cookies visible to the page. Note that
/// `httpOnly` cookies are not accessible from JavaScript.
pub(super) async fn get_cookies(
    State(state): State<AppState>,
    Path(tab_id): Path<String>,
) -> impl IntoResponse {
    if !state.is_enabled().await {
        return (
            StatusCode::SERVICE_UNAVAILABLE,
            Json(ApiResponse::<Vec<CookieInfo>>::error("API is disabled")),
        )
            .into_response();
    }

    let cmd = IpcCommand::EvaluateScript {
        tab_id: tab_id.clone(),
        script: SessionManager::get_cookies_script().to_string(),
        await_promise: false,
        frame_id: None,
    };

    match state.ipc_channel.send_command(IpcMessage::Command(cmd)).await {
        Ok(resp) if resp.success => {
            let cookies = parse_cookies_from_response(resp.data);
            Json(ApiResponse::success(cookies)).into_response()
        }
        Ok(resp) => (
            StatusCode::BAD_REQUEST,
            Json(ApiResponse::<Vec<CookieInfo>>::error(
                resp.error.unwrap_or_else(|| "Failed to get cookies".to_string()),
            )),
        )
            .into_response(),
        Err(e) => {
            error!("Failed to get cookies for tab {}: {}", tab_id, e);
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ApiResponse::<Vec<CookieInfo>>::error(format!(
                    "IPC error: {}",
                    e
                ))),
            )
                .into_response()
        }
    }
}

/// POST /tabs/{tab_id}/cookies - Set a cookie on a tab via JavaScript.
pub(super) async fn set_cookies(
    State(state): State<AppState>,
    Path(tab_id): Path<String>,
    Json(request): Json<SetCookieRequest>,
) -> impl IntoResponse {
    if !state.is_enabled().await {
        return (
            StatusCode::SERVICE_UNAVAILABLE,
            Json(ApiResponse::<()>::error("API is disabled")),
        )
            .into_response();
    }

    let cookie_info = CookieInfo {
        name: request.name,
        value: request.value,
        domain: request.domain.unwrap_or_default(),
        path: request.path,
        expires: request.expires,
        http_only: false, // Cannot set httpOnly via JS
        secure: request.secure,
        same_site: request.same_site,
    };

    let script = SessionManager::set_cookie_script(&cookie_info);
    let cmd = IpcCommand::EvaluateScript {
        tab_id: tab_id.clone(),
        script,
        await_promise: false,
        frame_id: None,
    };

    match state.ipc_channel.send_command(IpcMessage::Command(cmd)).await {
        Ok(resp) if resp.success => {
            Json(ApiResponse::success(())).into_response()
        }
        Ok(resp) => (
            StatusCode::BAD_REQUEST,
            Json(ApiResponse::<()>::error(
                resp.error.unwrap_or_else(|| "Failed to set cookie".to_string()),
            )),
        )
            .into_response(),
        Err(e) => {
            error!("Failed to set cookie for tab {}: {}", tab_id, e);
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ApiResponse::<()>::error(format!("IPC error: {}", e))),
            )
                .into_response()
        }
    }
}

/// GET /tabs/{tab_id}/local-storage - Get all localStorage entries via JavaScript.
pub(super) async fn get_local_storage(
    State(state): State<AppState>,
    Path(tab_id): Path<String>,
) -> impl IntoResponse {
    if !state.is_enabled().await {
        return (
            StatusCode::SERVICE_UNAVAILABLE,
            Json(ApiResponse::<HashMap<String, String>>::error("API is disabled")),
        )
            .into_response();
    }

    let cmd = IpcCommand::EvaluateScript {
        tab_id: tab_id.clone(),
        script: SessionManager::get_local_storage_script().to_string(),
        await_promise: false,
        frame_id: None,
    };

    match state.ipc_channel.send_command(IpcMessage::Command(cmd)).await {
        Ok(resp) if resp.success => {
            let storage = parse_storage_from_response(resp.data);
            Json(ApiResponse::success(storage)).into_response()
        }
        Ok(resp) => (
            StatusCode::BAD_REQUEST,
            Json(ApiResponse::<HashMap<String, String>>::error(
                resp.error
                    .unwrap_or_else(|| "Failed to get localStorage".to_string()),
            )),
        )
            .into_response(),
        Err(e) => {
            error!("Failed to get localStorage for tab {}: {}", tab_id, e);
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ApiResponse::<HashMap<String, String>>::error(format!(
                    "IPC error: {}",
                    e
                ))),
            )
                .into_response()
        }
    }
}

/// POST /session/{id}/snapshot - Create a state snapshot for a session.
///
/// For each tab in the session, captures the current URL, cookies,
/// localStorage, and sessionStorage via JavaScript.
pub(super) async fn create_snapshot(
    State(state): State<AppState>,
    Path(id): Path<String>,
    Json(request): Json<CreateSnapshotRequest>,
) -> impl IntoResponse {
    if !state.is_enabled().await {
        return (
            StatusCode::SERVICE_UNAVAILABLE,
            Json(ApiResponse::<SessionSnapshot>::error("API is disabled")),
        )
            .into_response();
    }

    let session = match SESSION_MANAGER.get_session(&id).await {
        Some(s) => s,
        None => {
            return (
                StatusCode::NOT_FOUND,
                Json(ApiResponse::<SessionSnapshot>::error(format!(
                    "Session '{}' not found",
                    id
                ))),
            )
                .into_response();
        }
    };

    let mut tab_states: Vec<TabSnapshot> = Vec::new();

    for tab_id in &session.tabs {
        let cmd = IpcCommand::EvaluateScript {
            tab_id: tab_id.clone(),
            script: SessionManager::capture_tab_state_script().to_string(),
            await_promise: false,
            frame_id: None,
        };

        match state.ipc_channel.send_command(IpcMessage::Command(cmd)).await {
            Ok(resp) if resp.success => {
                if let Some(data) = resp.data {
                    let tab_snapshot = parse_tab_snapshot(tab_id, data);
                    tab_states.push(tab_snapshot);
                } else {
                    tab_states.push(TabSnapshot {
                        tab_id: tab_id.clone(),
                        url: String::new(),
                        title: None,
                        cookies: Vec::new(),
                        local_storage: HashMap::new(),
                        session_storage: HashMap::new(),
                    });
                }
            }
            Ok(_) | Err(_) => {
                tracing::warn!(
                    "Could not capture state for tab {} in session {}, using empty snapshot",
                    tab_id, id
                );
                tab_states.push(TabSnapshot {
                    tab_id: tab_id.clone(),
                    url: String::new(),
                    title: None,
                    cookies: Vec::new(),
                    local_storage: HashMap::new(),
                    session_storage: HashMap::new(),
                });
            }
        }
    }

    match SESSION_MANAGER
        .create_snapshot(&id, request.name, request.description, tab_states)
        .await
    {
        Some(snapshot) => Json(ApiResponse::success(snapshot)).into_response(),
        None => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ApiResponse::<SessionSnapshot>::error("Failed to create snapshot")),
        )
            .into_response(),
    }
}

/// GET /session/{id}/snapshots - List all snapshots for a session.
pub(super) async fn list_snapshots(
    State(_state): State<AppState>,
    Path(id): Path<String>,
) -> impl IntoResponse {
    let session = match SESSION_MANAGER.get_session(&id).await {
        Some(s) => s,
        None => {
            return (
                StatusCode::NOT_FOUND,
                Json(ApiResponse::<Vec<SnapshotSummary>>::error(format!(
                    "Session '{}' not found",
                    id
                ))),
            )
                .into_response();
        }
    };

    let summaries: Vec<SnapshotSummary> = session
        .snapshots
        .iter()
        .map(|snap| SnapshotSummary {
            name: snap.name.clone(),
            description: snap.description.clone(),
            created_at: snap.created_at.clone(),
            tab_count: snap.tab_states.len(),
        })
        .collect();

    Json(ApiResponse::success(summaries)).into_response()
}
