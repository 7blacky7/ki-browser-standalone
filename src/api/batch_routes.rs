//! Route handlers for batch operations and session management
//!
//! Provides Axum HTTP handlers for executing batch browser commands
//! (sequential or parallel) and managing sessions with cookie, storage,
//! and snapshot support.

use std::collections::HashMap;
use std::time::Instant;

use axum::{
    extract::{Path, State},
    http::StatusCode,
    response::IntoResponse,
    routing::{delete, get, post},
    Json, Router,
};
use serde::{Deserialize, Serialize};
use tracing::{error, info, warn};

use crate::api::batch::{
    BatchCommand, BatchNavigateExtract, BatchNavigateResult, BatchRequest, BatchResponse,
    PageResult, extract_content_script, extract_links_script,
    extract_structured_data_script, detect_forms_script,
};
use crate::api::ipc::{IpcCommand, IpcMessage};

/// Result type for parallel batch operation futures: (success, data, error_message, duration_ms)
type BatchFutureResult = (bool, Option<serde_json::Value>, Option<String>, u64);
use crate::api::routes::ApiResponse;
use crate::api::server::AppState;
use crate::api::session::{CookieInfo, SessionManager, SessionSnapshot, TabSnapshot};

// ============================================================================
// Global Session Manager (lazy-initialized)
// ============================================================================

/// Lazy-initialized global session manager.
///
/// Since `AppState` must not be modified, we use a global `SessionManager`
/// instance protected by `Arc<RwLock<>>` for thread-safe access from
/// async handlers.
static SESSION_MANAGER: once_cell::sync::Lazy<SessionManager> =
    once_cell::sync::Lazy::new(SessionManager::new);

// ============================================================================
// Request / Response Types
// ============================================================================

/// Request body for creating a new session.
#[derive(Debug, Deserialize)]
pub struct CreateSessionRequest {
    /// Optional human-readable session name.
    #[serde(default)]
    pub name: Option<String>,
}

/// Request body for setting a key-value pair in session storage.
#[derive(Debug, Deserialize)]
pub struct SetStorageRequest {
    /// Storage key.
    pub key: String,
    /// Storage value (arbitrary JSON).
    pub value: serde_json::Value,
}

/// Response for a storage get operation.
#[derive(Debug, Serialize)]
pub struct StorageValueResponse {
    pub key: String,
    pub value: serde_json::Value,
}

/// Request body for setting a cookie via JavaScript.
#[derive(Debug, Deserialize)]
pub struct SetCookieRequest {
    pub name: String,
    pub value: String,
    #[serde(default)]
    pub domain: Option<String>,
    #[serde(default = "default_path")]
    pub path: String,
    #[serde(default)]
    pub expires: Option<String>,
    #[serde(default)]
    pub secure: bool,
    #[serde(default)]
    pub same_site: Option<String>,
}

fn default_path() -> String {
    "/".to_string()
}

/// Request body for creating a session snapshot.
#[derive(Debug, Deserialize)]
pub struct CreateSnapshotRequest {
    /// Snapshot name (must be unique within the session).
    pub name: String,
    /// Optional description.
    #[serde(default)]
    pub description: Option<String>,
}

/// Summary information about a snapshot (used in list responses).
#[derive(Debug, Serialize)]
pub struct SnapshotSummary {
    pub name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    pub created_at: String,
    pub tab_count: usize,
}

// ============================================================================
// Helpers
// ============================================================================

/// Unwrap the IPC response `{"result": <value>}` wrapper from EvaluateScript.
///
/// The browser handler wraps evaluate results inside `{"result": value}`.
/// This helper extracts the inner value, handling both string-encoded JSON
/// and pre-parsed values.
fn unwrap_ipc_result(data: &serde_json::Value) -> Option<serde_json::Value> {
    if let serde_json::Value::Object(map) = data {
        if let Some(result) = map.get("result") {
            return Some(result.clone());
        }
    }
    Some(data.clone())
}

/// Extract a JSON string from an IPC EvaluateScript response.
///
/// Handles the chain: IPC response → `{"result": "...json..."}` → parsed JSON Value.
fn parse_ipc_json_result(data: &serde_json::Value) -> Option<serde_json::Value> {
    let result = unwrap_ipc_result(data)?;
    match &result {
        serde_json::Value::String(s) => serde_json::from_str(s).ok(),
        serde_json::Value::Null => None,
        other => Some(other.clone()),
    }
}

// ============================================================================
// Router
// ============================================================================

/// Build the batch and session router.
///
/// All routes use `AppState` as the Axum state for IPC access.
pub fn batch_session_routes() -> Router<AppState> {
    Router::new()
        // Batch operations
        .route("/batch", post(execute_batch))
        .route("/batch/navigate-and-extract", post(batch_navigate_extract))
        // Session lifecycle
        .route("/session/start", post(create_session))
        .route("/session/list", get(list_sessions))
        .route("/session/{id}", get(get_session))
        .route("/session/{id}", delete(delete_session))
        // Session key-value storage
        .route("/session/{id}/storage", post(set_storage))
        .route("/session/{id}/storage/{key}", get(get_storage))
        // Cookie management via JS injection
        .route("/tabs/{tab_id}/cookies", get(get_cookies))
        .route("/tabs/{tab_id}/cookies", post(set_cookies))
        // LocalStorage via JS injection
        .route("/tabs/{tab_id}/local-storage", get(get_local_storage))
        // Session snapshots
        .route("/session/{id}/snapshot", post(create_snapshot))
        .route("/session/{id}/snapshots", get(list_snapshots))
}

