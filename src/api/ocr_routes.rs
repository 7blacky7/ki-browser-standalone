//! OCR REST API endpoints
//!
//! Provides endpoints for listing available OCR engines and running
//! OCR recognition on browser tab screenshots.

use std::collections::HashMap;

use axum::{
    extract::{Path, State},
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

/// Information about a single OCR engine: availability, runtime enabled state
/// and self-documenting metadata.
#[derive(Debug, Serialize)]
pub struct OcrEngineResponse {
    /// Engine identifier (e.g. "tesseract", "paddleocr", "surya").
    pub name: String,
    /// Whether the engine's runtime dependencies are available (installed).
    pub available: bool,
    /// Whether the engine is globally enabled at runtime (toggleable).
    pub enabled: bool,
    /// Engine version string, if available.
    pub version: Option<String>,
    /// Human-readable description of what the engine does.
    pub description: String,
    /// Short hint describing the workload this engine is best suited for.
    pub best_for: String,
    /// Relative speed class: "fast", "medium" or "slow".
    pub speed: String,
    /// Whether the engine uses GPU acceleration.
    pub gpu_accelerated: bool,
    /// Languages / language coverage the engine supports.
    pub languages: String,
}

/// Response wrapper for GET /ocr/engines: the engine list plus a usage hint.
#[derive(Debug, Serialize)]
pub struct OcrEnginesResponse {
    /// All registered engines with availability, enabled state and metadata.
    pub engines: Vec<OcrEngineResponse>,
    /// Plain-text hint explaining how to enable/disable engines.
    pub usage: String,
}

/// Optional request body for enable/disable endpoints.
#[derive(Debug, Deserialize, Default)]
pub struct OcrToggleRequest {
    /// When set, the toggle applies only to this tab (per-tab override).
    /// When absent, the global default for the engine is changed.
    #[serde(default)]
    pub tab_id: Option<String>,
}

/// Response for enable/disable endpoints, with a plain-text explanation.
#[derive(Debug, Serialize)]
pub struct OcrToggleResponse {
    /// The engine that was toggled.
    pub engine: String,
    /// New enabled state that was applied.
    pub enabled: bool,
    /// The tab the change applied to, or `null` if it changed the global default.
    pub tab_id: Option<String>,
    /// Whether the engine is actually installed/available right now.
    pub available: bool,
    /// Plain-text explanation of what happened and the resulting state.
    pub explanation: String,
}

/// Response for GET /ocr/config: the full runtime config plus an explanation
/// of the resolution order.
#[derive(Debug, Serialize)]
pub struct OcrConfigResponse {
    /// Global enable/disable state per engine name.
    pub global: HashMap<String, bool>,
    /// Per-tab overrides: `tab_id -> (engine -> enabled)`.
    pub per_tab: HashMap<String, HashMap<String, bool>>,
    /// Plain-text explanation of how the effective engine set is resolved.
    pub resolution_order: String,
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
    /// Plain-text hints, e.g. engines enabled via toggle but not installed.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub notes: Vec<String>,
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

/// GET /ocr/engines -- List all OCR engines with availability, runtime enabled
/// state and self-documenting metadata.
async fn list_ocr_engines(State(state): State<AppState>) -> impl IntoResponse {
    let global = { state.ocr_config.read().await.global.clone() };
    let engines = crate::ocr::engine_info();
    let data: Vec<OcrEngineResponse> = engines
        .into_iter()
        .map(|e| {
            let enabled = global.get(&e.name).copied().unwrap_or(true);
            OcrEngineResponse {
                available: e.available,
                enabled,
                version: e.version,
                description: e.description,
                best_for: e.best_for,
                speed: e.speed,
                gpu_accelerated: e.gpu_accelerated,
                languages: e.languages,
                name: e.name,
            }
        })
        .collect();
    Json(ApiResponse::success(OcrEnginesResponse {
        engines: data,
        usage: "Toggle engines at runtime (no restart): POST /ocr/engines/{name}/enable or \
                /ocr/engines/{name}/disable. Send an empty body to change the global default, \
                or {\"tab_id\":\"<id>\"} to override only that tab. Inspect the full config via \
                GET /ocr/config. When running OCR, explicit `engines` in POST /ocr/run always wins."
            .to_string(),
    }))
}

/// Validates an engine name against the registered catalog. Returns the
/// availability flag on success, or an error message listing valid names.
fn validate_engine(name: &str) -> Result<bool, String> {
    let engines = crate::ocr::engine_info();
    match engines.iter().find(|e| e.name == name) {
        Some(e) => Ok(e.available),
        None => {
            let valid: Vec<String> = engines.into_iter().map(|e| e.name).collect();
            Err(format!(
                "Unknown engine '{}'. Valid engines: {}",
                name,
                valid.join(", ")
            ))
        }
    }
}

/// Shared implementation for enable/disable endpoints.
async fn toggle_engine(
    state: AppState,
    name: String,
    req: OcrToggleRequest,
    enabled: bool,
) -> impl IntoResponse {
    let available = match validate_engine(&name) {
        Ok(a) => a,
        Err(msg) => {
            return (
                StatusCode::BAD_REQUEST,
                Json(ApiResponse::<OcrToggleResponse>::error(msg)),
            )
                .into_response();
        }
    };

    {
        let mut cfg = state.ocr_config.write().await;
        match &req.tab_id {
            Some(tab_id) => {
                cfg.per_tab
                    .entry(tab_id.clone())
                    .or_default()
                    .insert(name.clone(), enabled);
            }
            None => {
                cfg.global.insert(name.clone(), enabled);
            }
        }
    }

    let verb = if enabled { "enabled" } else { "disabled" };
    let scope = match &req.tab_id {
        Some(tab_id) => format!("for tab '{}'", tab_id),
        None => "globally".to_string(),
    };
    let mut explanation = format!("Engine '{}' is now {} {}.", name, verb, scope);
    if enabled && !available {
        explanation.push_str(&format!(
            " Note: engine '{}' is enabled but not installed/available, so it will be skipped \
             until the dependency is deployed.",
            name
        ));
    }

    Json(ApiResponse::success(OcrToggleResponse {
        engine: name,
        enabled,
        tab_id: req.tab_id,
        available,
        explanation,
    }))
    .into_response()
}

/// POST /ocr/engines/{name}/enable -- Enable an engine (global or per-tab).
async fn enable_engine(
    State(state): State<AppState>,
    Path(name): Path<String>,
    body: Option<Json<OcrToggleRequest>>,
) -> impl IntoResponse {
    let req = body.map(|Json(b)| b).unwrap_or_default();
    toggle_engine(state, name, req, true).await
}

/// POST /ocr/engines/{name}/disable -- Disable an engine (global or per-tab).
async fn disable_engine(
    State(state): State<AppState>,
    Path(name): Path<String>,
    body: Option<Json<OcrToggleRequest>>,
) -> impl IntoResponse {
    let req = body.map(|Json(b)| b).unwrap_or_default();
    toggle_engine(state, name, req, false).await
}

/// GET /ocr/config -- Show the full runtime config and explain resolution order.
async fn get_ocr_config(State(state): State<AppState>) -> impl IntoResponse {
    let cfg = state.ocr_config.read().await;
    Json(ApiResponse::success(OcrConfigResponse {
        global: cfg.global.clone(),
        per_tab: cfg.per_tab.clone(),
        resolution_order: "Effective engines for a run are resolved in this order: \
            1) explicit `engines` in POST /ocr/run (if non-empty) always wins; \
            2) otherwise, if the tab has per-tab overrides, the engines enabled there are used; \
            3) otherwise the globally enabled engines are used; \
            4) as a final fallback all installed/available engines run. \
            In every case only engines that are actually installed/available are executed; an \
            engine enabled via toggle but not installed is reported as a hint and skipped."
            .to_string(),
    }))
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
        tab_id: tab_id.clone(),
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

    // 3. Resolve the desired engine set.
    //
    // Priority:
    //   1) explicit request.engines (when non-empty) always wins
    //   2) per-tab overrides (only the engines enabled there)
    //   3) globally enabled engines
    //   4) fallback: all installed/available engines
    //
    // Only installed/available engines are actually executed. Engines that are
    // enabled via toggle but not installed are reported in `notes` and skipped.
    let all_engines = crate::ocr::all_engines();
    let requested_names = request.engines;
    let mut notes: Vec<String> = Vec::new();

    // The set of engine names we *want* (None = use availability fallback directly).
    let desired: Option<Vec<String>> = if let Some(ref names) = requested_names {
        if names.is_empty() {
            None
        } else {
            Some(names.clone())
        }
    } else {
        let cfg = state.ocr_config.read().await;
        let per_tab = cfg.per_tab.get(&tab_id).and_then(|map| {
            let enabled: Vec<String> = map
                .iter()
                .filter(|(_, &on)| on)
                .map(|(n, _)| n.clone())
                .collect();
            if enabled.is_empty() {
                None
            } else {
                Some(enabled)
            }
        });
        match per_tab {
            Some(list) => Some(list),
            None => {
                let global: Vec<String> = cfg
                    .global
                    .iter()
                    .filter(|(_, &on)| on)
                    .map(|(n, _)| n.clone())
                    .collect();
                if global.is_empty() {
                    None
                } else {
                    Some(global)
                }
            }
        }
    };

    // Emit hints for desired-but-not-available engines before filtering them out.
    if let Some(ref names) = desired {
        for n in names {
            if let Some(e) = all_engines.iter().find(|e| e.name() == n) {
                if !e.is_available() {
                    notes.push(format!(
                        "engine '{}' is enabled but not installed/available -- skipped",
                        n
                    ));
                }
            } else {
                notes.push(format!("engine '{}' is unknown -- skipped", n));
            }
        }
    }

    let engines: Vec<_> = all_engines
        .into_iter()
        .filter(|e| {
            if !e.is_available() {
                return false;
            }
            match desired {
                Some(ref names) => names.iter().any(|n| n == e.name()),
                None => true,
            }
        })
        .collect();

    if engines.is_empty() {
        return Json(ApiResponse::success(OcrRunResponse {
            results: vec![],
            notes,
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

    Json(ApiResponse::success(OcrRunResponse { results, notes })).into_response()
}

// ============================================================================
// Router
// ============================================================================

/// Creates the OCR sub-router with all OCR endpoints.
pub fn ocr_routes() -> Router<AppState> {
    Router::new()
        .route("/ocr/engines", get(list_ocr_engines))
        .route("/ocr/engines/{name}/enable", post(enable_engine))
        .route("/ocr/engines/{name}/disable", post(disable_engine))
        .route("/ocr/config", get(get_ocr_config))
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
    fn test_validate_engine_known_and_unknown() {
        assert!(validate_engine("tesseract").is_ok());
        let err = validate_engine("nope").unwrap_err();
        assert!(err.contains("Unknown engine"));
        assert!(err.contains("tesseract"));
    }

    #[test]
    fn test_runtime_config_enable_disable_global() {
        use crate::ocr::OcrRuntimeConfig;
        let mut cfg = OcrRuntimeConfig::with_all_enabled();
        cfg.global.insert("tesseract".into(), false);
        assert_eq!(cfg.global.get("tesseract"), Some(&false));
        cfg.global.insert("tesseract".into(), true);
        assert_eq!(cfg.global.get("tesseract"), Some(&true));
    }

    #[test]
    fn test_per_tab_override_beats_global() {
        use crate::ocr::OcrRuntimeConfig;
        use std::collections::HashMap;
        let mut cfg = OcrRuntimeConfig::with_all_enabled();
        // globally surya enabled, but disabled just for one tab
        let mut tab_map = HashMap::new();
        tab_map.insert("surya".to_string(), false);
        cfg.per_tab.insert("tab_1".to_string(), tab_map);

        // global default still true
        assert_eq!(cfg.global.get("surya"), Some(&true));
        // tab override is false
        assert_eq!(
            cfg.per_tab.get("tab_1").and_then(|m| m.get("surya")),
            Some(&false)
        );
    }

    #[test]
    fn test_per_tab_cleanup_removes_entry() {
        use crate::ocr::OcrRuntimeConfig;
        use std::collections::HashMap;
        let mut cfg = OcrRuntimeConfig::default();
        cfg.per_tab.insert("tab_1".to_string(), HashMap::new());
        assert!(cfg.per_tab.contains_key("tab_1"));
        cfg.per_tab.remove("tab_1");
        assert!(!cfg.per_tab.contains_key("tab_1"));
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
