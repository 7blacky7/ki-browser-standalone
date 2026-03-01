//! REST API routes and handlers
//!
//! Defines all HTTP endpoints for browser control operations.

use axum::{
    extract::{Path, Query, State},
    http::StatusCode,
    response::IntoResponse,
    routing::{get, post},
    Json, Router,
};
use serde::{Deserialize, Serialize};
use tracing::{error, info, warn};
use uuid::Uuid;
use utoipa::{IntoParams, ToSchema};

use crate::api::cdp_mapping::{CdpTargetInfo, CdpTargetLookupResponse, CdpTargetsResponse};
use crate::api::server::{AppState, TabState};
use crate::api::ipc::{IpcCommand, IpcMessage};
use crate::api::websocket::{self, BrowserEvent};
use crate::api::agent_routes::agent_routes;
use crate::api::gui_routes::gui_routes;
use crate::api::vision_routes::vision_routes;
use crate::api::extraction_routes::extraction_routes;
use crate::api::batch_routes::batch_session_routes;

// ============================================================================
// Request/Response Structs
// ============================================================================

/// Standard API response wrapper
#[derive(Debug, Serialize)]
pub struct ApiResponse<T: Serialize> {
    pub success: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub data: Option<T>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

impl<T: Serialize> ApiResponse<T> {
    pub fn success(data: T) -> Self {
        Self {
            success: true,
            data: Some(data),
            error: None,
        }
    }