// ============================================================================
// Batch Handlers
// ============================================================================

/// POST /batch - Execute a batch of browser commands.
///
/// Supports sequential (default) and parallel execution modes. When
/// `stop_on_error` is true (default), sequential execution aborts on the
/// first failure. Each operation result includes individual timing.
async fn execute_batch(
    State(state): State<AppState>,
    Json(request): Json<BatchRequest>,
) -> impl IntoResponse {
    if !state.is_enabled().await {
        return (
            StatusCode::SERVICE_UNAVAILABLE,
            Json(ApiResponse::<BatchResponse>::error("API is disabled")),
        )
            .into_response();
    }

    // Validate the request
    if let Err(e) = request.validate() {
        return (
            StatusCode::BAD_REQUEST,
            Json(ApiResponse::<BatchResponse>::error(e)),
        )
            .into_response();
    }

    let batch_start = Instant::now();

    // Resolve the default tab ID for operations that don't specify one
    let default_tab_id = {
        let browser_state = state.browser_state.read().await;
        browser_state.active_tab_id.clone()
    };

    let mut batch_response = BatchResponse::new();

    if request.parallel {
        // --- Parallel execution ---
        let commands = request.to_ipc_commands(default_tab_id.as_deref());

        // Also handle Wait operations separately
        let mut futures: Vec<(String, tokio::task::JoinHandle<BatchFutureResult>)> = Vec::new();

        for op in &request.operations {
            let op_id = op.id.clone();

            match &op.command {
                BatchCommand::Wait { condition } => {
                    // Wait operations become EvaluateScript calls
                    let script = condition.to_js_expression();
                    let tab_id = default_tab_id.clone().unwrap_or_default();
                    let ipc = state.ipc_channel.clone();

                    let handle = tokio::spawn(async move {
                        let start = Instant::now();
                        let cmd = IpcCommand::EvaluateScript {
                            tab_id,
                            script,
                            await_promise: true,
                            frame_id: None,
                        };
                        match ipc.send_command(IpcMessage::Command(cmd)).await {
                            Ok(resp) if resp.success => {
                                (true, resp.data, None, start.elapsed().as_millis() as u64)
                            }
                            Ok(resp) => {
                                let err = resp.error.unwrap_or_else(|| "Wait condition failed".to_string());
                                (false, None, Some(err), start.elapsed().as_millis() as u64)
                            }
                            Err(e) => {
                                (false, None, Some(format!("IPC error: {}", e)), start.elapsed().as_millis() as u64)
                            }
                        }
                    });
                    futures.push((op_id, handle));
                }
                _ => {
                    // Find the corresponding IPC command
                    if let Some((_cmd_id, ipc_cmd)) = commands.iter().find(|(id, _)| id == &op_id) {
                        let ipc_cmd = ipc_cmd.clone();
                        let ipc = state.ipc_channel.clone();
                        let delay_ms = op.delay_ms;

                        let handle = tokio::spawn(async move {
                            // Apply optional delay before execution
                            if let Some(delay) = delay_ms {
                                tokio::time::sleep(std::time::Duration::from_millis(delay)).await;
                            }

                            let start = Instant::now();
                            match ipc.send_command(IpcMessage::Command(ipc_cmd)).await {
                                Ok(resp) if resp.success => {
                                    (true, resp.data, None, start.elapsed().as_millis() as u64)
                                }
                                Ok(resp) => {
                                    let err = resp.error.unwrap_or_else(|| "Command failed".to_string());
                                    (false, None, Some(err), start.elapsed().as_millis() as u64)
                                }
                                Err(e) => {
                                    (false, None, Some(format!("IPC error: {}", e)), start.elapsed().as_millis() as u64)
                                }
                            }
                        });
                        futures.push((op_id, handle));
                    }
                }
            }
        }

        // Await all futures
        for (op_id, handle) in futures {
            match handle.await {
                Ok((true, data, _, duration)) => {
                    batch_response.add_success(op_id, data, duration);
                }
                Ok((false, _, error, duration)) => {
                    batch_response.add_failure(
                        op_id,
                        error.unwrap_or_else(|| "Unknown error".to_string()),
                        duration,
                    );
                }
                Err(e) => {
                    batch_response.add_failure(op_id, format!("Task join error: {}", e), 0);
                }
            }
        }
    } else {
        // --- Sequential execution ---
        for op in &request.operations {
            // Apply optional delay before execution
            if let Some(delay) = op.delay_ms {
                tokio::time::sleep(std::time::Duration::from_millis(delay)).await;
            }

            // Handle wait_before condition
            if let Some(ref wait) = op.wait_before {
                let script = wait.to_js_expression();
                let tab_id = default_tab_id.clone().unwrap_or_default();
                let cmd = IpcCommand::EvaluateScript {
                    tab_id,
                    script,
                    await_promise: true,
                    frame_id: None,
                };
                let wait_start = Instant::now();
                match state.ipc_channel.send_command(IpcMessage::Command(cmd)).await {
                    Ok(resp) if !resp.success => {
                        let err = resp
                            .error
                            .unwrap_or_else(|| "Wait condition failed".to_string());
                        batch_response.add_failure(op.id.clone(), err, wait_start.elapsed().as_millis() as u64);
                        if request.stop_on_error {
                            break;
                        }
                        continue;
                    }
                    Err(e) => {
                        batch_response.add_failure(
                            op.id.clone(),
                            format!("Wait IPC error: {}", e),
                            wait_start.elapsed().as_millis() as u64,
                        );
                        if request.stop_on_error {
                            break;
                        }
                        continue;
                    }
                    _ => { /* wait succeeded, proceed */ }
                }
            }

            // Handle Wait commands (they don't map to IPC, they run as scripts)
            let op_start = Instant::now();
            match &op.command {
                BatchCommand::Wait { condition } => {
                    let script = condition.to_js_expression();
                    let tab_id = default_tab_id.clone().unwrap_or_default();
                    let cmd = IpcCommand::EvaluateScript {
                        tab_id,
                        script,
                        await_promise: true,
                        frame_id: None,
                    };
                    match state.ipc_channel.send_command(IpcMessage::Command(cmd)).await {
                        Ok(resp) if resp.success => {
                            batch_response.add_success(
                                op.id.clone(),
                                resp.data,
                                op_start.elapsed().as_millis() as u64,
                            );
                        }
                        Ok(resp) => {
                            let err = resp.error.unwrap_or_else(|| "Wait failed".to_string());
                            batch_response.add_failure(op.id.clone(), err, op_start.elapsed().as_millis() as u64);
                            if request.stop_on_error {
                                break;
                            }
                        }
                        Err(e) => {
                            batch_response.add_failure(
                                op.id.clone(),
                                format!("IPC error: {}", e),
                                op_start.elapsed().as_millis() as u64,
                            );
                            if request.stop_on_error {
                                break;
                            }
                        }
                    }
                }
                _ => {
                    // Convert to IPC command using the helper from batch.rs
                    let single_batch = BatchRequest {
                        operations: vec![op.clone()],
                        parallel: false,
                        stop_on_error: true,
                        timeout_ms: None,
                    };
                    let cmds = single_batch.to_ipc_commands(default_tab_id.as_deref());

                    if let Some((_id, ipc_cmd)) = cmds.into_iter().next() {
                        match state
                            .ipc_channel
                            .send_command(IpcMessage::Command(ipc_cmd))
                            .await
                        {
                            Ok(resp) if resp.success => {
                                batch_response.add_success(
                                    op.id.clone(),
                                    resp.data,
                                    op_start.elapsed().as_millis() as u64,
                                );
                            }
                            Ok(resp) => {
                                let err = resp
                                    .error
                                    .unwrap_or_else(|| "Command failed".to_string());
                                batch_response.add_failure(
                                    op.id.clone(),
                                    err,
                                    op_start.elapsed().as_millis() as u64,
                                );
                                if request.stop_on_error {
                                    break;
                                }
                            }
                            Err(e) => {
                                batch_response.add_failure(
                                    op.id.clone(),
                                    format!("IPC error: {}", e),
                                    op_start.elapsed().as_millis() as u64,
                                );
                                if request.stop_on_error {
                                    break;
                                }
                            }
                        }
                    }
                }
            }
        }
    }

    batch_response.finalize(batch_start.elapsed().as_millis() as u64);

    info!(
        "Batch completed: {}/{} succeeded in {}ms",
        batch_response.succeeded,
        batch_response.succeeded + batch_response.failed,
        batch_response.total_time_ms
    );

    Json(ApiResponse::success(batch_response)).into_response()
}

