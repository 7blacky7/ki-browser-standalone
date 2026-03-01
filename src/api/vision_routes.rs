//! Vision overlay API endpoints for KI agent screenshot annotation.
//!
//! Provides HTTP endpoints that combine DOM snapshot data with screenshot
//! capture to produce annotated images with numbered interactive element
//! labels, enabling vision-based AI agents to reference page elements by number.

use axum::{
    extract::{Query, State},
    http::{header, StatusCode},
    response::IntoResponse,
    routing::get,
    Json, Router,
};
use base64::{engine::general_purpose::STANDARD as BASE64, Engine as _};
use serde::{Deserialize, Serialize};
use tracing::error;

use crate::api::routes::ApiResponse;
use crate::api::server::AppState;
use crate::api::ipc::{IpcCommand, IpcMessage};
use crate::browser::screenshot::ScreenshotFormat;
use crate::browser::vision::VisionLabel;

// -------------------------------------------------------------------------
// Request / Response types
// -------------------------------------------------------------------------

/// Query parameters for GET /vision/annotated.
#[derive(Debug, Deserialize)]
pub struct AnnotatedQuery {
    /// Tab ID to capture. Falls back to active tab if omitted.
    pub tab_id: Option<String>,

    /// Output image format: "png" (default) or "jpeg".
    #[serde(default = "default_format")]
    pub format: String,
}

fn default_format() -> String {
    "png".to_string()
}

/// Query parameters for GET /vision/labels.
#[derive(Debug, Deserialize)]
pub struct LabelsQuery {
    /// Tab ID to query. Falls back to active tab if omitted.
    pub tab_id: Option<String>,
}

/// JSON response for the /vision/labels endpoint.
#[derive(Debug, Serialize)]
pub struct LabelsResponse {
    /// Number of labels found.
    pub count: usize,
    /// Array of vision labels with bounding boxes and metadata.
    pub labels: Vec<VisionLabel>,
}

// -------------------------------------------------------------------------
// Helper
// -------------------------------------------------------------------------

fn parse_format(s: &str) -> ScreenshotFormat {
    match s.to_lowercase().as_str() {
        "jpeg" | "jpg" => ScreenshotFormat::Jpeg,
        "webp" => ScreenshotFormat::WebP,
        _ => ScreenshotFormat::Png,
    }
}

/// Resolves the tab ID from the query parameter or falls back to the active tab.
async fn resolve_tab_id(state: &AppState, requested: Option<String>) -> Option<String> {
    if let Some(id) = requested {
        return Some(id);
    }
    let browser_state = state.browser_state.read().await;
    browser_state.active_tab_id.clone()
}

// -------------------------------------------------------------------------
// Handlers
// -------------------------------------------------------------------------

/// GET /vision/annotated — Returns a screenshot with numbered label overlays.
///
/// Sends an IPC command to the browser engine requesting:
/// 1. A DOM snapshot of the active page
/// 2. A screenshot of the current viewport
///
/// Then runs vision label generation + annotation on the server side and
/// returns the raw image bytes with the appropriate Content-Type header.
async fn vision_annotated(
    State(state): State<AppState>,
    Query(query): Query<AnnotatedQuery>,
) -> impl IntoResponse {
    let tab_id = match resolve_tab_id(&state, query.tab_id).await {
        Some(id) => id,
        None => {
            return (
                StatusCode::BAD_REQUEST,
                Json(ApiResponse::<()>::error("No tab specified and no active tab")),
            )
                .into_response();
        }
    };

    let format = parse_format(&query.format);

    // Request annotated vision screenshot via IPC
    let command = IpcCommand::VisionAnnotated {
        tab_id,
        format: format_to_string(format),
    };

    match state
        .ipc_channel
        .send_command(IpcMessage::Command(command))
        .await
    {
        Ok(response) => {
            if response.success {
                if let Some(data) = response.data {
                    // The IPC response should contain base64-encoded image bytes
                    if let Some(image_b64) = data.get("image").and_then(|v| v.as_str()) {
                        match BASE64.decode(image_b64) {
                            Ok(image_bytes) => {
                                return (
                                    StatusCode::OK,
                                    [(header::CONTENT_TYPE, format.mime_type())],
                                    image_bytes,
                                )
                                    .into_response();
                            }
                            Err(e) => {
                                error!("Failed to decode vision image: {}", e);
                                return (
                                    StatusCode::INTERNAL_SERVER_ERROR,
                                    Json(ApiResponse::<()>::error(format!(
                                        "Failed to decode image: {}",
                                        e
                                    ))),
                                )
                                    .into_response();
                            }
                        }
                    }
                }
                (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    Json(ApiResponse::<()>::error("Invalid vision response: missing image data")),
                )
                    .into_response()
            } else {
                (
                    StatusCode::BAD_REQUEST,
                    Json(ApiResponse::<()>::error(
                        response
                            .error
                            .unwrap_or_else(|| "Vision annotation failed".to_string()),
                    )),
                )
                    .into_response()
            }
        }
        Err(e) => {
            error!("Vision annotated IPC failed: {}", e);
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ApiResponse::<()>::error(format!(
                    "Vision annotation failed: {}",
                    e
                ))),
            )
                .into_response()
        }
    }
}

