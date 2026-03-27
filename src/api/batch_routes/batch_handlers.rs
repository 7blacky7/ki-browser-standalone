//! Batch operation route handlers for executing multiple browser commands
//! sequentially or in parallel.

use std::time::Instant;

use axum::{
    extract::State,
    http::StatusCode,
    response::IntoResponse,
    Json,
};
use tracing::info;

use crate::api::batch::{
    BatchCommand, BatchNavigateExtract, BatchNavigateResult, BatchRequest, BatchResponse,
    PageResult, extract_content_script, extract_links_script,
    extract_structured_data_script, detect_forms_script,
};
use crate::api::ipc::{IpcCommand, IpcMessage};
use crate::api::routes::ApiResponse;
use crate::api::server::AppState;
use super::helpers::{unwrap_ipc_result, parse_ipc_json_result};

/// Result type for parallel batch operation futures: (success, data, error_message, duration_ms)
type BatchFutureResult = (bool, Option<serde_json::Value>, Option<String>, u64);

/// POST /batch - Execute a batch of browser commands.
///
/// Supports sequential (default) and parallel execution modes. When
/// `stop_on_error` is true (default), sequential execution aborts on the
/// first failure. Each operation result includes individual timing.
pub(super) async fn execute_batch(
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
        execute_parallel(&state, &request, default_tab_id.as_deref(), &mut batch_response).await;
    } else {
        execute_sequential(&state, &request, default_tab_id.as_deref(), &mut batch_response).await;
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

/// Execute batch operations in parallel using tokio tasks.
async fn execute_parallel(
    state: &AppState,
    request: &BatchRequest,
    default_tab_id: Option<&str>,
    batch_response: &mut BatchResponse,
) {
    let commands = request.to_ipc_commands(default_tab_id);
    let mut futures: Vec<(String, tokio::task::JoinHandle<BatchFutureResult>)> = Vec::new();

    for op in &request.operations {
        let op_id = op.id.clone();

        match &op.command {
            BatchCommand::Wait { condition } => {
                let script = condition.to_js_expression();
                let tab_id = default_tab_id.unwrap_or_default().to_string();
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
                if let Some((_cmd_id, ipc_cmd)) = commands.iter().find(|(id, _)| id == &op_id) {
                    let ipc_cmd = ipc_cmd.clone();
                    let ipc = state.ipc_channel.clone();
                    let delay_ms = op.delay_ms;

                    let handle = tokio::spawn(async move {
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
}

/// Execute batch operations sequentially, optionally stopping on first error.
async fn execute_sequential(
    state: &AppState,
    request: &BatchRequest,
    default_tab_id: Option<&str>,
    batch_response: &mut BatchResponse,
) {
    for op in &request.operations {
        // Apply optional delay before execution
        if let Some(delay) = op.delay_ms {
            tokio::time::sleep(std::time::Duration::from_millis(delay)).await;
        }

        // Handle wait_before condition
        if let Some(ref wait) = op.wait_before {
            let script = wait.to_js_expression();
            let tab_id = default_tab_id.unwrap_or_default().to_string();
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
                let tab_id = default_tab_id.unwrap_or_default().to_string();
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
                let single_batch = BatchRequest {
                    operations: vec![op.clone()],
                    parallel: false,
                    stop_on_error: true,
                    timeout_ms: None,
                };
                let cmds = single_batch.to_ipc_commands(default_tab_id);

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

/// POST /batch/navigate-and-extract - Navigate to multiple URLs and extract data.
///
/// Opens tabs (up to `parallel_limit`), navigates to each URL, optionally
/// waits, then extracts the requested data (screenshot, text, metadata, etc.).
pub(super) async fn batch_navigate_extract(
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

    let mut all_results: Vec<PageResult> = Vec::with_capacity(request.urls.len());

    for chunk in request.urls.chunks(parallel_limit) {
        let mut handles: Vec<tokio::task::JoinHandle<PageResult>> = Vec::new();

        for url in chunk {
            let url = url.clone();
            let ipc = state.ipc_channel.clone();
            let extract = request.extract.clone();
            let wait_ms = request.wait_after_navigate_ms;

            let handle = tokio::spawn(async move {
                process_url(url, ipc, extract, wait_ms).await
            });

            handles.push(handle);
        }

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

/// Process a single URL: create tab, extract data, close tab.
async fn process_url(
    url: String,
    ipc: std::sync::Arc<crate::api::ipc::IpcChannel>,
    extract: crate::api::batch::ExtractOptions,
    wait_ms: Option<u64>,
) -> PageResult {
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

    if extract.screenshot {
        extract_screenshot(&ipc, &tab_id, &mut page_result).await;
    }
    if extract.html {
        extract_html(&ipc, &tab_id, &mut page_result).await;
    }
    if extract.text {
        extract_text(&ipc, &tab_id, &mut page_result).await;
    }
    if extract.metadata {
        extract_metadata(&ipc, &tab_id, &mut page_result).await;
    }
    if extract.structured_data {
        extract_structured(&ipc, &tab_id, &mut page_result).await;
    }
    if extract.forms {
        extract_forms(&ipc, &tab_id, &mut page_result).await;
    }
    if extract.links {
        extract_page_links(&ipc, &tab_id, &mut page_result).await;
    }

    // Close the tab when done
    let close_cmd = IpcCommand::CloseTab { tab_id };
    let _ = ipc.send_command(IpcMessage::Command(close_cmd)).await;

    page_result.success = true;
    page_result.duration_ms = page_start.elapsed().as_millis() as u64;
    page_result
}

async fn extract_screenshot(ipc: &std::sync::Arc<crate::api::ipc::IpcChannel>, tab_id: &str, result: &mut PageResult) {
    let cmd = IpcCommand::CaptureScreenshot {
        tab_id: tab_id.to_string(),
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
            result.screenshot = resp
                .data
                .and_then(|d| d.get("screenshot").and_then(|v| v.as_str()).map(String::from));
        }
    }
}

async fn extract_html(ipc: &std::sync::Arc<crate::api::ipc::IpcChannel>, tab_id: &str, result: &mut PageResult) {
    let cmd = IpcCommand::EvaluateScript {
        tab_id: tab_id.to_string(),
        script: "document.documentElement.outerHTML".to_string(),
        await_promise: false,
        frame_id: None,
    };
    if let Ok(resp) = ipc.send_command(IpcMessage::Command(cmd)).await {
        if resp.success {
            if let Some(data) = &resp.data {
                let r = unwrap_ipc_result(data);
                result.html = r.and_then(|v| v.as_str().map(String::from));
            }
        }
    }
}

async fn extract_text(ipc: &std::sync::Arc<crate::api::ipc::IpcChannel>, tab_id: &str, result: &mut PageResult) {
    let cmd = IpcCommand::EvaluateScript {
        tab_id: tab_id.to_string(),
        script: extract_content_script().to_string(),
        await_promise: true,
        frame_id: None,
    };
    if let Ok(resp) = ipc.send_command(IpcMessage::Command(cmd)).await {
        if resp.success {
            if let Some(data) = &resp.data {
                if let Some(parsed) = parse_ipc_json_result(data) {
                    result.text = parsed.get("text").and_then(|v| v.as_str()).map(String::from);
                }
            }
        }
    }
}

async fn extract_metadata(ipc: &std::sync::Arc<crate::api::ipc::IpcChannel>, tab_id: &str, result: &mut PageResult) {
    let cmd = IpcCommand::EvaluateScript {
        tab_id: tab_id.to_string(),
        script: extract_structured_data_script().to_string(),
        await_promise: true,
        frame_id: None,
    };
    if let Ok(resp) = ipc.send_command(IpcMessage::Command(cmd)).await {
        if resp.success {
            if let Some(data) = &resp.data {
                result.metadata = parse_ipc_json_result(data);
            }
        }
    }
}

async fn extract_structured(ipc: &std::sync::Arc<crate::api::ipc::IpcChannel>, tab_id: &str, result: &mut PageResult) {
    let cmd = IpcCommand::EvaluateScript {
        tab_id: tab_id.to_string(),
        script: extract_structured_data_script().to_string(),
        await_promise: true,
        frame_id: None,
    };
    if let Ok(resp) = ipc.send_command(IpcMessage::Command(cmd)).await {
        if resp.success {
            if let Some(data) = &resp.data {
                result.structured_data = parse_ipc_json_result(data);
            }
        }
    }
}

async fn extract_forms(ipc: &std::sync::Arc<crate::api::ipc::IpcChannel>, tab_id: &str, result: &mut PageResult) {
    let cmd = IpcCommand::EvaluateScript {
        tab_id: tab_id.to_string(),
        script: detect_forms_script().to_string(),
        await_promise: true,
        frame_id: None,
    };
    if let Ok(resp) = ipc.send_command(IpcMessage::Command(cmd)).await {
        if resp.success {
            if let Some(data) = &resp.data {
                result.forms = parse_ipc_json_result(data);
            }
        }
    }
}

async fn extract_page_links(ipc: &std::sync::Arc<crate::api::ipc::IpcChannel>, tab_id: &str, result: &mut PageResult) {
    let cmd = IpcCommand::EvaluateScript {
        tab_id: tab_id.to_string(),
        script: extract_links_script().to_string(),
        await_promise: true,
        frame_id: None,
    };
    if let Ok(resp) = ipc.send_command(IpcMessage::Command(cmd)).await {
        if resp.success {
            if let Some(data) = &resp.data {
                if let Some(parsed) = parse_ipc_json_result(data) {
                    if let Ok(links) = serde_json::from_value::<Vec<crate::api::batch::LinkInfo>>(parsed) {
                        result.links = Some(links);
                    }
                }
            }
        }
    }
}