/// POST /batch/navigate-and-extract - Navigate to multiple URLs and extract data.
///
/// Opens tabs (up to `parallel_limit`), navigates to each URL, optionally
/// waits, then extracts the requested data (screenshot, text, metadata, etc.).
async fn batch_navigate_extract(
    State(state): State<AppState>,
    Json(request): Json<BatchNavigateExtract>,
) -> impl IntoResponse {
    if !state.is_enabled().await {
        return (
            StatusCode::SERVICE_UNAVAILABLE,
            Json(ApiResponse::<BatchNavigateResult>::error("API is disabled")),
        )
            .into_response();
    }

    if request.urls.is_empty() {
        return (
            StatusCode::BAD_REQUEST,
            Json(ApiResponse::<BatchNavigateResult>::error(
                "At least one URL is required",
            )),
        )
            .into_response();
    }

    let total_start = Instant::now();
    let parallel_limit = request.parallel_limit.unwrap_or(3).max(1);

    // Process URLs in chunks based on parallel_limit
    let mut all_results: Vec<PageResult> = Vec::with_capacity(request.urls.len());

    for chunk in request.urls.chunks(parallel_limit) {
        let mut handles: Vec<tokio::task::JoinHandle<PageResult>> = Vec::new();

        for url in chunk {
            let url = url.clone();
            let ipc = state.ipc_channel.clone();
            let extract = request.extract.clone();
            let wait_ms = request.wait_after_navigate_ms;

            let handle = tokio::spawn(async move {
                let page_start = Instant::now();
                let mut page_result = PageResult {
                    url: url.clone(),
                    success: false,
                    title: None,
                    screenshot: None,
                    html: None,
                    text: None,
                    metadata: None,
                    structured_data: None,
                    forms: None,
                    links: None,
                    error: None,
                    duration_ms: 0,
                };

                // Create a new tab for this URL
                let create_cmd = IpcCommand::CreateTab {
                    url: url.clone(),
                    active: false,
                };
                let tab_id = match ipc.send_command(IpcMessage::Command(create_cmd)).await {
                    Ok(resp) if resp.success => {
                        resp.tab_id.unwrap_or_default()
                    }
                    Ok(resp) => {
                        page_result.error = Some(resp.error.unwrap_or_else(|| "Failed to create tab".to_string()));
                        page_result.duration_ms = page_start.elapsed().as_millis() as u64;
                        return page_result;
                    }
                    Err(e) => {
                        page_result.error = Some(format!("IPC error creating tab: {}", e));
                        page_result.duration_ms = page_start.elapsed().as_millis() as u64;
                        return page_result;
                    }
                };

                // Wait after navigation if requested
                if let Some(wait) = wait_ms {
                    tokio::time::sleep(std::time::Duration::from_millis(wait)).await;
                }

                // Extract title
                let title_cmd = IpcCommand::EvaluateScript {
                    tab_id: tab_id.clone(),
                    script: "document.title".to_string(),
                    await_promise: false,
                    frame_id: None,
                };
                if let Ok(resp) = ipc.send_command(IpcMessage::Command(title_cmd)).await {
                    if resp.success {
                        page_result.title = resp
                            .data
                            .and_then(|v| v.as_str().map(String::from));
                    }
                }

                // Extract screenshot
                if extract.screenshot {
                    let cmd = IpcCommand::CaptureScreenshot {
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
                    if let Ok(resp) = ipc.send_command(IpcMessage::Command(cmd)).await {
                        if resp.success {
                            page_result.screenshot = resp
                                .data
                                .and_then(|d| d.get("screenshot").and_then(|v| v.as_str()).map(String::from));
                        }
                    }
                }

                // Extract HTML
                if extract.html {
                    let cmd = IpcCommand::EvaluateScript {
                        tab_id: tab_id.clone(),
                        script: "document.documentElement.outerHTML".to_string(),
                        await_promise: false,
                        frame_id: None,
                    };
                    if let Ok(resp) = ipc.send_command(IpcMessage::Command(cmd)).await {
                        if resp.success {
                            if let Some(data) = &resp.data {
                                let result = unwrap_ipc_result(data);
                                page_result.html = result.and_then(|v| {
                                    v.as_str().map(String::from)
                                });
                            }
                        }
                    }
                }

                // Extract text content
                if extract.text {
                    let cmd = IpcCommand::EvaluateScript {
                        tab_id: tab_id.clone(),
                        script: extract_content_script().to_string(),
                        await_promise: true,
                        frame_id: None,
                    };
                    if let Ok(resp) = ipc.send_command(IpcMessage::Command(cmd)).await {
                        if resp.success {
                            if let Some(data) = &resp.data {
                                if let Some(parsed) = parse_ipc_json_result(data) {
                                    page_result.text = parsed
                                        .get("text")
                                        .and_then(|v| v.as_str())
                                        .map(String::from);
                                }
                            }
                        }
                    }
                }

                // Extract metadata
                if extract.metadata {
                    let cmd = IpcCommand::EvaluateScript {
                        tab_id: tab_id.clone(),
                        script: extract_structured_data_script().to_string(),
                        await_promise: true,
                        frame_id: None,
                    };
                    if let Ok(resp) = ipc.send_command(IpcMessage::Command(cmd)).await {
                        if resp.success {
                            if let Some(data) = &resp.data {
                                page_result.metadata = parse_ipc_json_result(data);
                            }
                        }
                    }
                }

                // Extract structured data
                if extract.structured_data {
                    let cmd = IpcCommand::EvaluateScript {
                        tab_id: tab_id.clone(),
                        script: extract_structured_data_script().to_string(),
                        await_promise: true,
                        frame_id: None,
                    };
                    if let Ok(resp) = ipc.send_command(IpcMessage::Command(cmd)).await {
                        if resp.success {
                            if let Some(data) = &resp.data {
                                page_result.structured_data = parse_ipc_json_result(data);
                            }
                        }
                    }
                }

                // Detect forms
                if extract.forms {
                    let cmd = IpcCommand::EvaluateScript {
                        tab_id: tab_id.clone(),
                        script: detect_forms_script().to_string(),
                        await_promise: true,
                        frame_id: None,
                    };
                    if let Ok(resp) = ipc.send_command(IpcMessage::Command(cmd)).await {
                        if resp.success {
                            if let Some(data) = &resp.data {
                                page_result.forms = parse_ipc_json_result(data);
                            }
                        }
                    }
                }

                // Extract links
                if extract.links {
                    let cmd = IpcCommand::EvaluateScript {
                        tab_id: tab_id.clone(),
                        script: extract_links_script().to_string(),
                        await_promise: true,
                        frame_id: None,
                    };
                    if let Ok(resp) = ipc.send_command(IpcMessage::Command(cmd)).await {
                        if resp.success {
                            if let Some(data) = &resp.data {
                                if let Some(parsed) = parse_ipc_json_result(data) {
                                    if let Ok(links) = serde_json::from_value::<Vec<crate::api::batch::LinkInfo>>(parsed) {
                                        page_result.links = Some(links);
                                    }
                                }
                            }
                        }
                    }
                }

                // Close the tab when done
                let close_cmd = IpcCommand::CloseTab {
                    tab_id: tab_id.clone(),
                };
                let _ = ipc.send_command(IpcMessage::Command(close_cmd)).await;

                page_result.success = true;
                page_result.duration_ms = page_start.elapsed().as_millis() as u64;
                page_result
            });

            handles.push(handle);
        }

        // Collect results from this chunk
        for handle in handles {
            match handle.await {
                Ok(result) => all_results.push(result),
                Err(e) => {
                    all_results.push(PageResult {
                        url: String::new(),
                        success: false,
                        title: None,
                        screenshot: None,
                        html: None,
                        text: None,
                        metadata: None,
                        structured_data: None,
                        forms: None,
                        links: None,
                        error: Some(format!("Task join error: {}", e)),
                        duration_ms: 0,
                    });
                }
            }
        }
    }

    let total_time_ms = total_start.elapsed().as_millis() as u64;

    info!(
        "Batch navigate-and-extract completed: {} URLs in {}ms",
        all_results.len(),
        total_time_ms
    );

    Json(ApiResponse::success(BatchNavigateResult {
        results: all_results,
        total_time_ms,
    }))
    .into_response()
}

// ============================================================================
// Session Handlers
// ============================================================================

/// POST /session/start - Create a new session.
async fn create_session(
    State(_state): State<AppState>,
    Json(request): Json<CreateSessionRequest>,
) -> impl IntoResponse {
    let session = SESSION_MANAGER.create_session(request.name).await;
    info!("Created session via API: {}", session.id);
    Json(ApiResponse::success(session)).into_response()
}

/// GET /session/list - List all active sessions.
async fn list_sessions(State(_state): State<AppState>) -> impl IntoResponse {
    let sessions = SESSION_MANAGER.list_sessions().await;
    Json(ApiResponse::success(sessions)).into_response()
}

/// GET /session/{id} - Get session details.
async fn get_session(
    State(_state): State<AppState>,
    Path(id): Path<String>,
) -> impl IntoResponse {
    match SESSION_MANAGER.get_session(&id).await {
        Some(session) => Json(ApiResponse::success(session)).into_response(),
        None => (
            StatusCode::NOT_FOUND,
            Json(ApiResponse::<()>::error(format!("Session '{}' not found", id))),
        )
            .into_response(),
    }
}

/// DELETE /session/{id} - Delete a session.
async fn delete_session(
    State(_state): State<AppState>,
    Path(id): Path<String>,
) -> impl IntoResponse {
    if SESSION_MANAGER.delete_session(&id).await {
        info!("Deleted session via API: {}", id);
        Json(ApiResponse::success(())).into_response()
    } else {
        (
            StatusCode::NOT_FOUND,
            Json(ApiResponse::<()>::error(format!("Session '{}' not found", id))),
        )
            .into_response()
    }
}

/// POST /session/{id}/storage - Store a key-value pair in session storage.
async fn set_storage(
    State(_state): State<AppState>,
    Path(id): Path<String>,
    Json(request): Json<SetStorageRequest>,
) -> impl IntoResponse {
    if SESSION_MANAGER
        .set_storage(&id, request.key.clone(), request.value.clone())
        .await
    {
        Json(ApiResponse::success(StorageValueResponse {
            key: request.key,
            value: request.value,
        }))
        .into_response()
    } else {
        (
            StatusCode::NOT_FOUND,
            Json(ApiResponse::<()>::error(format!("Session '{}' not found", id))),
        )
            .into_response()
    }
}

/// GET /session/{id}/storage/{key} - Retrieve a value from session storage.
async fn get_storage(
    State(_state): State<AppState>,
    Path((id, key)): Path<(String, String)>,
) -> impl IntoResponse {
    match SESSION_MANAGER.get_storage(&id, &key).await {
        Some(value) => Json(ApiResponse::success(StorageValueResponse { key, value })).into_response(),
        None => {
            // Distinguish between "session not found" and "key not found"
            if SESSION_MANAGER.get_session(&id).await.is_some() {
                (
                    StatusCode::NOT_FOUND,
                    Json(ApiResponse::<()>::error(format!(
                        "Key '{}' not found in session '{}'",
                        key, id
                    ))),
                )
                    .into_response()
            } else {
                (
                    StatusCode::NOT_FOUND,
                    Json(ApiResponse::<()>::error(format!("Session '{}' not found", id))),
                )
                    .into_response()
            }
        }
    }
}

// ============================================================================
// Cookie / Storage Handlers (via JS injection)
// ============================================================================

/// GET /tabs/{tab_id}/cookies - Get cookies for a tab via JavaScript.
///
/// Uses `document.cookie` to read cookies visible to the page. Note that
/// `httpOnly` cookies are not accessible from JavaScript.
async fn get_cookies(
    State(state): State<AppState>,
    Path(tab_id): Path<String>,
) -> impl IntoResponse {
    if !state.is_enabled().await {
        return (
            StatusCode::SERVICE_UNAVAILABLE,
            Json(ApiResponse::<Vec<CookieInfo>>::error("API is disabled")),
        )
            .into_response();
    }

    let cmd = IpcCommand::EvaluateScript {
        tab_id: tab_id.clone(),
        script: SessionManager::get_cookies_script().to_string(),
        await_promise: false,
        frame_id: None,
    };

    match state.ipc_channel.send_command(IpcMessage::Command(cmd)).await {
        Ok(resp) if resp.success => {
            let cookies = parse_cookies_from_response(resp.data);
            Json(ApiResponse::success(cookies)).into_response()
        }
        Ok(resp) => (
            StatusCode::BAD_REQUEST,
            Json(ApiResponse::<Vec<CookieInfo>>::error(
                resp.error.unwrap_or_else(|| "Failed to get cookies".to_string()),
            )),
        )
            .into_response(),
        Err(e) => {
            error!("Failed to get cookies for tab {}: {}", tab_id, e);
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ApiResponse::<Vec<CookieInfo>>::error(format!(
                    "IPC error: {}",
                    e
                ))),
            )
                .into_response()
        }
    }
}

