//! Integration tests for the REST API endpoints
//!
//! Tests for health endpoint, tab endpoints, navigation endpoint,
//! and screenshot endpoint using axum's test utilities.

use axum::{
    body::Body,
    http::{Request, StatusCode, Method, header},
    Router,
};
use serde_json::{json, Value};
use tower::ServiceExt;

/// Mock implementations for API testing
mod mock {
    use std::collections::HashMap;
    use std::sync::Arc;
    use tokio::sync::RwLock;
    use axum::{
        extract::{Query, State},
        http::StatusCode,
        response::IntoResponse,
        routing::{get, post},
        Json, Router,
    };
    use serde::{Deserialize, Serialize};

    /// API response wrapper
    #[derive(Debug, Serialize, Deserialize)]
    pub struct ApiResponse<T> {
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
    #[derive(Debug, Serialize, Deserialize)]
    pub struct HealthResponse {
        pub status: String,
        pub version: String,
        pub api_enabled: bool,
    }

    /// Tab information
    #[derive(Debug, Serialize, Deserialize, Clone)]
    pub struct TabInfo {
        pub id: String,
        pub url: String,
        pub title: String,
        pub is_loading: bool,
        pub is_active: bool,
    }

    /// List tabs response
    #[derive(Debug, Serialize, Deserialize)]
    pub struct TabsResponse {
        pub tabs: Vec<TabInfo>,
        pub active_tab_id: Option<String>,
    }

    /// Create tab request
    #[derive(Debug, Deserialize)]
    pub struct NewTabRequest {
        #[serde(default)]
        pub url: Option<String>,
        #[serde(default)]
        pub active: Option<bool>,
    }

    /// Create tab response
    #[derive(Debug, Serialize, Deserialize)]
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

    /// Screenshot query params
    #[derive(Debug, Deserialize)]
    pub struct ScreenshotQuery {
        #[serde(default)]
        pub tab_id: Option<String>,
        #[serde(default = "default_format")]
        pub format: String,
        #[serde(default)]
        pub quality: Option<u8>,
        #[serde(default)]
        pub full_page: Option<bool>,
    }

    fn default_format() -> String {
        "png".to_string()
    }

    /// Screenshot response
    #[derive(Debug, Serialize, Deserialize)]
    pub struct ScreenshotResponse {
        pub data: String,
        pub format: String,
        pub width: u32,
        pub height: u32,
    }

    /// Internal tab state
    #[derive(Debug, Clone)]
    pub struct TabState {
        pub id: String,
        pub url: String,
        pub title: String,
        pub is_loading: bool,
    }

    /// Mock browser state
    #[derive(Debug, Default)]
    pub struct BrowserState {
        pub tabs: HashMap<String, TabState>,
        pub active_tab_id: Option<String>,
        pub next_tab_id: u32,
        pub api_enabled: bool,
    }

    impl BrowserState {
        pub fn new() -> Self {
            Self {
                tabs: HashMap::new(),
                active_tab_id: None,
                next_tab_id: 1,
                api_enabled: true,
            }
        }
    }

    /// Shared application state
    pub type AppState = Arc<RwLock<BrowserState>>;

    // ========================================================================
    // Route Handlers
    // ========================================================================

    /// GET /health
    pub async fn health_check(State(state): State<AppState>) -> impl IntoResponse {
        let state = state.read().await;
        Json(ApiResponse::success(HealthResponse {
            status: "healthy".to_string(),
            version: "1.0.0".to_string(),
            api_enabled: state.api_enabled,
        }))
    }

    /// GET /tabs
    pub async fn list_tabs(State(state): State<AppState>) -> impl IntoResponse {
        let state = state.read().await;

        if !state.api_enabled {
            return (
                StatusCode::SERVICE_UNAVAILABLE,
                Json(ApiResponse::<TabsResponse>::error("API is disabled")),
            ).into_response();
        }

        let tabs: Vec<TabInfo> = state
            .tabs
            .values()
            .map(|t| TabInfo {
                id: t.id.clone(),
                url: t.url.clone(),
                title: t.title.clone(),
                is_loading: t.is_loading,
                is_active: state.active_tab_id.as_ref() == Some(&t.id),
            })
            .collect();

        Json(ApiResponse::success(TabsResponse {
            tabs,
            active_tab_id: state.active_tab_id.clone(),
        })).into_response()
    }

