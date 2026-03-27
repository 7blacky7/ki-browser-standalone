//! Network request interception and capture endpoints.
//!
//! Provides REST endpoints to start/stop network traffic capture in a browser tab
//! and to retrieve or clear the recorded entries. Capture is performed by injecting
//! JavaScript into the page that monkey-patches `fetch` and `XMLHttpRequest` and
//! attaches a `PerformanceObserver` for resource-timing events.

use axum::{
    extract::{Query, State},
    http::StatusCode,
    response::IntoResponse,
    routing::{delete, get, post},
    Json, Router,
};
use serde::{Deserialize, Serialize};

use crate::api::debug_routes::types::{evaluate_in_tab, resolve_tab_id, TabQuery};
use crate::api::routes::ApiResponse;
use crate::api::server::AppState;

// ============================================================================
// Types
// ============================================================================

/// A single captured network request entry.
#[derive(Debug, Serialize, Deserialize)]
pub struct NetworkEntry {
    pub url: String,
    pub method: String,
    #[serde(default)]
    pub status: Option<u16>,
    pub resource_type: String,
    #[serde(default)]
    pub duration_ms: Option<f64>,
    #[serde(default)]
    pub transfer_size: Option<u64>,
    #[serde(default)]
    pub started_at: Option<f64>,
    #[serde(default)]
    pub initiator_type: Option<String>,
}

/// Response for listing captured network entries.
#[derive(Debug, Serialize, Deserialize)]
pub struct NetworkEntriesResponse {
    pub entries: Vec<NetworkEntry>,
    pub count: usize,
    pub capturing: bool,
}

// ============================================================================
// Query Parameters
// ============================================================================

/// Query parameters for `GET /debug/network/entries`.
#[derive(Debug, Deserialize)]
pub struct NetworkEntriesQuery {
    #[serde(default)]
    pub tab_id: Option<String>,
    #[serde(default)]
    pub resource_type: Option<String>,
    #[serde(default)]
    pub url_contains: Option<String>,
    #[serde(default = "default_limit")]
    pub limit: usize,
}

fn default_limit() -> usize {
    200
}

// ============================================================================
// Request Bodies
// ============================================================================

/// Request body for `POST /debug/network/start`.
#[derive(Debug, Deserialize)]
pub struct StartCaptureRequest {
    #[serde(default)]
    pub tab_id: Option<String>,
    #[serde(default)]
    pub capture_bodies: Option<bool>,
}

/// Request body for `POST /debug/network/stop`.
#[derive(Debug, Deserialize)]
pub struct StopCaptureRequest {
    #[serde(default)]
    pub tab_id: Option<String>,
}

// ============================================================================
// JavaScript snippets
// ============================================================================

/// JavaScript injected by `start_capture` — installs PerformanceObserver,
/// fetch and XHR monkey-patches, and initialises the entry buffer.
const CAPTURE_SCRIPT: &str = r#"(function(){
  if(window.__ki_network_capturing) return JSON.stringify({started: false, reason: 'already_capturing'});
  window.__ki_network_entries = window.__ki_network_entries || [];
  window.__ki_network_capturing = true;

  // PerformanceObserver for resource timing
  try {
    var obs = new PerformanceObserver(function(list){
      list.getEntries().forEach(function(e){
        window.__ki_network_entries.push({
          url: e.name, method: 'GET', status: null,
          resource_type: e.initiatorType, duration_ms: e.duration,
          transfer_size: e.transferSize, started_at: e.startTime,
          initiator_type: e.initiatorType
        });
      });
    });
    obs.observe({entryTypes: ['resource']});
  } catch(e){}

  // fetch() monkey-patch
  var origFetch = window.fetch;
  window.fetch = function(url, opts){
    var method = (opts && opts.method) || 'GET';
    var startTime = performance.now();
    return origFetch.apply(this, arguments).then(function(resp){
      if(window.__ki_network_capturing){
        window.__ki_network_entries.push({
          url: typeof url === 'string' ? url : url.url,
          method: method, status: resp.status,
          resource_type: 'fetch', duration_ms: performance.now() - startTime,
          transfer_size: null, started_at: startTime, initiator_type: 'fetch'
        });
      }
      return resp;
    });
  };

  // XHR monkey-patch
  var origOpen = XMLHttpRequest.prototype.open;
  var origSend = XMLHttpRequest.prototype.send;
  XMLHttpRequest.prototype.open = function(method, url){
    this.__ki_method = method;
    this.__ki_url = url;
    this.__ki_start = performance.now();
    return origOpen.apply(this, arguments);
  };
  XMLHttpRequest.prototype.send = function(){
    var xhr = this;
    xhr.addEventListener('loadend', function(){
      if(window.__ki_network_capturing){
        window.__ki_network_entries.push({
          url: xhr.__ki_url, method: xhr.__ki_method, status: xhr.status,
          resource_type: 'xmlhttprequest', duration_ms: performance.now() - xhr.__ki_start,
          transfer_size: null, started_at: xhr.__ki_start, initiator_type: 'xmlhttprequest'
        });
      }
    });
    return origSend.apply(this, arguments);
  };

  return JSON.stringify({started: true});
})()"#;