/// POST /tabs/{tab_id}/cookies - Set a cookie on a tab via JavaScript.
async fn set_cookies(
    State(state): State<AppState>,
    Path(tab_id): Path<String>,
    Json(request): Json<SetCookieRequest>,
) -> impl IntoResponse {
    if !state.is_enabled().await {
        return (
            StatusCode::SERVICE_UNAVAILABLE,
            Json(ApiResponse::<()>::error("API is disabled")),
        )
            .into_response();
    }

    let cookie_info = CookieInfo {
        name: request.name,
        value: request.value,
        domain: request.domain.unwrap_or_default(),
        path: request.path,
        expires: request.expires,
        http_only: false, // Cannot set httpOnly via JS
        secure: request.secure,
        same_site: request.same_site,
    };

    let script = SessionManager::set_cookie_script(&cookie_info);
    let cmd = IpcCommand::EvaluateScript {
        tab_id: tab_id.clone(),
        script,
        await_promise: false,
        frame_id: None,
    };

    match state.ipc_channel.send_command(IpcMessage::Command(cmd)).await {
        Ok(resp) if resp.success => {
            Json(ApiResponse::success(())).into_response()
        }
        Ok(resp) => (
            StatusCode::BAD_REQUEST,
            Json(ApiResponse::<()>::error(
                resp.error.unwrap_or_else(|| "Failed to set cookie".to_string()),
            )),
        )
            .into_response(),
        Err(e) => {
            error!("Failed to set cookie for tab {}: {}", tab_id, e);
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ApiResponse::<()>::error(format!("IPC error: {}", e))),
            )
                .into_response()
        }
    }
}