    pub fn error(message: impl Into<String>) -> ApiResponse<()> {
        ApiResponse {
            success: false,
            data: None,
            error: Some(message.into()),
        }
    }
}

/// Health check response
#[derive(Debug, Serialize, ToSchema)]
pub struct HealthResponse {
    pub status: String,
    pub version: String,
    pub api_enabled: bool,
}

/// Tab information response
#[derive(Debug, Serialize, Clone, ToSchema)]
pub struct TabInfo {
    pub id: String,
    pub url: String,
    pub title: String,
    pub is_loading: bool,
    pub is_active: bool,
    pub can_go_back: bool,
    pub can_go_forward: bool,
}

impl From<&TabState> for TabInfo {
    fn from(state: &TabState) -> Self {
        Self {
            id: state.id.clone(),
            url: state.url.clone(),
            title: state.title.clone(),
            is_loading: state.is_loading,
            is_active: false, // Set by caller
            can_go_back: state.can_go_back,
            can_go_forward: state.can_go_forward,
        }
    }
}

/// List tabs response
#[derive(Debug, Serialize, ToSchema)]
pub struct TabsResponse {
    pub tabs: Vec<TabInfo>,
    pub active_tab_id: Option<String>,
}

/// Create new tab request
#[derive(Debug, Deserialize, ToSchema)]
pub struct NewTabRequest {
    #[serde(default)]
    pub url: Option<String>,
    #[serde(default)]
    pub active: Option<bool>,
}

/// Create new tab response
#[derive(Debug, Serialize, ToSchema)]
pub struct NewTabResponse {
    pub tab_id: String,
    pub url: String,
}

/// Close tab request
#[derive(Debug, Deserialize, ToSchema)]
pub struct CloseTabRequest {
    pub tab_id: String,
}

/// Navigate request
#[derive(Debug, Deserialize, ToSchema)]
pub struct NavigateRequest {
    #[serde(default)]
    pub tab_id: Option<String>,
    pub url: String,
}

/// Click request - supports both coordinates and selectors
#[derive(Debug, Deserialize, ToSchema)]
pub struct ClickRequest {
    #[serde(default)]
    pub tab_id: Option<String>,
    #[serde(default)]
    pub x: Option<i32>,
    #[serde(default)]
    pub y: Option<i32>,
    #[serde(default)]
    pub selector: Option<String>,
    #[serde(default = "default_click_button")]
    pub button: String,
    #[serde(default)]
    pub modifiers: Option<Vec<String>>,
    #[serde(default)]
    pub frame_id: Option<String>,
}

fn default_click_button() -> String {
    "left".to_string()
}

/// Drag request - drag from one position to another
#[derive(Debug, Deserialize)]
pub struct DragRequest {
    #[serde(default)]
    pub tab_id: Option<String>,
    pub from_x: i32,
    pub from_y: i32,
    pub to_x: i32,
    pub to_y: i32,
    #[serde(default)]
    pub steps: Option<u32>,
    #[serde(default)]
    pub duration_ms: Option<u64>,
}

/// Type text request
#[derive(Debug, Deserialize, ToSchema)]
pub struct TypeRequest {
    #[serde(default)]
    pub tab_id: Option<String>,
    pub text: String,
    #[serde(default)]
    pub selector: Option<String>,
    #[serde(default)]
    pub clear_first: Option<bool>,
    #[serde(default)]
    pub frame_id: Option<String>,
}

/// Evaluate JavaScript request
#[derive(Debug, Deserialize, ToSchema)]
pub struct EvaluateRequest {
    #[serde(default)]
    pub tab_id: Option<String>,
    pub script: String,
    #[serde(default)]
    pub await_promise: Option<bool>,
    #[serde(default)]
    pub frame_id: Option<String>,
}

/// Evaluate JavaScript response
#[derive(Debug, Serialize, ToSchema)]
pub struct EvaluateResponse {
    /// The result of the JavaScript evaluation as arbitrary JSON
    #[schema(value_type = Object)]
    pub result: serde_json::Value,
}

/// Frames query parameters
#[derive(Debug, Deserialize)]
pub struct FramesQuery {
    pub tab_id: String,
}

/// Screenshot query parameters
#[derive(Debug, Deserialize, ToSchema, IntoParams)]
pub struct ScreenshotQuery {
    #[serde(default)]
    pub tab_id: Option<String>,
    #[serde(default = "default_screenshot_format")]
    pub format: String,
    #[serde(default)]
    pub quality: Option<u8>,
    #[serde(default)]
    pub full_page: Option<bool>,
    #[serde(default)]
    pub selector: Option<String>,
    /// Clip/Zoom region: x coordinate
    #[serde(default)]
    pub clip_x: Option<f64>,
    /// Clip/Zoom region: y coordinate
    #[serde(default)]
    pub clip_y: Option<f64>,
    /// Clip/Zoom region: width
    #[serde(default)]
    pub clip_width: Option<f64>,
    /// Clip/Zoom region: height
    #[serde(default)]
    pub clip_height: Option<f64>,
    /// Scale factor for clip region (default 1.0, use 2.0 to zoom 2x)
    #[serde(default)]
    pub clip_scale: Option<f64>,
}

fn default_screenshot_format() -> String {
    "png".to_string()
}

/// Screenshot response
#[derive(Debug, Serialize, ToSchema)]
pub struct ScreenshotResponse {
    pub data: String, // Base64 encoded image
    pub format: String,
    pub width: u32,
    pub height: u32,
}

/// Scroll request
#[derive(Debug, Deserialize, ToSchema)]
pub struct ScrollRequest {
    #[serde(default)]
    pub tab_id: Option<String>,
    #[serde(default)]
    pub x: Option<i32>,
    #[serde(default)]
    pub y: Option<i32>,
    #[serde(default)]
    pub delta_x: Option<i32>,
    #[serde(default)]
    pub delta_y: Option<i32>,
    #[serde(default)]
    pub selector: Option<String>,
    #[serde(default)]
    pub behavior: Option<String>, // "auto", "smooth", "instant"
}

/// Find element query parameters
#[derive(Debug, Deserialize, ToSchema, IntoParams)]
pub struct FindElementQuery {
    #[serde(default)]
    pub tab_id: Option<String>,
    pub selector: String,
    #[serde(default)]
    pub timeout: Option<u64>,
}

/// Element information response
#[derive(Debug, Serialize, ToSchema)]
pub struct ElementInfo {
    pub found: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tag_name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub text_content: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[schema(value_type = Option<Object>)]
    pub attributes: Option<serde_json::Map<String, serde_json::Value>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub bounding_box: Option<BoundingBox>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub is_visible: Option<bool>,
}

/// Element bounding box
#[derive(Debug, Serialize, ToSchema)]
pub struct BoundingBox {
    pub x: f64,
    pub y: f64,
    pub width: f64,
    pub height: f64,
}

/// Annotate elements request
#[derive(Debug, Deserialize)]
pub struct AnnotateRequest {
    #[serde(default)]
    pub tab_id: Option<String>,
    #[serde(default)]
    pub types: Option<Vec<String>>,
    #[serde(default)]
    pub selector: Option<String>,
    #[serde(default)]
    pub ocr: Option<bool>,
    #[serde(default = "default_ocr_lang")]
    pub ocr_lang: String,
}

fn default_ocr_lang() -> String {
    "deu+eng".to_string()
}

/// Annotate elements response
#[derive(Debug, Serialize)]
pub struct AnnotateResponse {
    pub screenshot: String,
    pub elements: serde_json::Value,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ocr_text: Option<String>,
}

/// DOM snapshot query parameters
#[derive(Debug, Deserialize)]
pub struct DomSnapshotQuery {
    #[serde(default)]
    pub tab_id: Option<String>,
    #[serde(default = "default_max_nodes")]
    pub max_nodes: u32,
    #[serde(default = "default_include_text")]
    pub include_text: bool,
}

fn default_max_nodes() -> u32 {
    1000
}

fn default_include_text() -> bool {
    true
}

/// API toggle request
#[derive(Debug, Deserialize, ToSchema)]
pub struct ApiToggleRequest {
    pub enabled: bool,
}

/// API status response
#[derive(Debug, Serialize, ToSchema)]
pub struct ApiStatusResponse {
    pub enabled: bool,
    pub port: u16,
    pub connected_clients: usize,
}

// ============================================================================
// Route Handlers
// ============================================================================

/// GET /health - Health check endpoint
#[utoipa::path(
    get,
    path = "/health",
    tag = "health",
    responses(
        (status = 200, description = "Server is healthy", body = HealthResponse)
    )
)]
pub async fn health_check(State(state): State<AppState>) -> impl IntoResponse {
    let api_enabled = state.is_enabled().await;

    Json(ApiResponse::success(HealthResponse {
        status: "healthy".to_string(),
        version: env!("CARGO_PKG_VERSION").to_string(),
        api_enabled,
    }))
}

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

    // Send IPC command to create tab
    let command = IpcCommand::CreateTab {
        url: url.clone(),
        active: request.active.unwrap_or(true),
    };

    match state.ipc_channel.send_command(IpcMessage::Command(command)).await {
        Ok(response) => {
            if let Some(tab_id) = response.tab_id {
                // Update local state
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

                // Broadcast event
                state.ws_handler.broadcast(BrowserEvent::TabCreated {
                    tab_id: tab_id.clone(),
                    url: url.clone(),
                }).await;

                info!("Created new tab: {}", tab_id);

                Json(ApiResponse::success(NewTabResponse {
                    tab_id,
                    url,
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

                info!("Navigating tab {} to {}", tab_id, request.url);

                Json(ApiResponse::success(())).into_response()
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

/// GET /frames - Get frame tree for a tab
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

    match state.ipc_channel.send_command(IpcMessage::Command(command)).await {
        Ok(response) => {
            if response.success {
                if let Some(data) = response.data {
                    // Parse screenshot data from response
                    if let Some(screenshot) = data.get("screenshot").and_then(|v| v.as_str()) {
                        let width = data.get("width").and_then(|v| v.as_u64()).unwrap_or(0) as u32;
                        let height = data.get("height").and_then(|v| v.as_u64()).unwrap_or(0) as u32;

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

/// GET /cdp - Returns CDP remote debugging connection info for Playwright/DevTools integration
async fn cdp_info(
    State(state): State<AppState>,
) -> impl IntoResponse {
    let cdp_port = state.cdp_port.unwrap_or(9222);
    let base = format!("http://127.0.0.1:{}", cdp_port);
    Json(serde_json::json!({
        "success": true,
        "data": {
            "base_url": base,
            "json_list": format!("{}/json/list", base),
            "json_version": format!("{}/json/version", base),
            "ws_base": format!("ws://127.0.0.1:{}", cdp_port),
            "port": cdp_port
        }
    }))
}

/// POST /api/toggle - Toggle API enabled state
#[utoipa::path(
    post,
    path = "/api/toggle",
    tag = "api",
    request_body = ApiToggleRequest,
    responses(
        (status = 200, description = "API state toggled", body = ApiStatusResponse)
    )
)]
pub async fn toggle_api(
    State(state): State<AppState>,
    Json(request): Json<ApiToggleRequest>,
) -> impl IntoResponse {
    state.set_enabled(request.enabled).await;

    info!("API {} by request", if request.enabled { "enabled" } else { "disabled" });

    Json(ApiResponse::success(ApiStatusResponse {
        enabled: request.enabled,
        port: 0, // Port info not available here
        connected_clients: state.ws_handler.client_count().await,
    }))
}

/// GET /api/status - Get current API status
#[utoipa::path(
    get,
    path = "/api/status",
    tag = "api",
    responses(
        (status = 200, description = "Current API status", body = ApiStatusResponse)
    )
)]
pub async fn api_status(State(state): State<AppState>) -> impl IntoResponse {
    let enabled = state.is_enabled().await;
    let connected_clients = state.ws_handler.client_count().await;

    Json(ApiResponse::success(ApiStatusResponse {
        enabled,
        port: 0, // Port info not available here
        connected_clients,
    }))
}

// ============================================================================
// CDP Mapping Handlers
// ============================================================================

/// GET /cdp/targets - List all CDP targets with their mapped ki-browser tab UUIDs.
///
/// Returns remote debugging connection info and all known tab-to-target mappings,
/// enabling external CDP clients to discover which WebSocket URL corresponds to
/// which ki-browser tab.
pub async fn cdp_targets(State(state): State<AppState>) -> impl IntoResponse {
    if !state.is_enabled().await {
        return (
            StatusCode::SERVICE_UNAVAILABLE,
            Json(ApiResponse::<CdpTargetsResponse>::error("API is disabled")),
        )
            .into_response();
    }

    let mapping = &state.cdp_mapping;
    let browser_state = state.browser_state.read().await;

    let targets: Vec<CdpTargetInfo> = mapping
        .all_mappings()
        .into_iter()
        .map(|(tab_uuid, target_id)| {
            let tab_id_str = tab_uuid.to_string();
            let (url, title) = browser_state
                .tabs
                .get(&tab_id_str)
                .map(|t| (t.url.clone(), t.title.clone()))
                .unwrap_or_else(|| ("unknown".to_string(), "Unknown".to_string()));

            CdpTargetInfo {
                tab_id: tab_id_str,
                target_id: target_id.clone(),
                target_type: "page".to_string(),
                ws_url: mapping.target_ws_url(&target_id),
                url,
                title,
            }
        })
        .collect();

    Json(ApiResponse::success(CdpTargetsResponse {
        remote_debugging_port: mapping.remote_debugging_port(),
        browser_ws_url: mapping.browser_ws_url(),
        targets,
    }))
    .into_response()
}

/// GET /cdp/target/:tab_id - Look up the CDP TargetId for a specific ki-browser tab UUID.
///
/// Returns the CDP target identifier and WebSocket URL for connecting to the
/// specified tab via Chrome DevTools Protocol.
pub async fn cdp_target_by_tab(
    State(state): State<AppState>,
    Path(tab_id): Path<String>,
) -> impl IntoResponse {
    if !state.is_enabled().await {
        return (
            StatusCode::SERVICE_UNAVAILABLE,
            Json(ApiResponse::<CdpTargetLookupResponse>::error("API is disabled")),
        )
            .into_response();
    }

    let uuid = match Uuid::parse_str(&tab_id) {
        Ok(u) => u,
        Err(_) => {
            return (
                StatusCode::BAD_REQUEST,
                Json(ApiResponse::<CdpTargetLookupResponse>::error(
                    "Invalid tab UUID format",
                )),
            )
                .into_response();
        }
    };

    let mapping = &state.cdp_mapping;

    match mapping.get_target_id(&uuid) {
        Some(target_id) => {
            let ws_url = mapping.target_ws_url(&target_id);
            Json(ApiResponse::success(CdpTargetLookupResponse {
                tab_id: tab_id.clone(),
                target_id,
                ws_url,
            }))
            .into_response()
        }
        None => {
            warn!("CDP target lookup failed: no mapping for tab {}", tab_id);
            (
                StatusCode::NOT_FOUND,
                Json(ApiResponse::<CdpTargetLookupResponse>::error(format!(
                    "No CDP target mapping found for tab: {}",
                    tab_id
                ))),
            )
                .into_response()
        }
    }
}

// ============================================================================
// Router Configuration
// ============================================================================

/// Create the API router with all routes configured
pub fn create_router(state: AppState) -> Router {
    Router::new()
        // Health check
        .route("/health", get(health_check))

        // Tab management
        .route("/tabs", get(list_tabs))
        .route("/tabs/new", post(create_tab))
        .route("/tabs/close", post(close_tab))

        // Navigation and interaction
        .route("/navigate", post(navigate))
        .route("/click", post(click))
        .route("/drag", post(drag))
        .route("/type", post(type_text))
        .route("/evaluate", post(evaluate))
        .route("/screenshot", get(screenshot))
        .route("/scroll", post(scroll))
        .route("/frames", get(get_frames))

        // DOM operations
        .route("/dom/element", get(find_element))
        .route("/dom/annotate", post(annotate_elements))
        .route("/dom/snapshot", get(dom_snapshot))

        // CDP remote debugging info
        .route("/cdp", get(cdp_info))

        // CDP tab mapping
        .route("/cdp/targets", get(cdp_targets))
        .route("/cdp/target/{tab_id}", get(cdp_target_by_tab))

        // API management
        .route("/api/toggle", post(toggle_api))
        .route("/api/status", get(api_status))

        // DOM extraction routes (structured data, content, forms)
        .merge(extraction_routes())

        // Batch operations and session management routes
        .merge(batch_session_routes())

        // Multi-agent session management and tab ownership
        .merge(agent_routes())

        // Vision overlay for KI agent annotated screenshots
        .merge(vision_routes())

        // GUI window visibility control (toggle, show, hide, status)
        .merge(gui_routes())

        // OCR endpoints
        .merge(crate::api::ocr_routes::ocr_routes())

        // WebSocket endpoint
        .route("/ws", get(websocket::ws_handler))

        .with_state(state)
}
