//! DOM extraction route handlers for structured data, content, and form operations.
//!
//! Provides Axum route handlers that bridge the REST API to browser-side
//! JavaScript extraction scripts. Each handler:
//! 1. Validates the API is enabled
//! 2. Resolves the target tab (from request or active tab)
//! 3. Generates a JavaScript extraction script
//! 4. Sends it to the browser via IPC as an `EvaluateScript` command
//! 5. Parses the JSON result into a typed response

use axum::{
    extract::State,
    http::StatusCode,
    response::IntoResponse,
    routing::post,
    Json, Router,
};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use tracing::error;

use crate::api::ipc::{IpcCommand, IpcMessage};
use crate::api::routes::ApiResponse;
use crate::api::server::AppState;
use crate::browser::{
    ContentExtractor, ExtractedContent, FormFillRequest, FormFillResult, FormHandler, FormInfo,
    FormValidationResult, PageStructure, StructuredDataExtractor, StructuredPageData,
};

// ============================================================================
// Request/Response Structs
// ============================================================================

/// Request for structured data extraction.
#[derive(Debug, Deserialize)]
pub struct ExtractStructuredDataRequest {
    /// Target tab ID. If omitted, uses the active tab.
    #[serde(default)]
    pub tab_id: Option<String>,
}

/// Request for content extraction (Readability-like).
#[derive(Debug, Deserialize)]
pub struct ExtractContentRequest {
    /// Target tab ID. If omitted, uses the active tab.
    #[serde(default)]
    pub tab_id: Option<String>,
}

/// Request for page structure analysis.
#[derive(Debug, Deserialize)]
pub struct AnalyzeStructureRequest {
    /// Target tab ID. If omitted, uses the active tab.
    #[serde(default)]
    pub tab_id: Option<String>,
}

/// Request for form detection.
#[derive(Debug, Deserialize)]
pub struct DetectFormsRequest {
    /// Target tab ID. If omitted, uses the active tab.
    #[serde(default)]
    pub tab_id: Option<String>,
}

/// Request for form filling.
#[derive(Debug, Deserialize)]
pub struct FillFormRequest {
    /// Target tab ID. If omitted, uses the active tab.
    #[serde(default)]
    pub tab_id: Option<String>,

    /// CSS selector for the form to fill. If omitted, uses the first form.
    #[serde(default)]
    pub form_selector: Option<String>,

    /// Field values keyed by field identifier (name, id, label, placeholder,
    /// aria-label, autocomplete, or CSS selector).
    pub data: HashMap<String, serde_json::Value>,

    /// If true, text is typed character-by-character with random delays.
    #[serde(default)]
    pub human_like: bool,

    /// If true, the form is submitted after filling.
    #[serde(default)]
    pub submit: bool,

    /// If true, existing field values are cleared before filling.
    #[serde(default = "default_clear_first")]
    pub clear_first: bool,
}

fn default_clear_first() -> bool {
    true
}

/// Request for form validation.
#[derive(Debug, Deserialize)]
pub struct ValidateFormRequest {
    /// Target tab ID. If omitted, uses the active tab.
    #[serde(default)]
    pub tab_id: Option<String>,

    /// CSS selector for the form to validate. Defaults to "form".
    #[serde(default = "default_form_selector")]
    pub form_selector: String,
}

fn default_form_selector() -> String {
    "form".to_string()
}

/// Response wrapping detected forms.
#[derive(Debug, Serialize)]
pub struct DetectFormsResponse {
    pub forms: Vec<FormInfo>,
}

// ============================================================================
// Helper: Resolve tab_id
// ============================================================================

/// Resolves the target tab ID from an optional request value, falling back
/// to the currently active tab.
async fn resolve_tab_id(state: &AppState, request_tab_id: Option<String>) -> Option<String> {
    request_tab_id.or({
        let browser_state = state.browser_state.read().await;
        browser_state.active_tab_id.clone()
    })
}