/// GET /tabs/{tab_id}/local-storage - Get all localStorage entries via JavaScript.
async fn get_local_storage(
    State(state): State<AppState>,
    Path(tab_id): Path<String>,
) -> impl IntoResponse {
    if !state.is_enabled().await {
        return (
            StatusCode::SERVICE_UNAVAILABLE,
            Json(ApiResponse::<HashMap<String, String>>::error("API is disabled")),
        )
            .into_response();
    }

    let cmd = IpcCommand::EvaluateScript {
        tab_id: tab_id.clone(),
        script: SessionManager::get_local_storage_script().to_string(),
        await_promise: false,
        frame_id: None,
    };

    match state.ipc_channel.send_command(IpcMessage::Command(cmd)).await {
        Ok(resp) if resp.success => {
            let storage = parse_storage_from_response(resp.data);
            Json(ApiResponse::success(storage)).into_response()
        }
        Ok(resp) => (
            StatusCode::BAD_REQUEST,
            Json(ApiResponse::<HashMap<String, String>>::error(
                resp.error
                    .unwrap_or_else(|| "Failed to get localStorage".to_string()),
            )),
        )
            .into_response(),
        Err(e) => {
            error!("Failed to get localStorage for tab {}: {}", tab_id, e);
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ApiResponse::<HashMap<String, String>>::error(format!(
                    "IPC error: {}",
                    e
                ))),
            )
                .into_response()
        }
    }
}

