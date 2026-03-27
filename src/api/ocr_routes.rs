//! OCR REST API endpoints
//!
//! Provides endpoints for listing available OCR engines and running
//! OCR recognition on browser tab screenshots.

use axum::{
    extract::State,
    http::StatusCode,
    response::IntoResponse,
    routing::{get, post},
    Json, Router,
};
use serde::{Deserialize, Serialize};
use tracing::error;

use crate::api::ipc::{IpcCommand, IpcMessage};
use crate::api::routes::ApiResponse;
use crate::api::server::AppState;

// ============================================================================
// Request/Response Structs
// ============================================================================

/// Information about a single OCR engine and its availability.
#[derive(Debug, Serialize)]
pub struct OcrEngineResponse {
    /// Engine identifier (e.g. "tesseract", "paddleocr", "surya").
    pub name: String,
    /// Whether the engine's runtime dependencies are available.
    pub available: bool,
    /// Engine version string, if available.
    pub version: Option<String>,
}

/// Request body for POST /ocr/run.
#[derive(Debug, Deserialize)]
pub struct OcrRunRequest {
    /// Tab ID to screenshot. Required -- triggers automatic screenshot capture.
    #[serde(default)]
    pub tab_id: Option<String>,
    /// Which engines to use (empty or absent = all available engines).
    #[serde(default)]
    pub engines: Option<Vec<String>>,
    /// Optional region of interest to crop before OCR.
    #[serde(default)]
    pub region: Option<OcrRegionRequest>,
}

/// Rectangular region of interest for cropping before OCR.
#[derive(Debug, Deserialize)]
pub struct OcrRegionRequest {
    pub x: u32,
    pub y: u32,
    pub w: u32,
    pub h: u32,
}

/// Response body for POST /ocr/run.
#[derive(Debug, Serialize)]
pub struct OcrRunResponse {
    /// Per-engine OCR results.
    pub results: Vec<OcrEngineResult>,
}