    /// POST /tabs/new
    pub async fn create_tab(
        State(state): State<AppState>,
        Json(request): Json<NewTabRequest>,
    ) -> impl IntoResponse {
        let mut state = state.write().await;

        if !state.api_enabled {
            return (
                StatusCode::SERVICE_UNAVAILABLE,
                Json(ApiResponse::<NewTabResponse>::error("API is disabled")),
            ).into_response();
        }

        let tab_id = format!("tab-{}", state.next_tab_id);
        state.next_tab_id += 1;

        let url = request.url.unwrap_or_else(|| "about:blank".to_string());

        let tab = TabState {
            id: tab_id.clone(),
            url: url.clone(),
            title: "New Tab".to_string(),
            is_loading: true,
        };

        state.tabs.insert(tab_id.clone(), tab);

        if request.active.unwrap_or(true) || state.active_tab_id.is_none() {
            state.active_tab_id = Some(tab_id.clone());
        }

        Json(ApiResponse::success(NewTabResponse {
            tab_id,
            url,
        })).into_response()
    }

    /// POST /tabs/close
    pub async fn close_tab(
        State(state): State<AppState>,
        Json(request): Json<CloseTabRequest>,
    ) -> impl IntoResponse {
        let mut state = state.write().await;

        if !state.api_enabled {
            return (
                StatusCode::SERVICE_UNAVAILABLE,
                Json(ApiResponse::<()>::error("API is disabled")),
            ).into_response();
        }

        if state.tabs.remove(&request.tab_id).is_none() {
            return (
                StatusCode::NOT_FOUND,
                Json(ApiResponse::<()>::error("Tab not found")),
            ).into_response();
        }

        // Update active tab if needed
        if state.active_tab_id.as_ref() == Some(&request.tab_id) {
            state.active_tab_id = state.tabs.keys().next().cloned();
        }

        Json(ApiResponse::success(())).into_response()
    }

    /// POST /navigate
    pub async fn navigate(
        State(state): State<AppState>,
        Json(request): Json<NavigateRequest>,
    ) -> impl IntoResponse {
        let mut state = state.write().await;

        if !state.api_enabled {
            return (
                StatusCode::SERVICE_UNAVAILABLE,
                Json(ApiResponse::<()>::error("API is disabled")),
            ).into_response();
        }

        let tab_id = request.tab_id.or_else(|| state.active_tab_id.clone());

        let tab_id = match tab_id {
            Some(id) => id,
            None => {
                return (
                    StatusCode::BAD_REQUEST,
                    Json(ApiResponse::<()>::error("No tab specified and no active tab")),
                ).into_response();
            }
        };

        let tab = match state.tabs.get_mut(&tab_id) {
            Some(t) => t,
            None => {
                return (
                    StatusCode::NOT_FOUND,
                    Json(ApiResponse::<()>::error("Tab not found")),
                ).into_response();
            }
        };

        tab.url = request.url;
        tab.is_loading = true;

        Json(ApiResponse::success(())).into_response()
    }

    /// GET /screenshot
    pub async fn screenshot(
        State(state): State<AppState>,
        Query(query): Query<ScreenshotQuery>,
    ) -> impl IntoResponse {
        let state = state.read().await;

        if !state.api_enabled {
            return (
                StatusCode::SERVICE_UNAVAILABLE,
                Json(ApiResponse::<ScreenshotResponse>::error("API is disabled")),
            ).into_response();
        }

        let tab_id = query.tab_id.or_else(|| state.active_tab_id.clone());

        match tab_id {
            Some(id) if state.tabs.contains_key(&id) => {
                // Generate mock screenshot data
                let mock_data = base64::Engine::encode(
                    &base64::engine::general_purpose::STANDARD,
                    b"mock screenshot data",
                );

                Json(ApiResponse::success(ScreenshotResponse {
                    data: mock_data,
                    format: query.format,
                    width: 1920,
                    height: 1080,
                })).into_response()
            }
            Some(_) => (
                StatusCode::NOT_FOUND,
                Json(ApiResponse::<ScreenshotResponse>::error("Tab not found")),
            ).into_response(),
            None => (
                StatusCode::BAD_REQUEST,
                Json(ApiResponse::<ScreenshotResponse>::error("No tab specified and no active tab")),
            ).into_response(),
        }
    }