// ============================================================================
// Snapshot Handlers
// ============================================================================

/// POST /session/{id}/snapshot - Create a state snapshot for a session.
///
/// For each tab in the session, captures the current URL, cookies,
/// localStorage, and sessionStorage via JavaScript.
async fn create_snapshot(
    State(state): State<AppState>,
    Path(id): Path<String>,
    Json(request): Json<CreateSnapshotRequest>,
) -> impl IntoResponse {
    if !state.is_enabled().await {
        return (
            StatusCode::SERVICE_UNAVAILABLE,
            Json(ApiResponse::<SessionSnapshot>::error("API is disabled")),
        )
            .into_response();
    }

    // Verify the session exists and get its tab list
    let session = match SESSION_MANAGER.get_session(&id).await {
        Some(s) => s,
        None => {
            return (
                StatusCode::NOT_FOUND,
                Json(ApiResponse::<SessionSnapshot>::error(format!(
                    "Session '{}' not found",
                    id
                ))),
            )
                .into_response();
        }
    };

    // Capture state for each tab in the session
    let mut tab_states: Vec<TabSnapshot> = Vec::new();

    for tab_id in &session.tabs {
        let cmd = IpcCommand::EvaluateScript {
            tab_id: tab_id.clone(),
            script: SessionManager::capture_tab_state_script().to_string(),
            await_promise: false,
            frame_id: None,
        };

        match state.ipc_channel.send_command(IpcMessage::Command(cmd)).await {
            Ok(resp) if resp.success => {
                if let Some(data) = resp.data {
                    let tab_snapshot = parse_tab_snapshot(tab_id, data);
                    tab_states.push(tab_snapshot);
                } else {
                    // No data returned, create empty snapshot for this tab
                    tab_states.push(TabSnapshot {
                        tab_id: tab_id.clone(),
                        url: String::new(),
                        title: None,
                        cookies: Vec::new(),
                        local_storage: HashMap::new(),
                        session_storage: HashMap::new(),
                    });
                }
            }
            Ok(_) | Err(_) => {
                warn!(
                    "Could not capture state for tab {} in session {}, using empty snapshot",
                    tab_id, id
                );
                tab_states.push(TabSnapshot {
                    tab_id: tab_id.clone(),
                    url: String::new(),
                    title: None,
                    cookies: Vec::new(),
                    local_storage: HashMap::new(),
                    session_storage: HashMap::new(),
                });
            }
        }
    }

    match SESSION_MANAGER
        .create_snapshot(&id, request.name, request.description, tab_states)
        .await
    {
        Some(snapshot) => Json(ApiResponse::success(snapshot)).into_response(),
        None => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ApiResponse::<SessionSnapshot>::error("Failed to create snapshot")),
        )
            .into_response(),
    }
}