/// OCR result from a single engine.
#[derive(Debug, Serialize)]
pub struct OcrEngineResult {
    /// Engine identifier.
    pub engine: String,
    /// Concatenated full text from all regions.
    pub full_text: String,
    /// Per-region recognition results with bounding boxes.
    pub results: Vec<OcrTextRegion>,
    /// Wall-clock duration of the recognition in milliseconds.
    pub duration_ms: u64,
    /// Error message if the engine failed (other fields may be empty).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

/// A single recognized text region with bounding box.
#[derive(Debug, Serialize)]
pub struct OcrTextRegion {
    pub text: String,
    pub confidence: f32,
    pub x: f32,
    pub y: f32,
    pub w: f32,
    pub h: f32,
}

// ============================================================================
// Route Handlers
// ============================================================================

/// GET /ocr/engines -- List all OCR engines with availability status.
async fn list_ocr_engines() -> impl IntoResponse {
    let engines = crate::ocr::engine_info();
    let data: Vec<OcrEngineResponse> = engines
        .into_iter()
        .map(|e| OcrEngineResponse {
            name: e.name,
            available: e.available,
            version: e.version,
        })
        .collect();
    Json(ApiResponse::success(data))
}

/// POST /ocr/run -- Run OCR on a tab screenshot.
///
/// 1. Captures a screenshot via IPC (tab_id required).
/// 2. Filters engines based on the `engines` parameter.
/// 3. Runs each engine in parallel via `spawn_blocking`.
/// 4. Collects results; engine errors populate the `error` field instead of
///    failing the entire request.
async fn run_ocr(
    State(state): State<AppState>,
    Json(request): Json<OcrRunRequest>,
) -> impl IntoResponse {
    if !state.is_enabled().await {
        return (
            StatusCode::SERVICE_UNAVAILABLE,
            Json(ApiResponse::<OcrRunResponse>::error("API is disabled")),
        )
            .into_response();
    }

    // tab_id is required
    let tab_id = match request.tab_id.or_else(|| {
        let browser_state = futures::executor::block_on(state.browser_state.read());
        browser_state.active_tab_id.clone()
    }) {
        Some(id) => id,
        None => {
            return (
                StatusCode::BAD_REQUEST,
                Json(ApiResponse::<OcrRunResponse>::error(
                    "tab_id is required (no active tab available)",
                )),
            )
                .into_response();
        }
    };

    // 1. Capture screenshot via IPC
    let command = IpcCommand::CaptureScreenshot {
        tab_id,
        format: "png".to_string(),
        quality: None,
        full_page: false,
        selector: None,
        clip_x: None,
        clip_y: None,
        clip_width: None,
        clip_height: None,
        clip_scale: None,
    };

    let png_data = match state
        .ipc_channel
        .send_command(IpcMessage::Command(command))
        .await
    {
        Ok(response) => {
            if !response.success {
                return (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    Json(ApiResponse::<OcrRunResponse>::error(
                        response
                            .error
                            .unwrap_or_else(|| "Screenshot failed".to_string()),
                    )),
                )
                    .into_response();
            }
            match response
                .data
                .as_ref()
                .and_then(|d| d.get("screenshot"))
                .and_then(|v| v.as_str())
            {
                Some(b64) => match base64::Engine::decode(
                    &base64::engine::general_purpose::STANDARD,
                    b64,
                ) {
                    Ok(bytes) => bytes,
                    Err(e) => {
                        error!("Base64 decode failed: {}", e);
                        return (
                            StatusCode::INTERNAL_SERVER_ERROR,
                            Json(ApiResponse::<OcrRunResponse>::error(format!(
                                "Failed to decode screenshot: {}",
                                e
                            ))),
                        )
                            .into_response();
                    }
                },
                None => {
                    return (
                        StatusCode::INTERNAL_SERVER_ERROR,
                        Json(ApiResponse::<OcrRunResponse>::error(
                            "Invalid screenshot response (missing data)",
                        )),
                    )
                        .into_response();
                }
            }
        }
        Err(e) => {
            error!("Failed to capture screenshot for OCR: {}", e);
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ApiResponse::<OcrRunResponse>::error(format!(
                    "Failed to capture screenshot: {}",
                    e
                ))),
            )
                .into_response();
        }
    };

    // 2. Build OCR region if specified
    let ocr_region = request.region.map(|r| crate::ocr::OcrRegion {
        x: r.x,
        y: r.y,
        w: r.w,
        h: r.h,
    });

    // 3. Get engines and filter
    let all_engines = crate::ocr::all_engines();
    let requested_names = request.engines;

    let engines: Vec<_> = all_engines
        .into_iter()
        .filter(|e| {
            if let Some(ref names) = requested_names {
                if names.is_empty() {
                    e.is_available()
                } else {
                    names.iter().any(|n| n == e.name())
                }
            } else {
                e.is_available()
            }
        })
        .collect();

    if engines.is_empty() {
        return Json(ApiResponse::success(OcrRunResponse {
            results: vec![],
        }))
        .into_response();
    }

    // 4. Run each engine in parallel via spawn_blocking
    let mut handles = Vec::new();
    for engine in engines {
        let png = png_data.clone();
        let region = ocr_region.clone();
        let handle = tokio::task::spawn_blocking(move || {
            let name = engine.name().to_string();
            match engine.recognize(&png, region) {
                Ok(resp) => OcrEngineResult {
                    engine: resp.engine,
                    full_text: resp.full_text,
                    results: resp
                        .results
                        .into_iter()
                        .map(|r| OcrTextRegion {
                            text: r.text,
                            confidence: r.confidence,
                            x: r.x,
                            y: r.y,
                            w: r.w,
                            h: r.h,
                        })
                        .collect(),
                    duration_ms: resp.duration_ms,
                    error: None,
                },
                Err(e) => OcrEngineResult {
                    engine: name,
                    full_text: String::new(),
                    results: vec![],
                    duration_ms: 0,
                    error: Some(e),
                },
            }
        });
        handles.push(handle);
    }

    // 5. Collect results
    let mut results = Vec::new();
    for handle in handles {
        match handle.await {
            Ok(result) => results.push(result),
            Err(e) => {
                error!("OCR task panicked: {}", e);
                results.push(OcrEngineResult {
                    engine: "unknown".to_string(),
                    full_text: String::new(),
                    results: vec![],
                    duration_ms: 0,
                    error: Some(format!("Task panicked: {}", e)),
                });
            }
        }
    }

    Json(ApiResponse::success(OcrRunResponse { results })).into_response()
}