    /// Create the test router
    pub fn create_test_router() -> Router {
        let state: AppState = Arc::new(RwLock::new(BrowserState::new()));

        Router::new()
            .route("/health", get(health_check))
            .route("/tabs", get(list_tabs))
            .route("/tabs/new", post(create_tab))
            .route("/tabs/close", post(close_tab))
            .route("/navigate", post(navigate))
            .route("/screenshot", get(screenshot))
            .with_state(state)
    }

    /// Create a test router with pre-configured state
    pub fn create_test_router_with_state(browser_state: BrowserState) -> Router {
        let state: AppState = Arc::new(RwLock::new(browser_state));

        Router::new()
            .route("/health", get(health_check))
            .route("/tabs", get(list_tabs))
            .route("/tabs/new", post(create_tab))
            .route("/tabs/close", post(close_tab))
            .route("/navigate", post(navigate))
            .route("/screenshot", get(screenshot))
            .with_state(state)
    }
}

use mock::*;

// ============================================================================
// Test Helpers
// ============================================================================

async fn make_request(
    app: Router,
    method: Method,
    uri: &str,
    body: Option<Value>,
) -> (StatusCode, Value) {
    let body = match body {
        Some(v) => Body::from(serde_json::to_vec(&v).unwrap()),
        None => Body::empty(),
    };

    let request = Request::builder()
        .method(method)
        .uri(uri)
        .header(header::CONTENT_TYPE, "application/json")
        .body(body)
        .unwrap();

    let response = app.oneshot(request).await.unwrap();

    let status = response.status();
    let body_bytes = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    let body_json: Value = serde_json::from_slice(&body_bytes).unwrap_or(Value::Null);

    (status, body_json)
}

// ============================================================================
// Health Endpoint Tests
// ============================================================================

#[tokio::test]
async fn test_health_endpoint_returns_ok() {
    let app = create_test_router();

    let (status, body) = make_request(app, Method::GET, "/health", None).await;

    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["success"], true);
}

#[tokio::test]
async fn test_health_endpoint_returns_status_healthy() {
    let app = create_test_router();

    let (_, body) = make_request(app, Method::GET, "/health", None).await;

    assert_eq!(body["data"]["status"], "healthy");
}

#[tokio::test]
async fn test_health_endpoint_returns_version() {
    let app = create_test_router();

    let (_, body) = make_request(app, Method::GET, "/health", None).await;

    assert!(body["data"]["version"].is_string());
    assert!(!body["data"]["version"].as_str().unwrap().is_empty());
}

#[tokio::test]
async fn test_health_endpoint_returns_api_enabled() {
    let app = create_test_router();

    let (_, body) = make_request(app, Method::GET, "/health", None).await;

    assert_eq!(body["data"]["api_enabled"], true);
}

#[tokio::test]
async fn test_health_endpoint_when_api_disabled() {
    let mut state = BrowserState::new();
    state.api_enabled = false;
    let app = create_test_router_with_state(state);

    let (status, body) = make_request(app, Method::GET, "/health", None).await;

    // Health endpoint should still work even when API is disabled
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["data"]["api_enabled"], false);
}

// ============================================================================
// Tab Endpoint Tests
// ============================================================================

#[tokio::test]
async fn test_list_tabs_empty() {
    let app = create_test_router();

    let (status, body) = make_request(app, Method::GET, "/tabs", None).await;

    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["success"], true);
    assert!(body["data"]["tabs"].is_array());
    assert_eq!(body["data"]["tabs"].as_array().unwrap().len(), 0);
}

#[tokio::test]
async fn test_create_tab_default_url() {
    let app = create_test_router();

    let (status, body) = make_request(app, Method::POST, "/tabs/new", Some(json!({}))).await;

    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["success"], true);
    assert_eq!(body["data"]["url"], "about:blank");
    assert!(body["data"]["tab_id"].is_string());
}

