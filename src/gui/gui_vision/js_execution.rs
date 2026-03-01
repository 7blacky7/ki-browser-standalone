//! JavaScript execution helpers for vision tactics running on background threads.
//!
//! Provides `execute_js_blocking` which creates a minimal single-threaded tokio
//! runtime to send JS via `CefCommand::ExecuteJsWithResult`, and
//! `capture_dom_snapshot` which builds and runs the DOM snapshot extraction script.

use std::sync::Arc;

use uuid::Uuid;

use crate::browser::cef_engine::CefBrowserEngine;

/// Executes JavaScript via the CEF engine from a background thread.
///
/// Sends a `CefCommand::ExecuteJsWithResult` to the CEF command thread and
/// blocks the calling thread until the result arrives. The CEF command thread
/// pumps `do_message_loop_work()` internally while waiting for the JS
/// console.log result via the KI_RESULT protocol, so no deadlock occurs.
///
/// Must only be called from a non-tokio background thread (e.g. `std::thread::spawn`).
/// Creates a minimal single-threaded tokio runtime without IO/timer drivers since
/// the oneshot channel polling does not require those subsystems.
pub(in crate::gui) fn execute_js_blocking(
    engine: &Arc<CefBrowserEngine>,
    tab_id: Uuid,
    script: &str,
) -> Result<String, String> {
    tracing::debug!("execute_js_blocking: starting JS execution for tab {}", tab_id);

    // Build a minimal current-thread runtime with timer support for the
    // caller-side timeout. IO driver is omitted to avoid epoll conflicts
    // with the global tokio runtime that owns the main IO driver.
    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_time()
        .build()
        .map_err(|e| format!("Tokio runtime error: {}", e))?;

    let result = rt.block_on(async {
        // Caller-side timeout: 15 seconds (CEF internal timeout is 10s, this
        // catches cases where the CEF command thread itself is stuck).
        match tokio::time::timeout(
            std::time::Duration::from_secs(15),
            engine.execute_js_with_result(tab_id, script),
        )
        .await
        {
            Ok(Ok(Some(result))) => Ok(result),
            Ok(Ok(None)) => Err("JavaScript returned no result".to_string()),
            Ok(Err(e)) => Err(format!("JS execution failed: {}", e)),
            Err(_) => Err("JS execution timed out after 15s".to_string()),
        }
    });

    match &result {
        Ok(s) => tracing::debug!("execute_js_blocking: JS returned {} bytes", s.len()),
        Err(e) => tracing::warn!("execute_js_blocking: {}", e),
    }

    result
}

/// Captures DOM snapshot via JavaScript and returns the parsed snapshot.
///
/// Builds the snapshot extraction script, executes it via `execute_js_blocking`,
/// and parses the JSON result into a `DomSnapshot`. Used by vision tactics to
/// obtain element bounding boxes for screenshot annotation.
pub(in crate::gui) fn capture_dom_snapshot(
    engine: &Arc<CefBrowserEngine>,
    tab_id: Uuid,
) -> Result<crate::browser::dom_snapshot::DomSnapshot, String> {
    tracing::debug!("capture_dom_snapshot: starting for tab {}", tab_id);

    let config = crate::browser::dom_snapshot::SnapshotConfig {
        max_nodes: 5000,
        include_text: true,
    };
    let script = crate::browser::dom_snapshot::build_snapshot_script(&config);
    let json_str = execute_js_blocking(engine, tab_id, &script)?;
    let snapshot = crate::browser::dom_snapshot::parse_snapshot_json(&json_str)
        .map_err(|e| format!("DOM snapshot parsing failed: {}", e))?;

    tracing::debug!("capture_dom_snapshot: {} nodes found", snapshot.nodes.len());
    Ok(snapshot)
}