/// Evaluates a JavaScript script in the specified tab and returns the raw
/// JSON string from the browser. Returns `Ok(json_string)` on success or
/// an error response on failure.
async fn evaluate_script_in_tab(
    state: &AppState,
    tab_id: String,
    script: String,
) -> Result<String, (StatusCode, Json<ApiResponse<()>>)> {
    let command = IpcCommand::EvaluateScript {
        tab_id,
        script,
        await_promise: true,
    };

    match state
        .ipc_channel
        .send_command(IpcMessage::Command(command))
        .await
    {
        Ok(response) => {
            if response.success {
                if let Some(data) = response.data {
                    // The IPC response wraps evaluate results as {"result": <value>}.
                    // Our extraction scripts return JSON.stringify(...), so the
                    // result field will be a JSON string containing serialized JSON.
                    // We need to extract that string for deserialization.
                    let result_value = match &data {
                        serde_json::Value::Object(map) => {
                            map.get("result").cloned().unwrap_or(data.clone())
                        }
                        _ => data.clone(),
                    };
                    match result_value {
                        serde_json::Value::String(s) => Ok(s),
                        serde_json::Value::Null => {
                            Err((
                                StatusCode::INTERNAL_SERVER_ERROR,
                                Json(ApiResponse::<()>::error(
                                    "Script returned null",
                                )),
                            ))
                        }
                        other => {
                            // If the browser returned the data already parsed
                            // (not as a string), re-serialize it so we can
                            // deserialize into the expected type.
                            Ok(other.to_string())
                        }
                    }
                } else {
                    Err((
                        StatusCode::INTERNAL_SERVER_ERROR,
                        Json(ApiResponse::<()>::error(
                            "Script returned no data",
                        )),
                    ))
                }
            } else {
                Err((
                    StatusCode::BAD_REQUEST,
                    Json(ApiResponse::<()>::error(
                        response
                            .error
                            .unwrap_or_else(|| "Script evaluation failed".to_string()),
                    )),
                ))
            }
        }
        Err(e) => {
            error!("IPC error during script evaluation: {}", e);
            Err((
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ApiResponse::<()>::error(format!(
                    "IPC error: {}",
                    e
                ))),
            ))
        }
    }
}

// ============================================================================
// Route Handlers
// ============================================================================

/// POST /dom/extract-structured-data - Extract JSON-LD, OpenGraph, Twitter Card,
/// meta tags, and Schema.org microdata from the current page.
async fn extract_structured_data(
    State(state): State<AppState>,
    Json(request): Json<ExtractStructuredDataRequest>,
) -> impl IntoResponse {
    if !state.is_enabled().await {
        return (
            StatusCode::SERVICE_UNAVAILABLE,
            Json(ApiResponse::<StructuredPageData>::error("API is disabled")),
        )
            .into_response();
    }

    let tab_id = match resolve_tab_id(&state, request.tab_id).await {
        Some(id) => id,
        None => {
            return (
                StatusCode::BAD_REQUEST,
                Json(ApiResponse::<StructuredPageData>::error(
                    "No tab specified and no active tab",
                )),
            )
                .into_response();
        }
    };

    let script = StructuredDataExtractor::extraction_script();

    match evaluate_script_in_tab(&state, tab_id, script).await {
        Ok(json_string) => match serde_json::from_str::<StructuredPageData>(&json_string) {
            Ok(data) => Json(ApiResponse::success(data)).into_response(),
            Err(e) => {
                error!("Failed to parse structured data response: {}", e);
                (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    Json(ApiResponse::<StructuredPageData>::error(format!(
                        "Failed to parse extraction result: {}",
                        e
                    ))),
                )
                    .into_response()
            }
        },
        Err((status, json)) => (status, json).into_response(),
    }
}

/// POST /dom/extract-content - Extract main readable content using a
/// Readability-like text-density algorithm.
async fn extract_content(
    State(state): State<AppState>,
    Json(request): Json<ExtractContentRequest>,
) -> impl IntoResponse {
    if !state.is_enabled().await {
        return (
            StatusCode::SERVICE_UNAVAILABLE,
            Json(ApiResponse::<ExtractedContent>::error("API is disabled")),
        )
            .into_response();
    }

    let tab_id = match resolve_tab_id(&state, request.tab_id).await {
        Some(id) => id,
        None => {
            return (
                StatusCode::BAD_REQUEST,
                Json(ApiResponse::<ExtractedContent>::error(
                    "No tab specified and no active tab",
                )),
            )
                .into_response();
        }
    };

    let script = ContentExtractor::content_extraction_script();

    match evaluate_script_in_tab(&state, tab_id, script).await {
        Ok(json_string) => match serde_json::from_str::<ExtractedContent>(&json_string) {
            Ok(data) => Json(ApiResponse::success(data)).into_response(),
            Err(e) => {
                error!("Failed to parse content extraction response: {}", e);
                (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    Json(ApiResponse::<ExtractedContent>::error(format!(
                        "Failed to parse extraction result: {}",
                        e
                    ))),
                )
                    .into_response()
            }
        },
        Err((status, json)) => (status, json).into_response(),
    }
}

