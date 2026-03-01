//! Tab management route handlers: list, create, and close browser tabs.
//!
//! All handlers forward tab-control commands to the browser core via the IPC
//! channel. State mutations (insert / remove from the in-memory tab map) are
//! applied after a successful IPC round-trip. WebSocket events are broadcast
//! to all connected clients on tab creation and closure.

use axum::{extract::State, http::StatusCode, response::IntoResponse, Json};
use serde::{Deserialize, Serialize};
use tracing::{error, info};

use crate::api::ipc::{IpcCommand, IpcMessage};
use crate::api::server::{AppState, TabState};
use crate::api::websocket::BrowserEvent;
use crate::api::routes::types::{ApiResponse, TabInfo};

// ============================================================================
// Request / Response Types
// ============================================================================

/// Response body for `GET /tabs` – full list of open tabs with the active tab ID.
#[derive(Debug, Serialize)]
pub struct TabsResponse {
    pub tabs: Vec<TabInfo>,
    pub active_tab_id: Option<String>,
}

/// Request body for `POST /tabs/new` – open a new browser tab.
#[derive(Debug, Deserialize)]
pub struct NewTabRequest {
    /// Initial URL for the new tab. Defaults to `about:blank`.
    #[serde(default)]
    pub url: Option<String>,
    /// Whether the new tab should immediately become the active tab. Defaults to `true`.
    #[serde(default)]
    pub active: Option<bool>,
}

/// Response body for `POST /tabs/new` – identifiers of the newly created tab.
#[derive(Debug, Serialize)]
pub struct NewTabResponse {
    pub tab_id: String,
    pub url: String,
}

/// Request body for `POST /tabs/close` – close a tab by its ID.
#[derive(Debug, Deserialize)]
pub struct CloseTabRequest {
    pub tab_id: String,
}

// ============================================================================
// Handlers
// ============================================================================

/// Handler for `GET /tabs` – return all open tabs and the currently active tab ID.
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

/// Handler for `POST /tabs/new` – create a new browser tab and optionally activate it.
///
/// On success, inserts the tab into the shared browser state and broadcasts a
/// `TabCreated` WebSocket event to all connected clients.
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

    let url = request
        .url
        .unwrap_or_else(|| "about:blank".to_string());

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

                // Drop read guard before broadcasting
                drop(browser_state);

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

/// Handler for `POST /tabs/close` – close the tab identified by `tab_id`.
///
/// Removes the tab from the shared browser state and broadcasts a `TabClosed`
/// WebSocket event. If the closed tab was the active tab, the active tab ID is
/// updated to the next available tab (or `None` when no tabs remain).
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
                    browser_state.active_tab_id =
                        browser_state.tabs.keys().next().cloned();
                }

                drop(browser_state);

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
