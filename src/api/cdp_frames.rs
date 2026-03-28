//! CDP Frame-Context Helper for frame_id isolation.
//!
//! Resolves a `frame_id` (from `/frames` endpoint) to a CDP `executionContextId`,
//! then evaluates JavaScript within that specific frame context.
//!
//! **No caching**: frame IDs change on navigation, so every call resolves fresh.

use serde::Deserialize;
use tracing::debug;

use super::cdp_client::CdpClient;

// ============================================================================
// Types
// ============================================================================

/// A single frame from the CDP `Page.getFrameTree` response.
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct CdpFrame {
    id: String,
    #[allow(dead_code)]
    url: String,
    #[allow(dead_code)]
    name: Option<String>,
}

/// Wrapper for `Page.getFrameTree` result: `{ frameTree: { frame, childFrames } }`.
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct FrameTreeResult {
    frame_tree: FrameTreeNode,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct FrameTreeNode {
    frame: CdpFrame,
    #[serde(default)]
    child_frames: Vec<FrameTreeNode>,
}

// Note: We use Page.createIsolatedWorld instead of collecting
// Runtime.executionContextCreated events, since send_command skips
// events. createIsolatedWorld returns the executionContextId directly.

// ============================================================================
// Frame resolution helpers
// ============================================================================

/// Collect all frame IDs from a `Page.getFrameTree` response.
fn collect_frame_ids(node: &FrameTreeNode, out: &mut Vec<String>) {
    out.push(node.frame.id.clone());
    for child in &node.child_frames {
        collect_frame_ids(child, out);
    }
}

/// Checks whether `frame_id` exists in the frame tree returned by CDP.
///
/// Also handles the special JS-level names our `/frames` endpoint returns
/// (e.g. `"main"`, `"frame-0"`, iframe `id`/`name` attributes) by matching
/// them against the CDP frame tree order.
fn find_cdp_frame_id(tree: &FrameTreeNode, frame_id: &str) -> Option<String> {
    let mut all_ids = Vec::new();
    collect_frame_ids(tree, &mut all_ids);

    // Direct match by CDP frame ID
    if all_ids.contains(&frame_id.to_string()) {
        return Some(frame_id.to_string());
    }

    // Our /frames endpoint returns "main" for the root frame
    if frame_id == "main" {
        return all_ids.first().cloned();
    }

    // Our /frames endpoint returns "frame-N" for child frames (0-indexed)
    if let Some(idx_str) = frame_id.strip_prefix("frame-") {
        if let Ok(idx) = idx_str.parse::<usize>() {
            // child frames start at index 1 in all_ids (0 is main)
            return all_ids.get(idx + 1).cloned();
        }
    }

    // Try matching by iframe name/id attribute from the tree
    fn search_by_name(node: &FrameTreeNode, name: &str) -> Option<String> {
        if node.frame.name.as_deref() == Some(name) {
            return Some(node.frame.id.clone());
        }
        for child in &node.child_frames {
            if let Some(found) = search_by_name(child, name) {
                return Some(found);
            }
        }
        None
    }

    search_by_name(tree, frame_id)
}

// ============================================================================
// Public API
// ============================================================================

/// Resolve a `frame_id` to a CDP `executionContextId`.
///
/// Steps:
/// 1. `Runtime.enable` (idempotent — safe to call multiple times)
/// 2. `Page.getFrameTree` to validate the frame exists and get its CDP frame ID
/// 3. `Runtime.evaluate` with a no-op in each context to discover contexts,
///    OR use `Page.createIsolatedWorld` to get a context for the frame
///
/// Since `send_command` skips events (we cannot collect them after-the-fact),
/// we use `Page.createIsolatedWorld` which returns the `executionContextId`
/// directly — no event listening required.
pub async fn resolve_frame_execution_context(
    cdp: &CdpClient,
    ws_url: &str,
    frame_id: &str,
) -> Result<i64, String> {
    // 1. Enable Runtime domain (idempotent)
    cdp.send_command_pub(ws_url, "Runtime.enable", serde_json::json!({}))
        .await?;

    // 2. Get frame tree to validate frame_id and resolve CDP frame ID
    let tree_result = cdp
        .send_command_pub(ws_url, "Page.getFrameTree", serde_json::json!({}))
        .await?;

    let tree: FrameTreeResult = serde_json::from_value(tree_result)
        .map_err(|e| format!("Failed to parse frame tree: {}", e))?;

    let cdp_frame_id = find_cdp_frame_id(&tree.frame_tree, frame_id)
        .ok_or_else(|| format!("Frame '{}' not found in frame tree", frame_id))?;

    debug!("Resolved frame_id '{}' -> CDP frame '{}'", frame_id, cdp_frame_id);

    // 3. Create an isolated world for the frame — returns executionContextId directly.
    //    We use grantUniveralAccess so the context can access the frame's DOM.
    let world_result = cdp
        .send_command_pub(
            ws_url,
            "Page.createIsolatedWorld",
            serde_json::json!({
                "frameId": cdp_frame_id,
                "worldName": "_ki_frame_ctx",
                "grantUniveralAccess": true
            }),
        )
        .await?;

    let context_id = world_result
        .get("executionContextId")
        .and_then(|v| v.as_i64())
        .ok_or_else(|| "No executionContextId in createIsolatedWorld response".to_string())?;

    debug!("Frame '{}' -> executionContextId {}", frame_id, context_id);
    Ok(context_id)
}

/// Evaluate JavaScript in a specific frame identified by `frame_id`.
///
/// Uses `resolve_frame_execution_context` to get the context ID, then calls
/// `Runtime.evaluate` with `contextId` to execute in that frame.
pub async fn evaluate_in_frame(
    cdp: &CdpClient,
    ws_url: &str,
    frame_id: &str,
    script: &str,
    await_promise: bool,
) -> Result<String, String> {
    let context_id = resolve_frame_execution_context(cdp, ws_url, frame_id).await?;

    let params = serde_json::json!({
        "expression": script,
        "contextId": context_id,
        "returnByValue": true,
        "awaitPromise": await_promise,
        "userGesture": true
    });

    let result = cdp
        .send_command_pub(ws_url, "Runtime.evaluate", params)
        .await?;

    // Check for exceptions
    if let Some(exception) = result.get("exceptionDetails") {
        let text = exception
            .get("text")
            .and_then(|t| t.as_str())
            .unwrap_or("Unknown error");
        let desc = exception
            .get("exception")
            .and_then(|e| e.get("description"))
            .and_then(|d| d.as_str())
            .unwrap_or(text);
        return Err(format!("JS exception in frame '{}': {}", frame_id, desc));
    }

    // Extract the result value (same logic as CdpClient::evaluate)
    if let Some(val) = result.get("result").and_then(|r| r.get("value")) {
        match val {
            serde_json::Value::String(s) => Ok(s.clone()),
            serde_json::Value::Null => Ok("null".to_string()),
            other => Ok(other.to_string()),
        }
    } else if let Some(val) = result.get("result") {
        let type_str = val
            .get("type")
            .and_then(|t| t.as_str())
            .unwrap_or("unknown");
        if type_str == "undefined" {
            Ok("undefined".to_string())
        } else {
            Ok(val.to_string())
        }
    } else {
        Ok(result.to_string())
    }
}

/// Click an element inside a specific frame by selector.
///
/// Evaluates JS in the frame to find the element's bounding rect, then
/// returns the coordinates so the caller can issue a click at the right position.
pub async fn get_element_center_in_frame(
    cdp: &CdpClient,
    ws_url: &str,
    frame_id: &str,
    selector: &str,
) -> Result<(i32, i32), String> {
    let escaped = selector.replace('\\', "\\\\").replace('\'', "\\'");
    let js = format!(
        r#"(function(){{var el=document.querySelector('{}');if(!el)return null;var r=el.getBoundingClientRect();return JSON.stringify({{x:r.x+r.width/2,y:r.y+r.height/2}})}})()"#,
        escaped
    );

    let result = evaluate_in_frame(cdp, ws_url, frame_id, &js, false).await?;

    if result == "null" || result.is_empty() {
        return Err(format!(
            "Element '{}' not found in frame '{}'",
            selector, frame_id
        ));
    }

    let coords: serde_json::Value = serde_json::from_str(&result)
        .map_err(|e| format!("Failed to parse element coords: {}", e))?;

    let x = coords["x"]
        .as_f64()
        .ok_or("Missing x coordinate")? as i32;
    let y = coords["y"]
        .as_f64()
        .ok_or("Missing y coordinate")? as i32;

    Ok((x, y))
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    fn make_tree(main_id: &str, children: Vec<(&str, &str)>) -> FrameTreeNode {
        FrameTreeNode {
            frame: CdpFrame {
                id: main_id.to_string(),
                url: "https://example.com".to_string(),
                name: None,
            },
            child_frames: children
                .into_iter()
                .map(|(id, name)| FrameTreeNode {
                    frame: CdpFrame {
                        id: id.to_string(),
                        url: format!("https://example.com/{}", name),
                        name: Some(name.to_string()),
                    },
                    child_frames: vec![],
                })
                .collect(),
        }
    }

    #[test]
    fn test_find_cdp_frame_id_direct_match() {
        let tree = make_tree("MAIN123", vec![("CHILD456", "login-frame")]);
        assert_eq!(
            find_cdp_frame_id(&tree, "CHILD456"),
            Some("CHILD456".to_string())
        );
    }

    #[test]
    fn test_find_cdp_frame_id_main_alias() {
        let tree = make_tree("MAIN123", vec![("CHILD456", "login-frame")]);
        assert_eq!(
            find_cdp_frame_id(&tree, "main"),
            Some("MAIN123".to_string())
        );
    }

    #[test]
    fn test_find_cdp_frame_id_frame_index() {
        let tree = make_tree(
            "MAIN",
            vec![("C1", "first"), ("C2", "second")],
        );
        assert_eq!(find_cdp_frame_id(&tree, "frame-0"), Some("C1".to_string()));
        assert_eq!(find_cdp_frame_id(&tree, "frame-1"), Some("C2".to_string()));
        assert_eq!(find_cdp_frame_id(&tree, "frame-2"), None);
    }

    #[test]
    fn test_find_cdp_frame_id_by_name() {
        let tree = make_tree("MAIN", vec![("C1", "login-frame")]);
        assert_eq!(
            find_cdp_frame_id(&tree, "login-frame"),
            Some("C1".to_string())
        );
    }

    #[test]
    fn test_find_cdp_frame_id_not_found() {
        let tree = make_tree("MAIN", vec![]);
        assert_eq!(find_cdp_frame_id(&tree, "nonexistent"), None);
    }
}