/// GET /vision/labels — Returns JSON array of vision labels without the screenshot.
///
/// Useful when the AI agent already has a screenshot and only needs the label
/// metadata (bounding boxes, roles, selector hints).
async fn vision_labels(
    State(state): State<AppState>,
    Query(query): Query<LabelsQuery>,
) -> impl IntoResponse {
    let tab_id = match resolve_tab_id(&state, query.tab_id).await {
        Some(id) => id,
        None => {
            return (
                StatusCode::BAD_REQUEST,
                Json(ApiResponse::<LabelsResponse>::error(
                    "No tab specified and no active tab",
                )),
            )
                .into_response();
        }
    };

    let command = IpcCommand::VisionLabels { tab_id };

    match state
        .ipc_channel
        .send_command(IpcMessage::Command(command))
        .await
    {
        Ok(response) => {
            if response.success {
                if let Some(data) = response.data {
                    if let Some(labels_val) = data.get("labels") {
                        match serde_json::from_value::<Vec<VisionLabel>>(labels_val.clone()) {
                            Ok(labels) => {
                                let count = labels.len();
                                return Json(ApiResponse::success(LabelsResponse {
                                    count,
                                    labels,
                                }))
                                .into_response();
                            }
                            Err(e) => {
                                error!("Failed to parse vision labels: {}", e);
                                return (
                                    StatusCode::INTERNAL_SERVER_ERROR,
                                    Json(ApiResponse::<LabelsResponse>::error(format!(
                                        "Failed to parse labels: {}",
                                        e
                                    ))),
                                )
                                    .into_response();
                            }
                        }
                    }
                }
                (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    Json(ApiResponse::<LabelsResponse>::error(
                        "Invalid vision response: missing labels",
                    )),
                )
                    .into_response()
            } else {
                (
                    StatusCode::BAD_REQUEST,
                    Json(ApiResponse::<LabelsResponse>::error(
                        response
                            .error
                            .unwrap_or_else(|| "Vision labels failed".to_string()),
                    )),
                )
                    .into_response()
            }
        }
        Err(e) => {
            error!("Vision labels IPC failed: {}", e);
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ApiResponse::<LabelsResponse>::error(format!(
                    "Vision labels failed: {}",
                    e
                ))),
            )
                .into_response()
        }
    }
}

fn format_to_string(format: ScreenshotFormat) -> String {
    match format {
        ScreenshotFormat::Png => "png".to_string(),
        ScreenshotFormat::Jpeg => "jpeg".to_string(),
        ScreenshotFormat::WebP => "webp".to_string(),
    }
}

// -------------------------------------------------------------------------
// Router
// -------------------------------------------------------------------------

/// Creates the vision overlay sub-router with /vision/* endpoints.
pub fn vision_routes() -> Router<AppState> {
    Router::new()
        .route("/vision/annotated", get(vision_annotated))
        .route("/vision/labels", get(vision_labels))
}

// -------------------------------------------------------------------------
// Tests
// -------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_format_png() {
        assert_eq!(parse_format("png"), ScreenshotFormat::Png);
        assert_eq!(parse_format("PNG"), ScreenshotFormat::Png);
    }

    #[test]
    fn test_parse_format_jpeg() {
        assert_eq!(parse_format("jpeg"), ScreenshotFormat::Jpeg);
        assert_eq!(parse_format("jpg"), ScreenshotFormat::Jpeg);
        assert_eq!(parse_format("JPEG"), ScreenshotFormat::Jpeg);
    }

    #[test]
    fn test_parse_format_default() {
        assert_eq!(parse_format("unknown"), ScreenshotFormat::Png);
        assert_eq!(parse_format(""), ScreenshotFormat::Png);
    }

    #[test]
    fn test_format_to_string() {
        assert_eq!(format_to_string(ScreenshotFormat::Png), "png");
        assert_eq!(format_to_string(ScreenshotFormat::Jpeg), "jpeg");
        assert_eq!(format_to_string(ScreenshotFormat::WebP), "webp");
    }

    #[test]
    fn test_labels_response_serialization() {
        use crate::browser::dom::BoundingBox;

        let resp = LabelsResponse {
            count: 1,
            labels: vec![VisionLabel {
                id: 1,
                bbox: BoundingBox::new(10.0, 20.0, 100.0, 30.0),
                role: "button".to_string(),
                name: "Submit".to_string(),
                text_hint: Some("Submit".to_string()),
                selector_hint: "#submit-btn".to_string(),
            }],
        };

        let json = serde_json::to_string(&resp).unwrap();
        assert!(json.contains("\"count\":1"));
        assert!(json.contains("\"role\":\"button\""));
        assert!(json.contains("\"selector_hint\":\"#submit-btn\""));
    }
}
