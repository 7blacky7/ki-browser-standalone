//! Tab management route handlers: list, create, and close browser tabs.
//!
//! Handles the /tabs, /tabs/new, and /tabs/close endpoints that manage
//! the browser tab lifecycle via IPC commands to the browser core.

use axum::{extract::State, http::StatusCode, response::IntoResponse, Json};
use tracing::{error, info};

use crate::api::ipc::{IpcCommand, IpcMessage};
use crate::api::server::{AppState, TabState};
use crate::api::websocket::BrowserEvent;

use super::types::{
    ApiResponse, CloseTabRequest, NewTabRequest, NewTabResponse, TabInfo, TabsResponse,
};

/// GET /tabs - List all currently open tabs.
///
/// Returns all tab states and the active tab ID. Returns 503 if the API is disabled.
pub async fn list_tabs(State(state): State<AppState>) -> impl IntoResponse {
    if !state.is_enabled().await {
        return (
            StatusCode::SERVICE_UNAVAILABLE,
            Json(ApiResponse::<TabsResponse>::error("API is disabled")),
        )
            .into_response();
    }

    let browser_state = state.browser_state.read().await;
    let active_tab_id = browser_state.active_tab_id.clone();

    let tabs: Vec<TabInfo> = browser_state
        .tabs
        .values()
        .map(|tab| {
            let mut info = TabInfo::from(tab);
            info.is_active = Some(&info.id) == active_tab_id.as_ref();
            info
        })
        .collect();

    Json(ApiResponse::success(TabsResponse {
        tabs,
        active_tab_id,
    }))
    .into_response()
}

/// POST /tabs/new - Create a new browser tab via IPC and update local state.
///
/// Sends a CreateTab IPC command to the browser core, updates the shared tab
/// map, optionally activates the new tab, and broadcasts a TabCreated event.
pub async fn create_tab(
    State(state): State<AppState>,
    Json(request): Json<NewTabRequest>,
) -> impl IntoResponse {
    if !state.is_enabled().await {
        return (
            StatusCode::SERVICE_UNAVAILABLE,
            Json(ApiResponse::<NewTabResponse>::error("API is disabled")),
        )
            .into_response();
    }

    let url = request.url.unwrap_or_else(|| "about:blank".to_string());

    let command = IpcCommand::CreateTab {
        url: url.clone(),
        active: request.active.unwrap_or(true),
    };

    match state
        .ipc_channel
        .send_command(IpcMessage::Command(command))
        .await
    {
        Ok(response) => {
            if let Some(tab_id) = response.tab_id {
                let mut browser_state = state.browser_state.write().await;
                let tab = TabState {
                    id: tab_id.clone(),
                    url: url.clone(),
                    title: "New Tab".to_string(),
                    is_loading: true,
                    can_go_back: false,
                    can_go_forward: false,
                };
                browser_state.tabs.insert(tab_id.clone(), tab);

                if request.active.unwrap_or(true) {
                    browser_state.active_tab_id = Some(tab_id.clone());
                }

                state
                    .ws_handler
                    .broadcast(BrowserEvent::TabCreated {
                        tab_id: tab_id.clone(),
                        url: url.clone(),
                    })
                    .await;

                info!("Created new tab: {}", tab_id);

                Json(ApiResponse::success(NewTabResponse { tab_id, url })).into_response()
            } else {
                (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    Json(ApiResponse::<NewTabResponse>::error("Failed to create tab")),
                )
                    .into_response()
            }
        }
        Err(e) => {
            error!("Failed to create tab: {}", e);
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ApiResponse::<NewTabResponse>::error(format!(
                    "Failed to create tab: {}",
                    e
                ))),
            )
                .into_response()
        }
    }
}

/// POST /tabs/close - Close a tab by ID via IPC and remove it from local state.
///
/// Sends a CloseTab IPC command, removes the tab from the shared map,
/// updates the active tab if the closed tab was active, and broadcasts TabClosed.
pub async fn close_tab(
    State(state): State<AppState>,
    Json(request): Json<CloseTabRequest>,
) -> impl IntoResponse {
    if !state.is_enabled().await {
        return (
            StatusCode::SERVICE_UNAVAILABLE,
            Json(ApiResponse::<()>::error("API is disabled")),
        )
            .into_response();
    }

    let command = IpcCommand::CloseTab {
        tab_id: request.tab_id.clone(),
    };

    match state
        .ipc_channel
        .send_command(IpcMessage::Command(command))
        .await
    {
        Ok(response) => {
            if response.success {
                let mut browser_state = state.browser_state.write().await;
                browser_state.tabs.remove(&request.tab_id);

                if browser_state.active_tab_id.as_ref() == Some(&request.tab_id) {
                    browser_state.active_tab_id = browser_state.tabs.keys().next().cloned();
                }

                state
                    .ws_handler
                    .broadcast(BrowserEvent::TabClosed {
                        tab_id: request.tab_id.clone(),
                    })
                    .await;

                info!("Closed tab: {}", request.tab_id);

                Json(ApiResponse::success(())).into_response()
            } else {
                (
                    StatusCode::NOT_FOUND,
                    Json(ApiResponse::<()>::error(
                        response
                            .error
                            .unwrap_or_else(|| "Tab not found".to_string()),
                    )),
                )
                    .into_response()
            }
        }
        Err(e) => {
            error!("Failed to close tab: {}", e);
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ApiResponse::<()>::error(format!(
                    "Failed to close tab: {}",
                    e
                ))),
            )
                .into_response()
        }
    }
}