#[tokio::test]
async fn test_create_tab_with_url() {
    let app = create_test_router();

    let (status, body) = make_request(
        app,
        Method::POST,
        "/tabs/new",
        Some(json!({
            "url": "https://example.com"
        })),
    ).await;

    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["data"]["url"], "https://example.com");
}

#[tokio::test]
async fn test_create_tab_increments_id() {
    let app = create_test_router();

    // Create first tab
    let (_, body1) = make_request(
        app.clone(),
        Method::POST,
        "/tabs/new",
        Some(json!({})),
    ).await;

    // Create second tab
    let (_, body2) = make_request(
        app,
        Method::POST,
        "/tabs/new",
        Some(json!({})),
    ).await;

    let id1 = body1["data"]["tab_id"].as_str().unwrap();
    let id2 = body2["data"]["tab_id"].as_str().unwrap();

    assert_ne!(id1, id2);
}

#[tokio::test]
async fn test_list_tabs_after_creation() {
    let app = create_test_router();

    // Create a tab
    make_request(
        app.clone(),
        Method::POST,
        "/tabs/new",
        Some(json!({"url": "https://example.com"})),
    ).await;

    // List tabs
    let (status, body) = make_request(app, Method::GET, "/tabs", None).await;

    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["data"]["tabs"].as_array().unwrap().len(), 1);
    assert_eq!(body["data"]["tabs"][0]["url"], "https://example.com");
}

#[tokio::test]
async fn test_first_tab_becomes_active() {
    let app = create_test_router();

    // Create a tab
    let (_, create_body) = make_request(
        app.clone(),
        Method::POST,
        "/tabs/new",
        Some(json!({})),
    ).await;

    let tab_id = create_body["data"]["tab_id"].as_str().unwrap();

    // List tabs
    let (_, list_body) = make_request(app, Method::GET, "/tabs", None).await;

    assert_eq!(list_body["data"]["active_tab_id"], tab_id);
}

#[tokio::test]
async fn test_close_tab() {
    let app = create_test_router();

    // Create a tab
    let (_, create_body) = make_request(
        app.clone(),
        Method::POST,
        "/tabs/new",
        Some(json!({})),
    ).await;

    let tab_id = create_body["data"]["tab_id"].as_str().unwrap();

    // Close the tab
    let (status, body) = make_request(
        app.clone(),
        Method::POST,
        "/tabs/close",
        Some(json!({"tab_id": tab_id})),
    ).await;

    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["success"], true);

    // Verify tab is gone
    let (_, list_body) = make_request(app, Method::GET, "/tabs", None).await;
    assert_eq!(list_body["data"]["tabs"].as_array().unwrap().len(), 0);
}

#[tokio::test]
async fn test_close_nonexistent_tab() {
    let app = create_test_router();

    let (status, body) = make_request(
        app,
        Method::POST,
        "/tabs/close",
        Some(json!({"tab_id": "nonexistent"})),
    ).await;

    assert_eq!(status, StatusCode::NOT_FOUND);
    assert_eq!(body["success"], false);
}

#[tokio::test]
async fn test_tabs_api_disabled() {
    let mut state = BrowserState::new();
    state.api_enabled = false;
    let app = create_test_router_with_state(state);

    let (status, body) = make_request(app, Method::GET, "/tabs", None).await;

    assert_eq!(status, StatusCode::SERVICE_UNAVAILABLE);
    assert_eq!(body["success"], false);
}

// ============================================================================
// Navigation Endpoint Tests
// ============================================================================

#[tokio::test]
async fn test_navigate_with_active_tab() {
    let app = create_test_router();

    // Create a tab
    make_request(
        app.clone(),
        Method::POST,
        "/tabs/new",
        Some(json!({"url": "about:blank"})),
    ).await;

    // Navigate (uses active tab)
    let (status, body) = make_request(
        app.clone(),
        Method::POST,
        "/navigate",
        Some(json!({"url": "https://rust-lang.org"})),
    ).await;

    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["success"], true);

    // Verify URL changed
    let (_, list_body) = make_request(app, Method::GET, "/tabs", None).await;
    assert_eq!(list_body["data"]["tabs"][0]["url"], "https://rust-lang.org");
}

