//! Request and response types for the ki-browser REST API.
//!
//! Contains all DTOs (Data Transfer Objects) used by API route handlers,
//! including request bodies, query parameters, and response structures.

use serde::{Deserialize, Serialize};
use utoipa::{IntoParams, ToSchema};

use crate::api::server::TabState;

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
    /// Frame-ID fuer iFrame-Isolation. Aendert sich bei Navigation — /frames muss danach neu
    /// abgerufen werden. Unterstuetzt: CDP-native IDs, 'main', 'frame-N' Index, iframe name/id
    /// Attribute.
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
    /// Frame-ID fuer iFrame-Isolation. Aendert sich bei Navigation — /frames muss danach neu
    /// abgerufen werden. Unterstuetzt: CDP-native IDs, 'main', 'frame-N' Index, iframe name/id
    /// Attribute.
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
    /// Frame-ID fuer iFrame-Isolation. Aendert sich bei Navigation — /frames muss danach neu
    /// abgerufen werden. Unterstuetzt: CDP-native IDs, 'main', 'frame-N' Index, iframe name/id
    /// Attribute.
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
    /// If true, return raw binary image instead of JSON with base64 data
    #[serde(default)]
    pub raw: Option<bool>,
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
    /// Frame-ID fuer iFrame-Isolation. Aendert sich bei Navigation — /frames muss danach neu
    /// abgerufen werden. Unterstuetzt: CDP-native IDs, 'main', 'frame-N' Index, iframe name/id
    /// Attribute.
    #[serde(default)]
    pub frame_id: Option<String>,
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
