//! REST API routes and handlers
//!
//! Defines all HTTP endpoints for browser control operations.

use axum::{
    extract::{Query, State},
    http::StatusCode,
    response::IntoResponse,
    routing::{get, post},
    Json, Router,
};
use serde::{Deserialize, Serialize};
use tracing::{error, info};

use crate::api::server::{AppState, TabState};
use crate::api::ipc::{IpcCommand, IpcMessage};
use crate::api::websocket::BrowserEvent;

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
#[derive(Debug, Serialize)]
pub struct HealthResponse {
    pub status: String,
    pub version: String,
    pub api_enabled: bool,
}

/// Tab information response
#[derive(Debug, Serialize, Clone)]
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
#[derive(Debug, Serialize)]
pub struct TabsResponse {
    pub tabs: Vec<TabInfo>,
    pub active_tab_id: Option<String>,
}

/// Create new tab request
#[derive(Debug, Deserialize)]
pub struct NewTabRequest {
    #[serde(default)]
    pub url: Option<String>,
    #[serde(default)]
    pub active: Option<bool>,
}

/// Create new tab response
#[derive(Debug, Serialize)]
pub struct NewTabResponse {
    pub tab_id: String,
    pub url: String,
}

/// Close tab request
#[derive(Debug, Deserialize)]
pub struct CloseTabRequest {
    pub tab_id: String,
}

/// Navigate request
#[derive(Debug, Deserialize)]
pub struct NavigateRequest {
    #[serde(default)]
    pub tab_id: Option<String>,
    pub url: String,
}

/// Click request - supports both coordinates and selectors
#[derive(Debug, Deserialize)]
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
}

fn default_click_button() -> String {
    "left".to_string()
}

/// Type text request
#[derive(Debug, Deserialize)]
pub struct TypeRequest {
    #[serde(default)]
    pub tab_id: Option<String>,
    pub text: String,
    #[serde(default)]
    pub selector: Option<String>,
    #[serde(default)]
    pub clear_first: Option<bool>,
}

/// Evaluate JavaScript request
#[derive(Debug, Deserialize)]
pub struct EvaluateRequest {
    #[serde(default)]
    pub tab_id: Option<String>,
    pub script: String,
    #[serde(default)]
    pub await_promise: Option<bool>,
}

/// Evaluate JavaScript response
#[derive(Debug, Serialize)]
pub struct EvaluateResponse {
    pub result: serde_json::Value,
}

/// Screenshot query parameters
#[derive(Debug, Deserialize)]
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
}

fn default_screenshot_format() -> String {
    "png".to_string()
}

/// Screenshot response
#[derive(Debug, Serialize)]
pub struct ScreenshotResponse {
    pub data: String, // Base64 encoded image
    pub format: String,
    pub width: u32,
    pub height: u32,
}

/// Scroll request
#[derive(Debug, Deserialize)]
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
#[derive(Debug, Deserialize)]
pub struct FindElementQuery {
    #[serde(default)]
    pub tab_id: Option<String>,
    pub selector: String,
    #[serde(default)]
    pub timeout: Option<u64>,
}

/// Element information response
#[derive(Debug, Serialize)]
pub struct ElementInfo {
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

/// Element bounding box
#[derive(Debug, Serialize)]
pub struct BoundingBox {
    pub x: f64,
    pub y: f64,
    pub width: f64,
    pub height: f64,
}

/// API toggle request
#[derive(Debug, Deserialize)]
pub struct ApiToggleRequest {
    pub enabled: bool,
}

/// API status response
#[derive(Debug, Serialize)]
pub struct ApiStatusResponse {
    pub enabled: bool,
    pub port: u16,
    pub connected_clients: usize,
}

// ============================================================================
// Route Handlers
// ============================================================================

/// GET /health - Health check endpoint
pub async fn health_check(State(state): State<AppState>) -> impl IntoResponse {
    let api_enabled = state.is_enabled().await;

    Json(ApiResponse::success(HealthResponse {
        status: "healthy".to_string(),
        version: env!("CARGO_PKG_VERSION").to_string(),
        api_enabled,
    }))
}

/// GET /tabs - List all tabs
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

    let tab_id = match request.tab_id.or_else(|| {
        // Use active tab if not specified
        let browser_state = futures::executor::block_on(state.browser_state.read());
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

    let tab_id = match request.tab_id.or_else(|| {
        let browser_state = futures::executor::block_on(state.browser_state.read());
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

/// POST /type - Type text
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

    let tab_id = match request.tab_id.or_else(|| {
        let browser_state = futures::executor::block_on(state.browser_state.read());
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

/// POST /evaluate - Execute JavaScript
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

    let tab_id = match request.tab_id.or_else(|| {
        let browser_state = futures::executor::block_on(state.browser_state.read());
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

/// GET /screenshot - Capture screenshot
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

    let tab_id = match query.tab_id.or_else(|| {
        let browser_state = futures::executor::block_on(state.browser_state.read());
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

/// POST /scroll - Scroll page
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

    let tab_id = match request.tab_id.or_else(|| {
        let browser_state = futures::executor::block_on(state.browser_state.read());
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

/// GET /dom/element - Find element
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

    let tab_id = match query.tab_id.or_else(|| {
        let browser_state = futures::executor::block_on(state.browser_state.read());
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

/// POST /api/toggle - Toggle API enabled state
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

/// GET /api/status - Get API status
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
        .route("/type", post(type_text))
        .route("/evaluate", post(evaluate))
        .route("/screenshot", get(screenshot))
        .route("/scroll", post(scroll))

        // DOM operations
        .route("/dom/element", get(find_element))

        // API management
        .route("/api/toggle", post(toggle_api))
        .route("/api/status", get(api_status))

        // WebSocket endpoint is handled separately

        .with_state(state)
}