/// GET /session/{id}/snapshots - List all snapshots for a session.
async fn list_snapshots(
    State(_state): State<AppState>,
    Path(id): Path<String>,
) -> impl IntoResponse {
    // Verify session exists
    let session = match SESSION_MANAGER.get_session(&id).await {
        Some(s) => s,
        None => {
            return (
                StatusCode::NOT_FOUND,
                Json(ApiResponse::<Vec<SnapshotSummary>>::error(format!(
                    "Session '{}' not found",
                    id
                ))),
            )
                .into_response();
        }
    };

    let summaries: Vec<SnapshotSummary> = session
        .snapshots
        .iter()
        .map(|snap| SnapshotSummary {
            name: snap.name.clone(),
            description: snap.description.clone(),
            created_at: snap.created_at.clone(),
            tab_count: snap.tab_states.len(),
        })
        .collect();

    Json(ApiResponse::success(summaries)).into_response()
}

// ============================================================================
// Helper Functions
// ============================================================================

/// Parse a list of `CookieInfo` from an IPC response data value.
///
/// The JS script returns a JSON string; the IPC response may wrap it
/// as a string value or as a parsed JSON value.
fn parse_cookies_from_response(data: Option<serde_json::Value>) -> Vec<CookieInfo> {
    let Some(data) = data else {
        return Vec::new();
    };

    // Unwrap IPC {"result": ...} wrapper, then parse the JSON string
    let parsed = parse_ipc_json_result(&data);
    match parsed {
        Some(val) => serde_json::from_value::<Vec<CookieInfo>>(val).unwrap_or_default(),
        None => Vec::new(),
    }
}

/// Parse a localStorage/sessionStorage map from an IPC response.
fn parse_storage_from_response(data: Option<serde_json::Value>) -> HashMap<String, String> {
    let Some(data) = data else {
        return HashMap::new();
    };

    let parsed = parse_ipc_json_result(&data);
    match parsed {
        Some(val) => serde_json::from_value::<HashMap<String, String>>(val).unwrap_or_default(),
        None => HashMap::new(),
    }
}