/// POST /dom/analyze-structure - Analyze page structure, detecting sections,
/// navigation, and page type.
async fn analyze_structure(
    State(state): State<AppState>,
    Json(request): Json<AnalyzeStructureRequest>,
) -> impl IntoResponse {
    if !state.is_enabled().await {
        return (
            StatusCode::SERVICE_UNAVAILABLE,
            Json(ApiResponse::<PageStructure>::error("API is disabled")),
        )
            .into_response();
    }

    let tab_id = match resolve_tab_id(&state, request.tab_id).await {
        Some(id) => id,
        None => {
            return (
                StatusCode::BAD_REQUEST,
                Json(ApiResponse::<PageStructure>::error(
                    "No tab specified and no active tab",
                )),
            )
                .into_response();
        }
    };

    let script = ContentExtractor::structure_analysis_script();

    match evaluate_script_in_tab(&state, tab_id, script).await {
        Ok(json_string) => match serde_json::from_str::<PageStructure>(&json_string) {
            Ok(data) => Json(ApiResponse::success(data)).into_response(),
            Err(e) => {
                error!("Failed to parse structure analysis response: {}", e);
                (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    Json(ApiResponse::<PageStructure>::error(format!(
                        "Failed to parse analysis result: {}",
                        e
                    ))),
                )
                    .into_response()
            }
        },
        Err((status, json)) => (status, json).into_response(),
    }
}

/// POST /dom/forms - Detect and analyze all forms on the current page.
async fn detect_forms(
    State(state): State<AppState>,
    Json(request): Json<DetectFormsRequest>,
) -> impl IntoResponse {
    if !state.is_enabled().await {
        return (
            StatusCode::SERVICE_UNAVAILABLE,
            Json(ApiResponse::<DetectFormsResponse>::error("API is disabled")),
        )
            .into_response();
    }

    let tab_id = match resolve_tab_id(&state, request.tab_id).await {
        Some(id) => id,
        None => {
            return (
                StatusCode::BAD_REQUEST,
                Json(ApiResponse::<DetectFormsResponse>::error(
                    "No tab specified and no active tab",
                )),
            )
                .into_response();
        }
    };

    let script = FormHandler::detect_forms_script();

    match evaluate_script_in_tab(&state, tab_id, script).await {
        Ok(json_string) => match serde_json::from_str::<Vec<FormInfo>>(&json_string) {
            Ok(forms) => {
                Json(ApiResponse::success(DetectFormsResponse { forms })).into_response()
            }
            Err(e) => {
                error!("Failed to parse form detection response: {}", e);
                (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    Json(ApiResponse::<DetectFormsResponse>::error(format!(
                        "Failed to parse form detection result: {}",
                        e
                    ))),
                )
                    .into_response()
            }
        },
        Err((status, json)) => (status, json).into_response(),
    }
}

/// POST /dom/fill-form - Fill a form with the provided data, optionally
/// submitting it afterward.
async fn fill_form(
    State(state): State<AppState>,
    Json(request): Json<FillFormRequest>,
) -> impl IntoResponse {
    if !state.is_enabled().await {
        return (
            StatusCode::SERVICE_UNAVAILABLE,
            Json(ApiResponse::<FormFillResult>::error("API is disabled")),
        )
            .into_response();
    }

    let tab_id = match resolve_tab_id(&state, request.tab_id).await {
        Some(id) => id,
        None => {
            return (
                StatusCode::BAD_REQUEST,
                Json(ApiResponse::<FormFillResult>::error(
                    "No tab specified and no active tab",
                )),
            )
                .into_response();
        }
    };

    // Build the FormFillRequest for the script generator
    let fill_request = FormFillRequest {
        form_selector: request.form_selector,
        data: request.data,
        human_like: request.human_like,
        submit: request.submit,
        clear_first: request.clear_first,
    };

    // Choose the appropriate script variant based on human_like mode
    let script = if fill_request.human_like {
        FormHandler::fill_form_human_like_script(&fill_request)
    } else {
        FormHandler::fill_form_script(&fill_request)
    };

    match evaluate_script_in_tab(&state, tab_id, script).await {
        Ok(json_string) => match serde_json::from_str::<FormFillResult>(&json_string) {
            Ok(result) => Json(ApiResponse::success(result)).into_response(),
            Err(e) => {
                error!("Failed to parse form fill response: {}", e);
                (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    Json(ApiResponse::<FormFillResult>::error(format!(
                        "Failed to parse form fill result: {}",
                        e
                    ))),
                )
                    .into_response()
            }
        },
        Err((status, json)) => (status, json).into_response(),
    }
}