/// JavaScript snippet to stop capturing.
const STOP_SCRIPT: &str =
    r#"(function(){ window.__ki_network_capturing = false; return JSON.stringify({stopped: true}); })()"#;

/// JavaScript snippet to retrieve all entries and current capturing flag.
const ENTRIES_SCRIPT: &str = r#"(function(){
  var entries = window.__ki_network_entries || [];
  var capturing = window.__ki_network_capturing || false;
  return JSON.stringify({entries: entries, count: entries.length, capturing: capturing});
})()"#;

/// JavaScript snippet to clear the entry buffer.
const CLEAR_SCRIPT: &str =
    r#"(function(){ window.__ki_network_entries = []; return JSON.stringify({cleared: true}); })()"#;

// ============================================================================
// Handlers
// ============================================================================

/// POST /debug/network/start — Start network capture in the given tab.
async fn start_capture(
    State(state): State<AppState>,
    Json(req): Json<StartCaptureRequest>,
) -> impl IntoResponse {
    let resolved = match resolve_tab_id(&state, req.tab_id).await {
        Some(id) => id,
        None => {
            return (
                StatusCode::BAD_REQUEST,
                Json(ApiResponse::<()>::error("No active tab found")),
            )
                .into_response();
        }
    };

    match evaluate_in_tab(&state, &resolved, CAPTURE_SCRIPT).await {
        Ok(_) => Json(ApiResponse::<()>::success(())).into_response(),
        Err(err) => err.into_response(),
    }
}

/// POST /debug/network/stop — Stop network capture in the given tab.
async fn stop_capture(
    State(state): State<AppState>,
    Json(req): Json<StopCaptureRequest>,
) -> impl IntoResponse {
    let resolved = match resolve_tab_id(&state, req.tab_id).await {
        Some(id) => id,
        None => {
            return (
                StatusCode::BAD_REQUEST,
                Json(ApiResponse::<()>::error("No active tab found")),
            )
                .into_response();
        }
    };

    match evaluate_in_tab(&state, &resolved, STOP_SCRIPT).await {
        Ok(_) => Json(ApiResponse::<()>::success(())).into_response(),
        Err(err) => err.into_response(),
    }
}

/// GET /debug/network/entries?tab_id=&resource_type=&url_contains=&limit= — Retrieve captured entries.
async fn get_entries(
    State(state): State<AppState>,
    Query(query): Query<NetworkEntriesQuery>,
) -> impl IntoResponse {
    let resolved = match resolve_tab_id(&state, query.tab_id).await {
        Some(id) => id,
        None => {
            return (
                StatusCode::BAD_REQUEST,
                Json(ApiResponse::<NetworkEntriesResponse>::error("No active tab found")),
            )
                .into_response();
        }
    };

    match evaluate_in_tab(&state, &resolved, ENTRIES_SCRIPT).await {
        Ok(raw) => {
            match serde_json::from_str::<NetworkEntriesResponse>(&raw) {
                Ok(mut resp) => {
                    // Apply optional client-side filters
                    if let Some(rt) = &query.resource_type {
                        resp.entries
                            .retain(|e| e.resource_type.eq_ignore_ascii_case(rt));
                    }
                    if let Some(contains) = &query.url_contains {
                        resp.entries.retain(|e| e.url.contains(contains.as_str()));
                    }
                    // Apply limit
                    let limit = query.limit;
                    if resp.entries.len() > limit {
                        let skip = resp.entries.len() - limit;
                        resp.entries = resp.entries.into_iter().skip(skip).collect();
                    }
                    resp.count = resp.entries.len();

                    Json(ApiResponse::success(resp)).into_response()
                }
                Err(e) => (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    Json(ApiResponse::<NetworkEntriesResponse>::error(format!(
                        "Failed to parse network entries: {}",
                        e
                    ))),
                )
                    .into_response(),
            }
        }
        Err(err) => err.into_response(),
    }
}

/// DELETE /debug/network/entries?tab_id= — Clear all captured entries.
async fn clear_entries(
    State(state): State<AppState>,
    Query(query): Query<TabQuery>,
) -> impl IntoResponse {
    let resolved = match resolve_tab_id(&state, query.tab_id).await {
        Some(id) => id,
        None => {
            return (
                StatusCode::BAD_REQUEST,
                Json(ApiResponse::<()>::error("No active tab found")),
            )
                .into_response();
        }
    };

    match evaluate_in_tab(&state, &resolved, CLEAR_SCRIPT).await {
        Ok(_) => Json(ApiResponse::<()>::success(())).into_response(),
        Err(err) => err.into_response(),
    }
}

// ============================================================================
// Router
// ============================================================================