#[tokio::test]
async fn test_navigate_with_explicit_tab_id() {
    let app = create_test_router();

    // Create a tab
    let (_, create_body) = make_request(
        app.clone(),
        Method::POST,
        "/tabs/new",
        Some(json!({})),
    ).await;

    let tab_id = create_body["data"]["tab_id"].as_str().unwrap();

    // Navigate with explicit tab_id
    let (status, _) = make_request(
        app,
        Method::POST,
        "/navigate",
        Some(json!({
            "tab_id": tab_id,
            "url": "https://github.com"
        })),
    ).await;

    assert_eq!(status, StatusCode::OK);
}

#[tokio::test]
async fn test_navigate_no_active_tab() {
    let app = create_test_router();

    // Try to navigate without any tabs
    let (status, body) = make_request(
        app,
        Method::POST,
        "/navigate",
        Some(json!({"url": "https://example.com"})),
    ).await;

    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert_eq!(body["success"], false);
}

#[tokio::test]
async fn test_navigate_nonexistent_tab() {
    let app = create_test_router();

    let (status, body) = make_request(
        app,
        Method::POST,
        "/navigate",
        Some(json!({
            "tab_id": "nonexistent",
            "url": "https://example.com"
        })),
    ).await;

    assert_eq!(status, StatusCode::NOT_FOUND);
    assert_eq!(body["success"], false);
}

#[tokio::test]
async fn test_navigate_sets_loading_state() {
    let app = create_test_router();

    // Create a tab
    make_request(
        app.clone(),
        Method::POST,
        "/tabs/new",
        Some(json!({})),
    ).await;

    // Navigate
    make_request(
        app.clone(),
        Method::POST,
        "/navigate",
        Some(json!({"url": "https://example.com"})),
    ).await;

    // Check is_loading
    let (_, list_body) = make_request(app, Method::GET, "/tabs", None).await;
    assert_eq!(list_body["data"]["tabs"][0]["is_loading"], true);
}

// ============================================================================
// Screenshot Endpoint Tests
// ============================================================================

#[tokio::test]
async fn test_screenshot_success() {
    let app = create_test_router();

    // Create a tab
    make_request(
        app.clone(),
        Method::POST,
        "/tabs/new",
        Some(json!({})),
    ).await;

    // Take screenshot
    let (status, body) = make_request(app, Method::GET, "/screenshot", None).await;

    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["success"], true);
    assert!(body["data"]["data"].is_string());
    assert_eq!(body["data"]["format"], "png");
    assert!(body["data"]["width"].is_number());
    assert!(body["data"]["height"].is_number());
}

#[tokio::test]
async fn test_screenshot_with_format() {
    let app = create_test_router();

    // Create a tab
    make_request(
        app.clone(),
        Method::POST,
        "/tabs/new",
        Some(json!({})),
    ).await;

    // Take screenshot with jpeg format
    let (status, body) = make_request(app, Method::GET, "/screenshot?format=jpeg", None).await;

    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["data"]["format"], "jpeg");
}

#[tokio::test]
async fn test_screenshot_with_explicit_tab_id() {
    let app = create_test_router();

    // Create a tab
    let (_, create_body) = make_request(
        app.clone(),
        Method::POST,
        "/tabs/new",
        Some(json!({})),
    ).await;

    let tab_id = create_body["data"]["tab_id"].as_str().unwrap();

    // Take screenshot with explicit tab_id
    let uri = format!("/screenshot?tab_id={}", tab_id);
    let (status, _) = make_request(app, Method::GET, &uri, None).await;

    assert_eq!(status, StatusCode::OK);
}

#[tokio::test]
async fn test_screenshot_no_active_tab() {
    let app = create_test_router();

    // Try screenshot without any tabs
    let (status, body) = make_request(app, Method::GET, "/screenshot", None).await;

    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert_eq!(body["success"], false);
}

#[tokio::test]
async fn test_screenshot_nonexistent_tab() {
    let app = create_test_router();

    let (status, body) = make_request(
        app,
        Method::GET,
        "/screenshot?tab_id=nonexistent",
        None,
    ).await;

    assert_eq!(status, StatusCode::NOT_FOUND);
    assert_eq!(body["success"], false);
}

