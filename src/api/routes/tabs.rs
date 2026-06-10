//! Tab management route handlers for creating, listing, and closing browser tabs.

use axum::{
    extract::{Path, State},
    http::StatusCode,
    response::IntoResponse,
    Json,
};
use tracing::{error, info};

use crate::api::server::AppState;
use crate::api::ipc::{IpcCommand, IpcMessage};
use crate::api::websocket::BrowserEvent;
use super::types::*;

/// GET /tabs - List all tabs
#[utoipa::path(
    get,
    path = "/tabs",
    tag = "tabs",
    responses(
        (status = 200, description = "List of all open tabs", body = TabsResponse),
        (status = 503, description = "API is disabled")
    )
)]
pub async fn list_tabs(State(state): State<AppState>) -> impl IntoResponse {
    if !state.is_enabled().await {
        return (
            StatusCode::SERVICE_UNAVAILABLE,
            Json(ApiResponse::<TabsResponse>::error("API is disabled")),
        ).into_response();
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
    })).into_response()
}

/// POST /tabs/new - Create a new tab
#[utoipa::path(
    post,
    path = "/tabs/new",
    tag = "tabs",
    request_body = NewTabRequest,
    responses(
        (status = 200, description = "Tab created successfully", body = NewTabResponse),
        (status = 500, description = "Failed to create tab"),
        (status = 503, description = "API is disabled")
    )
)]
pub async fn create_tab(
    State(state): State<AppState>,
    Json(request): Json<NewTabRequest>,
) -> impl IntoResponse {
    if !state.is_enabled().await {
        return (
            StatusCode::SERVICE_UNAVAILABLE,
            Json(ApiResponse::<NewTabResponse>::error("API is disabled")),
        ).into_response();
    }

    let url = request.url.unwrap_or_else(|| "about:blank".to_string());

    // Resolve an optional inherited session: inline bundle wins, otherwise a
    // stored session_id is loaded (and decrypted) from the session store.
    let session_bundle = match request.session_bundle.clone() {
        Some(bundle) => Some(bundle),
        None => match &request.session_id {
            Some(id) => match &state.session_store {
                Some(store) => match store.load(id).await {
                    Ok(Some(bundle)) => Some(bundle),
                    Ok(None) => {
                        return (
                            StatusCode::NOT_FOUND,
                            Json(ApiResponse::<NewTabResponse>::error(format!(
                                "Unknown session_id: {}", id
                            ))),
                        ).into_response();
                    }
                    Err(e) => {
                        return (
                            StatusCode::INTERNAL_SERVER_ERROR,
                            Json(ApiResponse::<NewTabResponse>::error(format!(
                                "Failed to load session: {}", e
                            ))),
                        ).into_response();
                    }
                },
                None => {
                    return (
                        StatusCode::SERVICE_UNAVAILABLE,
                        Json(ApiResponse::<NewTabResponse>::error(
                            "Session store unavailable",
                        )),
                    ).into_response();
                }
            },
            None => None,
        },
    };

    // Send IPC command to create tab (identity is resolved in the browser handler
    // where engine + viewport are known; default = consistent random Chrome profile)
    let command = IpcCommand::CreateTab {
        url: url.clone(),
        active: request.active.unwrap_or(true),
        identity: request.identity.clone(),
        session_bundle: session_bundle.map(Box::new),
    };

    match state.ipc_channel.send_command(IpcMessage::Command(command)).await {
        Ok(response) => {
            if let Some(tab_id) = response.tab_id {
                // Update local state
                let mut browser_state = state.browser_state.write().await;
                let tab = crate::api::server::TabState {
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

                // Broadcast event
                state.ws_handler.broadcast(BrowserEvent::TabCreated {
                    tab_id: tab_id.clone(),
                    url: url.clone(),
                }).await;

                info!("Created new tab: {}", tab_id);

                // Self-documenting: include the resolved identity in the response.
                let identity = tab_identity_summary(&state, &tab_id);

                Json(ApiResponse::success(NewTabResponse {
                    tab_id,
                    url,
                    identity,
                })).into_response()
            } else {
                (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    Json(ApiResponse::<NewTabResponse>::error("Failed to create tab")),
                ).into_response()
            }
        }
        Err(e) => {
            error!("Failed to create tab: {}", e);
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ApiResponse::<NewTabResponse>::error(format!("Failed to create tab: {}", e))),
            ).into_response()
        }
    }
}

