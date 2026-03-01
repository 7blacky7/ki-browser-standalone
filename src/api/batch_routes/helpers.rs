//! Helper functions for parsing IPC responses from batch and session operations.

use std::collections::HashMap;

use crate::api::session::{CookieInfo, TabSnapshot};

/// Unwrap the IPC response `{"result": <value>}` wrapper from EvaluateScript.
///
/// The browser handler wraps evaluate results inside `{"result": value}`.
/// This helper extracts the inner value, handling both string-encoded JSON
/// and pre-parsed values.
pub(super) fn unwrap_ipc_result(data: &serde_json::Value) -> Option<serde_json::Value> {
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
pub(super) fn parse_ipc_json_result(data: &serde_json::Value) -> Option<serde_json::Value> {
    let result = unwrap_ipc_result(data)?;
    match &result {
        serde_json::Value::String(s) => serde_json::from_str(s).ok(),
        serde_json::Value::Null => None,
        other => Some(other.clone()),
    }
}

/// Parse a list of `CookieInfo` from an IPC response data value.
///
/// The JS script returns a JSON string; the IPC response may wrap it
/// as a string value or as a parsed JSON value.
pub(super) fn parse_cookies_from_response(data: Option<serde_json::Value>) -> Vec<CookieInfo> {
    let Some(data) = data else {
        return Vec::new();
    };

    let parsed = parse_ipc_json_result(&data);
    match parsed {
        Some(val) => serde_json::from_value::<Vec<CookieInfo>>(val).unwrap_or_default(),
        None => Vec::new(),
    }
}

/// Parse a localStorage/sessionStorage map from an IPC response.
pub(super) fn parse_storage_from_response(data: Option<serde_json::Value>) -> HashMap<String, String> {
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
pub(super) fn parse_tab_snapshot(tab_id: &str, data: serde_json::Value) -> TabSnapshot {
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