#[tokio::test]
async fn test_screenshot_base64_data_valid() {
    let app = create_test_router();

    // Create a tab
    make_request(
        app.clone(),
        Method::POST,
        "/tabs/new",
        Some(json!({})),
    ).await;

    // Take screenshot
    let (_, body) = make_request(app, Method::GET, "/screenshot", None).await;

    let data = body["data"]["data"].as_str().unwrap();

    // Verify it's valid base64
    let decoded = base64::Engine::decode(
        &base64::engine::general_purpose::STANDARD,
        data,
    );
    assert!(decoded.is_ok());
}

// ============================================================================
// API Response Format Tests
// ============================================================================

#[tokio::test]
async fn test_success_response_format() {
    let app = create_test_router();

    let (_, body) = make_request(app, Method::GET, "/health", None).await;

    // Success response should have: success=true, data present, no error
    assert_eq!(body["success"], true);
    assert!(!body["data"].is_null());
    assert!(body.get("error").map_or(true, |e| e.is_null()));
}

#[tokio::test]
async fn test_error_response_format() {
    let app = create_test_router();

    let (_, body) = make_request(
        app,
        Method::POST,
        "/tabs/close",
        Some(json!({"tab_id": "nonexistent"})),
    ).await;

    // Error response should have: success=false, no data, error message
    assert_eq!(body["success"], false);
    assert!(body.get("data").map_or(true, |d| d.is_null()));
    assert!(body["error"].is_string());
}

// ============================================================================
// Integration Tests - Multiple Operations
// ============================================================================

#[tokio::test]
async fn test_full_tab_lifecycle() {
    let app = create_test_router();

    // 1. Verify no tabs initially
    let (_, list_body) = make_request(app.clone(), Method::GET, "/tabs", None).await;
    assert_eq!(list_body["data"]["tabs"].as_array().unwrap().len(), 0);

    // 2. Create a tab
    let (_, create_body) = make_request(
        app.clone(),
        Method::POST,
        "/tabs/new",
        Some(json!({"url": "https://example.com"})),
    ).await;
    let tab_id = create_body["data"]["tab_id"].as_str().unwrap().to_string();

    // 3. Verify tab exists
    let (_, list_body) = make_request(app.clone(), Method::GET, "/tabs", None).await;
    assert_eq!(list_body["data"]["tabs"].as_array().unwrap().len(), 1);

    // 4. Navigate to new URL
    make_request(
        app.clone(),
        Method::POST,
        "/navigate",
        Some(json!({
            "tab_id": &tab_id,
            "url": "https://rust-lang.org"
        })),
    ).await;

    // 5. Take screenshot
    let (status, _) = make_request(
        app.clone(),
        Method::GET,
        &format!("/screenshot?tab_id={}", tab_id),
        None,
    ).await;
    assert_eq!(status, StatusCode::OK);

    // 6. Close tab
    make_request(
        app.clone(),
        Method::POST,
        "/tabs/close",
        Some(json!({"tab_id": &tab_id})),
    ).await;

    // 7. Verify tab is gone
    let (_, list_body) = make_request(app, Method::GET, "/tabs", None).await;
    assert_eq!(list_body["data"]["tabs"].as_array().unwrap().len(), 0);
}

#[tokio::test]
async fn test_multiple_tabs_operations() {
    let app = create_test_router();

    // Create 3 tabs
    let mut tab_ids = Vec::new();
    for i in 0..3 {
        let (_, body) = make_request(
            app.clone(),
            Method::POST,
            "/tabs/new",
            Some(json!({"url": format!("https://site{}.com", i)})),
        ).await;
        tab_ids.push(body["data"]["tab_id"].as_str().unwrap().to_string());
    }

    // List all tabs
    let (_, list_body) = make_request(app.clone(), Method::GET, "/tabs", None).await;
    assert_eq!(list_body["data"]["tabs"].as_array().unwrap().len(), 3);

    // Close middle tab
    make_request(
        app.clone(),
        Method::POST,
        "/tabs/close",
        Some(json!({"tab_id": &tab_ids[1]})),
    ).await;

    // Verify 2 tabs remain
    let (_, list_body) = make_request(app, Method::GET, "/tabs", None).await;
    assert_eq!(list_body["data"]["tabs"].as_array().unwrap().len(), 2);
}