/// Looks up the resolved stealth identity of a tab from the CEF engine.
#[cfg(feature = "cef-browser")]
fn tab_identity_summary(state: &AppState, tab_id: &str) -> Option<serde_json::Value> {
    let engine = state.cef_engine.as_ref()?;
    let uuid = uuid::Uuid::parse_str(tab_id).ok()?;
    engine
        .get_tab_stealth(&uuid)
        .map(|s| crate::api::identity::identity_summary(&s))
}

#[cfg(not(feature = "cef-browser"))]
fn tab_identity_summary(_state: &AppState, _tab_id: &str) -> Option<serde_json::Value> {
    None
}

/// GET /tabs/{tab_id}/identity - Inspect the active stealth identity of a tab
///
/// Returns the complete externally visible identity (user agent, platform,
/// languages + Accept-Language, hardware, WebGL strings, screen, timezone).
#[utoipa::path(
    get,
    path = "/tabs/{tab_id}/identity",
    tag = "tabs",
    params(("tab_id" = String, Path, description = "Tab UUID")),
    responses(
        (status = 200, description = "Active stealth identity of the tab"),
        (status = 400, description = "Invalid tab ID"),
        (status = 404, description = "Tab not found"),
        (status = 503, description = "API is disabled or engine unavailable")
    )
)]
pub async fn get_tab_identity(
    State(state): State<AppState>,
    Path(tab_id): Path<String>,
) -> impl IntoResponse {
    if !state.is_enabled().await {
        return (
            StatusCode::SERVICE_UNAVAILABLE,
            Json(ApiResponse::<serde_json::Value>::error("API is disabled")),
        ).into_response();
    }

    #[cfg(feature = "cef-browser")]
    {
        let uuid = match uuid::Uuid::parse_str(&tab_id) {
            Ok(u) => u,
            Err(_) => {
                return (
                    StatusCode::BAD_REQUEST,
                    Json(ApiResponse::<serde_json::Value>::error("Invalid tab ID format")),
                ).into_response();
            }
        };

        if let Some(engine) = &state.cef_engine {
            return match engine.get_tab_stealth(&uuid) {
                Some(stealth) => Json(ApiResponse::success(
                    crate::api::identity::identity_summary(&stealth),
                )).into_response(),
                None => (
                    StatusCode::NOT_FOUND,
                    Json(ApiResponse::<serde_json::Value>::error(format!(
                        "Tab not found: {}", tab_id
                    ))),
                ).into_response(),
            };
        }
    }

    (
        StatusCode::SERVICE_UNAVAILABLE,
        Json(ApiResponse::<serde_json::Value>::error(
            "Identity introspection requires the CEF browser engine",
        )),
    ).into_response()
}

/// POST /tabs/close - Close a tab
#[utoipa::path(
    post,
    path = "/tabs/close",
    tag = "tabs",
    request_body = CloseTabRequest,
    responses(
        (status = 200, description = "Tab closed successfully"),
        (status = 404, description = "Tab not found"),
        (status = 503, description = "API is disabled")
    )
)]
pub async fn close_tab(
    State(state): State<AppState>,
    Json(request): Json<CloseTabRequest>,
) -> impl IntoResponse {
    if !state.is_enabled().await {
        return (
            StatusCode::SERVICE_UNAVAILABLE,
            Json(ApiResponse::<()>::error("API is disabled")),
        ).into_response();
    }

    let command = IpcCommand::CloseTab { tab_id: request.tab_id.clone() };

    match state.ipc_channel.send_command(IpcMessage::Command(command)).await {
        Ok(response) => {
            if response.success {
                // Update local state
                let mut browser_state = state.browser_state.write().await;
                browser_state.tabs.remove(&request.tab_id);

                if browser_state.active_tab_id.as_ref() == Some(&request.tab_id) {
                    browser_state.active_tab_id = browser_state.tabs.keys().next().cloned();
                }
                drop(browser_state);

                // Remove any per-tab OCR engine overrides so closed tabs don't leak.
                state
                    .ocr_config
                    .write()
                    .await
                    .per_tab
                    .remove(&request.tab_id);

                // Broadcast event
                state.ws_handler.broadcast(BrowserEvent::TabClosed {
                    tab_id: request.tab_id.clone(),
                }).await;

                info!("Closed tab: {}", request.tab_id);

                Json(ApiResponse::success(())).into_response()
            } else {
                (
                    StatusCode::NOT_FOUND,
                    Json(ApiResponse::<()>::error(response.error.unwrap_or_else(|| "Tab not found".to_string()))),
                ).into_response()
            }
        }
        Err(e) => {
            error!("Failed to close tab: {}", e);
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ApiResponse::<()>::error(format!("Failed to close tab: {}", e))),
            ).into_response()
        }
    }
}