/// Creates the router fragment for network capture endpoints.
pub fn network_routes() -> Router<AppState> {
    Router::new()
        .route("/debug/network/start", post(start_capture))
        .route("/debug/network/stop", post(stop_capture))
        .route(
            "/debug/network/entries",
            get(get_entries).delete(clear_entries),
        )
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_network_entry_serialization() {
        let entry = NetworkEntry {
            url: "https://example.com/api".to_string(),
            method: "POST".to_string(),
            status: Some(200),
            resource_type: "fetch".to_string(),
            duration_ms: Some(42.5),
            transfer_size: Some(1024),
            started_at: Some(1000.0),
            initiator_type: Some("fetch".to_string()),
        };
        let json = serde_json::to_string(&entry).expect("serialize");
        assert!(json.contains("\"url\":\"https://example.com/api\""));
        assert!(json.contains("\"method\":\"POST\""));
        assert!(json.contains("\"status\":200"));
        assert!(json.contains("\"resource_type\":\"fetch\""));
    }

    #[test]
    fn test_network_entry_deserialization() {
        let json = r#"{
            "url": "https://api.example.com/data",
            "method": "GET",
            "status": 404,
            "resource_type": "xmlhttprequest",
            "duration_ms": 123.4,
            "transfer_size": null,
            "started_at": 500.0,
            "initiator_type": "xmlhttprequest"
        }"#;
        let entry: NetworkEntry = serde_json::from_str(json).expect("deserialize");
        assert_eq!(entry.url, "https://api.example.com/data");
        assert_eq!(entry.status, Some(404));
        assert_eq!(entry.resource_type, "xmlhttprequest");
    }

    #[test]
    fn test_network_entry_optional_fields_null() {
        let json = r#"{"url":"x","method":"GET","resource_type":"script"}"#;
        let entry: NetworkEntry = serde_json::from_str(json).expect("deserialize");
        assert!(entry.status.is_none());
        assert!(entry.duration_ms.is_none());
        assert!(entry.transfer_size.is_none());
        assert!(entry.started_at.is_none());
        assert!(entry.initiator_type.is_none());
    }

    #[test]
    fn test_network_entries_response_serialization() {
        let resp = NetworkEntriesResponse {
            entries: vec![],
            count: 0,
            capturing: true,
        };
        let json = serde_json::to_string(&resp).expect("serialize");
        assert!(json.contains("\"count\":0"));
        assert!(json.contains("\"capturing\":true"));
        assert!(json.contains("\"entries\":[]"));
    }

    #[test]
    fn test_network_entries_response_deserialization() {
        let json = r#"{"entries":[],"count":0,"capturing":false}"#;
        let resp: NetworkEntriesResponse = serde_json::from_str(json).expect("deserialize");
        assert!(!resp.capturing);
        assert_eq!(resp.count, 0);
    }

    #[test]
    fn test_start_capture_request_deserialization_defaults() {
        let json = r#"{}"#;
        let req: StartCaptureRequest = serde_json::from_str(json).expect("deserialize");
        assert!(req.tab_id.is_none());
        assert!(req.capture_bodies.is_none());
    }

    #[test]
    fn test_start_capture_request_deserialization_full() {
        let json = r#"{"tab_id":"tab-1","capture_bodies":true}"#;
        let req: StartCaptureRequest = serde_json::from_str(json).expect("deserialize");
        assert_eq!(req.tab_id.as_deref(), Some("tab-1"));
        assert_eq!(req.capture_bodies, Some(true));
    }

    #[test]
    fn test_stop_capture_request_deserialization() {
        let json = r#"{"tab_id":"tab-2"}"#;
        let req: StopCaptureRequest = serde_json::from_str(json).expect("deserialize");
        assert_eq!(req.tab_id.as_deref(), Some("tab-2"));
    }

    #[test]
    fn test_network_entries_query_defaults() {
        // Simulate serde_qs / axum Query parsing with all-defaults
        let json = r#"{}"#;
        // Use serde_json as a proxy since axum Query uses serde internally
        let q: NetworkEntriesQuery = serde_json::from_str(json).expect("deserialize");
        assert!(q.tab_id.is_none());
        assert!(q.resource_type.is_none());
        assert!(q.url_contains.is_none());
        assert_eq!(q.limit, 200);
    }

    #[test]
    fn test_limit_filter_applied() {
        let mut resp = NetworkEntriesResponse {
            entries: (0..10)
                .map(|i| NetworkEntry {
                    url: format!("https://example.com/{}", i),
                    method: "GET".to_string(),
                    status: Some(200),
                    resource_type: "fetch".to_string(),
                    duration_ms: None,
                    transfer_size: None,
                    started_at: None,
                    initiator_type: None,
                })
                .collect(),
            count: 10,
            capturing: false,
        };

        // Simulate the limit logic used in get_entries
        let limit = 3_usize;
        if resp.entries.len() > limit {
            let skip = resp.entries.len() - limit;
            resp.entries = resp.entries.into_iter().skip(skip).collect();
        }
        resp.count = resp.entries.len();

        assert_eq!(resp.count, 3);
        // Should contain the last 3 entries
        assert_eq!(resp.entries[0].url, "https://example.com/7");
    }
}
