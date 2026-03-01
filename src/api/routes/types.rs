//! Shared request/response types for API routes.
//!
//! Defines the common data structures used across all route handlers,
//! including the generic ApiResponse wrapper, tab information structs,
//! and per-resource request/response types.

use serde::{Deserialize, Serialize};

use crate::api::server::TabState;

// ============================================================================
// Generic Response Wrapper
// ============================================================================

/// Standard API response wrapper for all endpoints.
///
/// Wraps successful data or error messages in a consistent JSON structure
/// with a `success` flag for easy client-side checking.
#[derive(Debug, Serialize)]
pub struct ApiResponse<T: Serialize> {
    pub success: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub data: Option<T>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

impl<T: Serialize> ApiResponse<T> {
    /// Create a successful response wrapping the given data.
    pub fn success(data: T) -> Self {
        Self {
            success: true,
            data: Some(data),
            error: None,
        }
    }

    /// Create an error response with no data payload.
    pub fn error(message: impl Into<String>) -> ApiResponse<()> {
        ApiResponse {
            success: false,
            data: None,
            error: Some(message.into()),
        }
    }
}

// ============================================================================
// Health / Status Types
// ============================================================================

/// Health check response indicating server liveness and API state.
#[derive(Debug, Serialize)]
pub struct HealthResponse {
    pub status: String,
    pub version: String,
    pub api_enabled: bool,
}

/// API toggle request to enable or disable the REST API at runtime.
#[derive(Debug, Deserialize)]
pub struct ApiToggleRequest {
    pub enabled: bool,
}

/// API status response with current enabled state and WebSocket client count.
#[derive(Debug, Serialize)]
pub struct ApiStatusResponse {
    pub enabled: bool,
    pub port: u16,
    pub connected_clients: usize,
}

// ============================================================================
// Tab Types
// ============================================================================

/// Tab information response containing current tab state and navigation capabilities.
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

/// List tabs response containing all open tabs and the active tab identifier.
#[derive(Debug, Serialize)]
pub struct TabsResponse {
    pub tabs: Vec<TabInfo>,
    pub active_tab_id: Option<String>,
}

/// Create new tab request with optional URL and activation preference.
#[derive(Debug, Deserialize)]
pub struct NewTabRequest {
    #[serde(default)]
    pub url: Option<String>,
    #[serde(default)]
    pub active: Option<bool>,
}

/// Create new tab response with the assigned tab ID and initial URL.
#[derive(Debug, Serialize)]
pub struct NewTabResponse {
    pub tab_id: String,
    pub url: String,
}

/// Close tab request identifying the target tab by its unique ID.
#[derive(Debug, Deserialize)]
pub struct CloseTabRequest {
    pub tab_id: String,
}

// ============================================================================
// Navigation Types
// ============================================================================

/// Navigate request targeting a specific tab or the active tab.
#[derive(Debug, Deserialize)]
pub struct NavigateRequest {
    #[serde(default)]
    pub tab_id: Option<String>,
    pub url: String,
}

// ============================================================================
// Input Types
// ============================================================================

/// Click request supporting both CSS selector targeting and raw coordinate clicking.
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

/// Type text request with optional element targeting and clear-before-type behaviour.
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

/// Scroll request supporting coordinate-based or selector-based scrolling with delta values.
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

// ============================================================================
// JavaScript Evaluation Types
// ============================================================================

/// Evaluate JavaScript request for executing arbitrary scripts inside a tab context.
#[derive(Debug, Deserialize)]
pub struct EvaluateRequest {
    #[serde(default)]
    pub tab_id: Option<String>,
    pub script: String,
    #[serde(default)]
    pub await_promise: Option<bool>,
}

/// Evaluate JavaScript response containing the serialised script return value.
#[derive(Debug, Serialize)]
pub struct EvaluateResponse {
    pub result: serde_json::Value,
}

// ============================================================================
// Screenshot Types
// ============================================================================

/// Screenshot query parameters for the GET /screenshot endpoint.
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

/// Screenshot response containing base64-encoded image data with dimensions.
#[derive(Debug, Serialize)]
pub struct ScreenshotResponse {
    pub data: String, // Base64 encoded image
    pub format: String,
    pub width: u32,
    pub height: u32,
}

// ============================================================================
// DOM Types
// ============================================================================

/// Find element query parameters for CSS selector-based DOM lookup.
#[derive(Debug, Deserialize)]
pub struct FindElementQuery {
    #[serde(default)]
    pub tab_id: Option<String>,
    pub selector: String,
    #[serde(default)]
    pub timeout: Option<u64>,
}

/// Element information response with layout, visibility, and content details.
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

/// Element bounding box with page-relative pixel coordinates and dimensions.
#[derive(Debug, Serialize)]
pub struct BoundingBox {
    pub x: f64,
    pub y: f64,
    pub width: f64,
    pub height: f64,
}