/// POST /dom/validate-form - Validate a form using HTML5 constraint validation.
async fn validate_form(
    State(state): State<AppState>,
    Json(request): Json<ValidateFormRequest>,
) -> impl IntoResponse {
    if !state.is_enabled().await {
        return (
            StatusCode::SERVICE_UNAVAILABLE,
            Json(ApiResponse::<FormValidationResult>::error("API is disabled")),
        )
            .into_response();
    }

    let tab_id = match resolve_tab_id(&state, request.tab_id).await {
        Some(id) => id,
        None => {
            return (
                StatusCode::BAD_REQUEST,
                Json(ApiResponse::<FormValidationResult>::error(
                    "No tab specified and no active tab",
                )),
            )
                .into_response();
        }
    };

    let script = FormHandler::validate_form_script(&request.form_selector);

    match evaluate_script_in_tab(&state, tab_id, script).await {
        Ok(json_string) => match serde_json::from_str::<FormValidationResult>(&json_string) {
            Ok(result) => Json(ApiResponse::success(result)).into_response(),
            Err(e) => {
                error!("Failed to parse form validation response: {}", e);
                (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    Json(ApiResponse::<FormValidationResult>::error(format!(
                        "Failed to parse validation result: {}",
                        e
                    ))),
                )
                    .into_response()
            }
        },
        Err((status, json)) => (status, json).into_response(),
    }
}

// ============================================================================
// Router Configuration
// ============================================================================

/// Create the extraction sub-router with all DOM extraction routes.
pub fn extraction_routes() -> Router<AppState> {
    Router::new()
        .route("/dom/extract-structured-data", post(extract_structured_data))
        .route("/dom/extract-content", post(extract_content))
        .route("/dom/analyze-structure", post(analyze_structure))
        .route("/dom/forms", post(detect_forms))
        .route("/dom/fill-form", post(fill_form))
        .route("/dom/validate-form", post(validate_form))
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_fill_form_request_deserialize_minimal() {
        let json = r#"{"data": {"username": "test"}}"#;
        let request: FillFormRequest = serde_json::from_str(json).unwrap();
        assert!(request.tab_id.is_none());
        assert!(request.form_selector.is_none());
        assert!(!request.human_like);
        assert!(!request.submit);
        assert!(request.clear_first); // default is true
        assert_eq!(request.data.len(), 1);
    }

    #[test]
    fn test_fill_form_request_deserialize_full() {
        let json = r#"{
            "tab_id": "tab_1",
            "form_selector": "form#login",
            "data": {"username": "admin", "password": "secret"},
            "human_like": true,
            "submit": true,
            "clear_first": false
        }"#;
        let request: FillFormRequest = serde_json::from_str(json).unwrap();
        assert_eq!(request.tab_id, Some("tab_1".to_string()));
        assert_eq!(request.form_selector, Some("form#login".to_string()));
        assert!(request.human_like);
        assert!(request.submit);
        assert!(!request.clear_first);
        assert_eq!(request.data.len(), 2);
    }

    #[test]
    fn test_validate_form_request_defaults() {
        let json = r#"{}"#;
        let request: ValidateFormRequest = serde_json::from_str(json).unwrap();
        assert!(request.tab_id.is_none());
        assert_eq!(request.form_selector, "form");
    }

    #[test]
    fn test_detect_forms_response_serialize() {
        let response = DetectFormsResponse { forms: vec![] };
        let json = serde_json::to_string(&response).unwrap();
        assert!(json.contains("\"forms\":[]"));
    }

    #[test]
    fn test_extract_structured_data_request_defaults() {
        let json = r#"{}"#;
        let request: ExtractStructuredDataRequest = serde_json::from_str(json).unwrap();
        assert!(request.tab_id.is_none());
    }

    #[test]
    fn test_extract_content_request_with_tab() {
        let json = r#"{"tab_id": "tab_3"}"#;
        let request: ExtractContentRequest = serde_json::from_str(json).unwrap();
        assert_eq!(request.tab_id, Some("tab_3".to_string()));
    }

    #[test]
    fn test_analyze_structure_request_defaults() {
        let json = r#"{}"#;
        let request: AnalyzeStructureRequest = serde_json::from_str(json).unwrap();
        assert!(request.tab_id.is_none());
    }
}