// ============================================================================
// Router
// ============================================================================

/// Creates the OCR sub-router with all OCR endpoints.
pub fn ocr_routes() -> Router<AppState> {
    Router::new()
        .route("/ocr/engines", get(list_ocr_engines))
        .route("/ocr/run", post(run_ocr))
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_list_ocr_engines_returns_success() {
        // Directly call engine_info and verify the response structure
        let engines = crate::ocr::engine_info();
        assert_eq!(engines.len(), 3);
        let names: Vec<&str> = engines.iter().map(|e| e.name.as_str()).collect();
        assert!(names.contains(&"tesseract"));
        assert!(names.contains(&"paddleocr"));
        assert!(names.contains(&"surya"));
    }

    #[test]
    fn test_ocr_run_request_deserialization() {
        let json = r#"{
            "tab_id": "tab_1",
            "engines": ["tesseract", "surya"],
            "region": { "x": 10, "y": 20, "w": 300, "h": 100 }
        }"#;
        let req: OcrRunRequest = serde_json::from_str(json).unwrap();
        assert_eq!(req.tab_id.unwrap(), "tab_1");
        let engines = req.engines.unwrap();
        assert_eq!(engines.len(), 2);
        assert_eq!(engines[0], "tesseract");
        assert_eq!(engines[1], "surya");
        let region = req.region.unwrap();
        assert_eq!(region.x, 10);
        assert_eq!(region.y, 20);
        assert_eq!(region.w, 300);
        assert_eq!(region.h, 100);
    }

    #[test]
    fn test_ocr_run_request_minimal_deserialization() {
        let json = r#"{ "tab_id": "tab_5" }"#;
        let req: OcrRunRequest = serde_json::from_str(json).unwrap();
        assert_eq!(req.tab_id.unwrap(), "tab_5");
        assert!(req.engines.is_none());
        assert!(req.region.is_none());
    }

    #[test]
    fn test_ocr_region_request_deserialization() {
        let json = r#"{ "x": 0, "y": 0, "w": 1920, "h": 1080 }"#;
        let region: OcrRegionRequest = serde_json::from_str(json).unwrap();
        assert_eq!(region.x, 0);
        assert_eq!(region.y, 0);
        assert_eq!(region.w, 1920);
        assert_eq!(region.h, 1080);
    }

    #[test]
    fn test_ocr_engine_result_serialization() {
        let result = OcrEngineResult {
            engine: "tesseract".to_string(),
            full_text: "Hello World".to_string(),
            results: vec![OcrTextRegion {
                text: "Hello World".to_string(),
                confidence: 0.95,
                x: 10.0,
                y: 20.0,
                w: 200.0,
                h: 30.0,
            }],
            duration_ms: 150,
            error: None,
        };
        let json = serde_json::to_string(&result).unwrap();
        assert!(json.contains("Hello World"));
        assert!(json.contains("0.95"));
        // error field should be skipped when None
        assert!(!json.contains("error"));
    }

    #[test]
    fn test_ocr_engine_result_with_error_serialization() {
        let result = OcrEngineResult {
            engine: "paddleocr".to_string(),
            full_text: String::new(),
            results: vec![],
            duration_ms: 0,
            error: Some("Engine not available".to_string()),
        };
        let json = serde_json::to_string(&result).unwrap();
        assert!(json.contains("Engine not available"));
        assert!(json.contains("\"error\""));
    }
}