/// Parse tab state data returned by `capture_tab_state_script()` into a `TabSnapshot`.
fn parse_tab_snapshot(tab_id: &str, data: serde_json::Value) -> TabSnapshot {
    // Unwrap IPC {"result": ...} wrapper, then parse the JSON string
    let parsed: serde_json::Value = parse_ipc_json_result(&data)
        .unwrap_or(serde_json::Value::Null);

    let url = parsed
        .get("url")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_string();

    let title = parsed
        .get("title")
        .and_then(|v| v.as_str())
        .map(String::from);

    let cookies: Vec<CookieInfo> = parsed
        .get("cookies")
        .and_then(|v| serde_json::from_value(v.clone()).ok())
        .unwrap_or_default();

    let local_storage: HashMap<String, String> = parsed
        .get("local_storage")
        .and_then(|v| serde_json::from_value(v.clone()).ok())
        .unwrap_or_default();

    let session_storage: HashMap<String, String> = parsed
        .get("session_storage")
        .and_then(|v| serde_json::from_value(v.clone()).ok())
        .unwrap_or_default();

    TabSnapshot {
        tab_id: tab_id.to_string(),
        url,
        title,
        cookies,
        local_storage,
        session_storage,
    }
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_cookies_from_response_none() {
        let result = parse_cookies_from_response(None);
        assert!(result.is_empty());
    }

    #[test]
    fn test_parse_cookies_from_response_string() {
        let json = serde_json::json!(r#"[{"name":"sid","value":"abc","domain":"example.com","path":"/","expires":null,"http_only":false,"secure":true,"same_site":null}]"#);
        let result = parse_cookies_from_response(Some(json));
        assert_eq!(result.len(), 1);
        assert_eq!(result[0].name, "sid");
        assert_eq!(result[0].value, "abc");
    }

    #[test]
    fn test_parse_cookies_from_response_array() {
        let json = serde_json::json!([{
            "name": "token",
            "value": "xyz",
            "domain": "test.com",
            "path": "/",
            "http_only": false,
            "secure": false
        }]);
        let result = parse_cookies_from_response(Some(json));
        assert_eq!(result.len(), 1);
        assert_eq!(result[0].name, "token");
    }

    #[test]
    fn test_parse_storage_from_response_none() {
        let result = parse_storage_from_response(None);
        assert!(result.is_empty());
    }

    #[test]
    fn test_parse_storage_from_response_string() {
        let json = serde_json::json!(r#"{"theme":"dark","lang":"en"}"#);
        let result = parse_storage_from_response(Some(json));
        assert_eq!(result.len(), 2);
        assert_eq!(result.get("theme"), Some(&"dark".to_string()));
        assert_eq!(result.get("lang"), Some(&"en".to_string()));
    }

    #[test]
    fn test_parse_storage_from_response_object() {
        let json = serde_json::json!({"key1": "val1", "key2": "val2"});
        let result = parse_storage_from_response(Some(json));
        assert_eq!(result.len(), 2);
    }

    #[test]
    fn test_parse_tab_snapshot_string_data() {
        let data = serde_json::json!(
            r#"{"url":"https://example.com","title":"Example","cookies":[],"local_storage":{"theme":"dark"},"session_storage":{}}"#
        );
        let snapshot = parse_tab_snapshot("tab_1", data);
        assert_eq!(snapshot.tab_id, "tab_1");
        assert_eq!(snapshot.url, "https://example.com");
        assert_eq!(snapshot.title, Some("Example".to_string()));
        assert_eq!(
            snapshot.local_storage.get("theme"),
            Some(&"dark".to_string())
        );
        assert!(snapshot.cookies.is_empty());
        assert!(snapshot.session_storage.is_empty());
    }

    #[test]
    fn test_parse_tab_snapshot_object_data() {
        let data = serde_json::json!({
            "url": "https://test.com",
            "title": "Test",
            "cookies": [{
                "name": "sid",
                "value": "123",
                "domain": "test.com",
                "path": "/",
                "http_only": false,
                "secure": true
            }],
            "local_storage": {},
            "session_storage": {"token": "abc"}
        });
        let snapshot = parse_tab_snapshot("tab_2", data);
        assert_eq!(snapshot.url, "https://test.com");
        assert_eq!(snapshot.cookies.len(), 1);
        assert_eq!(
            snapshot.session_storage.get("token"),
            Some(&"abc".to_string())
        );
    }

    #[test]
    fn test_parse_tab_snapshot_null_data() {
        let snapshot = parse_tab_snapshot("tab_x", serde_json::Value::Null);
        assert_eq!(snapshot.tab_id, "tab_x");
        assert_eq!(snapshot.url, "");
        assert!(snapshot.title.is_none());
        assert!(snapshot.cookies.is_empty());
    }

    #[test]
    fn test_create_session_request_deserialization() {
        let json = r#"{"name": "My Session"}"#;
        let req: CreateSessionRequest = serde_json::from_str(json).unwrap();
        assert_eq!(req.name, Some("My Session".to_string()));

        let json_empty = r#"{}"#;
        let req: CreateSessionRequest = serde_json::from_str(json_empty).unwrap();
        assert!(req.name.is_none());
    }

    #[test]
    fn test_set_storage_request_deserialization() {
        let json = r#"{"key": "results", "value": [1, 2, 3]}"#;
        let req: SetStorageRequest = serde_json::from_str(json).unwrap();
        assert_eq!(req.key, "results");
        assert_eq!(req.value, serde_json::json!([1, 2, 3]));
    }

    #[test]
    fn test_set_cookie_request_deserialization() {
        let json = r#"{"name": "token", "value": "abc123", "secure": true}"#;
        let req: SetCookieRequest = serde_json::from_str(json).unwrap();
        assert_eq!(req.name, "token");
        assert_eq!(req.value, "abc123");
        assert!(req.secure);
        assert_eq!(req.path, "/"); // default
        assert!(req.domain.is_none());
    }

    #[test]
    fn test_create_snapshot_request_deserialization() {
        let json = r#"{"name": "before_login", "description": "State before login"}"#;
        let req: CreateSnapshotRequest = serde_json::from_str(json).unwrap();
        assert_eq!(req.name, "before_login");
        assert_eq!(
            req.description,
            Some("State before login".to_string())
        );
    }

    #[test]
    fn test_snapshot_summary_serialization() {
        let summary = SnapshotSummary {
            name: "checkpoint1".to_string(),
            description: None,
            created_at: "2026-01-01T00:00:00Z".to_string(),
            tab_count: 3,
        };
        let json = serde_json::to_string(&summary).unwrap();
        assert!(json.contains("checkpoint1"));
        assert!(json.contains("tab_count"));
        // description is None, should be omitted
        assert!(!json.contains("description"));
    }
}
