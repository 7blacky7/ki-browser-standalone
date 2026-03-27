//! Performance monitoring route handlers.
//!
//! Provides Axum route handlers that expose browser performance metrics via
//! the Performance API. Each handler evaluates JavaScript in the target tab
//! and returns structured JSON results.
//!
//! ## Endpoints
//! - `GET /debug/performance/timing`   – Navigation timing breakdown
//! - `GET /debug/performance/resources` – Resource timing entries
//! - `GET /debug/performance/vitals`   – Core Web Vitals (LCP, FCP, CLS, TTFB)
//! - `GET /debug/performance/memory`   – JS heap memory (Chrome-only)

use axum::{
    extract::{Query, State},
    http::StatusCode,
    response::IntoResponse,
    routing::get,
    Json, Router,
};
use serde::{Deserialize, Serialize};

use crate::api::debug_routes::types::{resolve_tab_id, evaluate_in_tab, TabQuery};
use crate::api::routes::ApiResponse;
use crate::api::server::AppState;

// ============================================================================
// Response Structs
// ============================================================================

/// Computed navigation timing metrics derived from the PerformanceNavigationTiming entry.
#[derive(Debug, Serialize, Deserialize)]
pub struct TimingResponse {
    /// Redirect duration in milliseconds.
    pub redirect_time_ms: f64,
    /// DNS lookup duration in milliseconds.
    pub dns_time_ms: f64,
    /// TCP connection duration in milliseconds.
    pub connect_time_ms: f64,
    /// Time to First Byte in milliseconds.
    pub ttfb_ms: f64,
    /// Response download duration in milliseconds.
    pub response_time_ms: f64,
    /// Time until DOM became interactive in milliseconds.
    pub dom_interactive_ms: f64,
    /// Time until DOMContentLoaded fired in milliseconds.
    pub dom_content_loaded_ms: f64,
    /// Time until load event fired in milliseconds.
    pub load_complete_ms: f64,
    /// Raw PerformanceNavigationTiming entry as returned by the browser.
    pub raw: serde_json::Value,
}

/// A single resource timing entry.
#[derive(Debug, Serialize, Deserialize)]
pub struct ResourceEntry {
    pub name: String,
    pub entry_type: String,
    pub start_time: f64,
    pub duration: f64,
    pub transfer_size: f64,
    pub encoded_body_size: f64,
    pub decoded_body_size: f64,
    pub initiator_type: String,
}

/// Response containing all captured resource timing entries.
#[derive(Debug, Serialize, Deserialize)]
pub struct ResourcesResponse {
    pub count: usize,
    pub entries: Vec<ResourceEntry>,
}

/// Core Web Vitals collected from the browser.
#[derive(Debug, Serialize, Deserialize)]
pub struct VitalsResponse {
    /// Largest Contentful Paint in milliseconds (`null` if not yet available).
    pub lcp_ms: Option<f64>,
    /// First Contentful Paint in milliseconds (`null` if not yet available).
    pub fcp_ms: Option<f64>,
    /// Cumulative Layout Shift score (dimensionless, sum of all layout shifts).
    pub cls: f64,
    /// Time to First Byte in milliseconds derived from navigation timing.
    pub ttfb_ms: Option<f64>,
}

/// JS heap memory snapshot (Chrome/Chromium only via `performance.memory`).
#[derive(Debug, Serialize, Deserialize)]
pub struct MemoryResponse {
    /// Currently used JS heap memory in bytes.
    pub used_js_heap_size: f64,
    /// Total allocated JS heap memory in bytes.
    pub total_js_heap_size: f64,
    /// Maximum JS heap size limit in bytes.
    pub js_heap_size_limit: f64,
}

// ============================================================================
// Query Structs
// ============================================================================

/// Query parameters for the resources endpoint.
#[derive(Debug, Deserialize)]
pub struct ResourcesQuery {
    /// Target tab ID. If omitted, uses the active tab.
    #[serde(default)]
    pub tab_id: Option<String>,
    /// Maximum number of resource entries to return (default: 100).
    #[serde(default = "default_resource_limit")]
    pub limit: usize,
}

fn default_resource_limit() -> usize {
    100
}

// ============================================================================
// Handlers
// ============================================================================

/// `GET /debug/performance/timing`
///
/// Returns a breakdown of navigation timing metrics for the current page,
/// computed from `performance.getEntriesByType('navigation')[0]`.
async fn timing(
    State(state): State<AppState>,
    Query(query): Query<TabQuery>,
) -> impl IntoResponse {
    let tab_id = match resolve_tab_id(&state, query.tab_id).await {
        Some(id) => id,
        None => {
            return (
                StatusCode::BAD_REQUEST,
                Json(ApiResponse::<()>::error("No active tab")),
            )
                .into_response();
        }
    };

    let script = r#"
        JSON.stringify((function() {
            var entries = performance.getEntriesByType('navigation');
            if (!entries || entries.length === 0) {
                return null;
            }
            var e = entries[0];
            return {
                redirect_time_ms:       e.redirectEnd   - e.redirectStart,
                dns_time_ms:            e.domainLookupEnd - e.domainLookupStart,
                connect_time_ms:        e.connectEnd    - e.connectStart,
                ttfb_ms:                e.responseStart - e.requestStart,
                response_time_ms:       e.responseEnd   - e.responseStart,
                dom_interactive_ms:     e.domInteractive,
                dom_content_loaded_ms:  e.domContentLoadedEventEnd,
                load_complete_ms:       e.loadEventEnd,
                raw: {
                    name:                    e.name,
                    entryType:               e.entryType,
                    startTime:               e.startTime,
                    duration:                e.duration,
                    initiatorType:           e.initiatorType,
                    redirectStart:           e.redirectStart,
                    redirectEnd:             e.redirectEnd,
                    fetchStart:              e.fetchStart,
                    domainLookupStart:       e.domainLookupStart,
                    domainLookupEnd:         e.domainLookupEnd,
                    connectStart:            e.connectStart,
                    connectEnd:              e.connectEnd,
                    secureConnectionStart:   e.secureConnectionStart,
                    requestStart:            e.requestStart,
                    responseStart:           e.responseStart,
                    responseEnd:             e.responseEnd,
                    transferSize:            e.transferSize,
                    encodedBodySize:         e.encodedBodySize,
                    decodedBodySize:         e.decodedBodySize,
                    domInteractive:          e.domInteractive,
                    domContentLoadedEventStart: e.domContentLoadedEventStart,
                    domContentLoadedEventEnd:   e.domContentLoadedEventEnd,
                    domComplete:             e.domComplete,
                    loadEventStart:          e.loadEventStart,
                    loadEventEnd:            e.loadEventEnd,
                    type:                    e.type,
                    redirectCount:           e.redirectCount
                }
            };
        })())
    "#;

    match evaluate_in_tab(&state, &tab_id, script).await {
        Ok(json_str) => match serde_json::from_str::<TimingResponse>(&json_str) {
            Ok(data) => Json(ApiResponse::success(data)).into_response(),
            Err(e) => (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ApiResponse::<()>::error(format!("Parse error: {}", e))),
            )
                .into_response(),
        },
        Err(err_response) => err_response.into_response(),
    }
}

/// `GET /debug/performance/resources`
///
/// Returns resource timing entries for all sub-resources loaded by the current
/// page. Use the `limit` query parameter to cap the number of returned entries
/// (default: 100).
async fn resources(
    State(state): State<AppState>,
    Query(query): Query<ResourcesQuery>,
) -> impl IntoResponse {
    let tab_id = match resolve_tab_id(&state, query.tab_id).await {
        Some(id) => id,
        None => {
            return (
                StatusCode::BAD_REQUEST,
                Json(ApiResponse::<()>::error("No active tab")),
            )
                .into_response();
        }
    };

    let limit = query.limit;
    let script = format!(
        r#"
        JSON.stringify((function() {{
            var entries = performance.getEntriesByType('resource').slice(0, {limit});
            return entries.map(function(e) {{
                return {{
                    name:             e.name,
                    entry_type:       e.entryType,
                    start_time:       e.startTime,
                    duration:         e.duration,
                    transfer_size:    e.transferSize    || 0,
                    encoded_body_size: e.encodedBodySize || 0,
                    decoded_body_size: e.decodedBodySize || 0,
                    initiator_type:   e.initiatorType
                }};
            }});
        }})())
        "#,
        limit = limit
    );

    match evaluate_in_tab(&state, &tab_id, &script).await {
        Ok(json_str) => match serde_json::from_str::<Vec<ResourceEntry>>(&json_str) {
            Ok(entries) => {
                let count = entries.len();
                let response = ResourcesResponse { count, entries };
                Json(ApiResponse::success(response)).into_response()
            }
            Err(e) => (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ApiResponse::<()>::error(format!("Parse error: {}", e))),
            )
                .into_response(),
        },
        Err(err_response) => err_response.into_response(),
    }
}

/// `GET /debug/performance/vitals`
///
/// Collects Core Web Vitals:
/// - **LCP** (Largest Contentful Paint) from `largest-contentful-paint` entries
/// - **FCP** (First Contentful Paint) from `paint` entries
/// - **CLS** (Cumulative Layout Shift) summed from `layout-shift` entries
/// - **TTFB** (Time to First Byte) from navigation timing
async fn vitals(
    State(state): State<AppState>,
    Query(query): Query<TabQuery>,
) -> impl IntoResponse {
    let tab_id = match resolve_tab_id(&state, query.tab_id).await {
        Some(id) => id,
        None => {
            return (
                StatusCode::BAD_REQUEST,
                Json(ApiResponse::<()>::error("No active tab")),
            )
                .into_response();
        }
    };

    let script = r#"
        JSON.stringify((function() {
            // LCP – use the last (largest) entry if multiple exist
            var lcpEntries = performance.getEntriesByType('largest-contentful-paint');
            var lcp_ms = lcpEntries.length > 0
                ? lcpEntries[lcpEntries.length - 1].startTime
                : null;

            // FCP – first paint entry with name 'first-contentful-paint'
            var paintEntries = performance.getEntriesByType('paint');
            var fcpEntry = paintEntries.filter(function(e) {
                return e.name === 'first-contentful-paint';
            });
            var fcp_ms = fcpEntry.length > 0 ? fcpEntry[0].startTime : null;

            // CLS – sum of all layout-shift values
            var layoutShifts = performance.getEntriesByType('layout-shift');
            var cls = layoutShifts.reduce(function(sum, e) {
                return sum + (e.value || 0);
            }, 0);

            // TTFB from navigation timing
            var navEntries = performance.getEntriesByType('navigation');
            var ttfb_ms = navEntries.length > 0
                ? navEntries[0].responseStart - navEntries[0].requestStart
                : null;

            return {
                lcp_ms:  lcp_ms,
                fcp_ms:  fcp_ms,
                cls:     cls,
                ttfb_ms: ttfb_ms
            };
        })())
    "#;

    match evaluate_in_tab(&state, &tab_id, script).await {
        Ok(json_str) => match serde_json::from_str::<VitalsResponse>(&json_str) {
            Ok(data) => Json(ApiResponse::success(data)).into_response(),
            Err(e) => (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ApiResponse::<()>::error(format!("Parse error: {}", e))),
            )
                .into_response(),
        },
        Err(err_response) => err_response.into_response(),
    }
}

/// `GET /debug/performance/memory`
///
/// Returns JS heap memory statistics from `performance.memory`.
/// This is a non-standard, Chrome/Chromium-only API and may not be available
/// in all browsers or contexts.
async fn memory(
    State(state): State<AppState>,
    Query(query): Query<TabQuery>,
) -> impl IntoResponse {
    let tab_id = match resolve_tab_id(&state, query.tab_id).await {
        Some(id) => id,
        None => {
            return (
                StatusCode::BAD_REQUEST,
                Json(ApiResponse::<()>::error("No active tab")),
            )
                .into_response();
        }
    };

    let script = r#"
        JSON.stringify((function() {
            var mem = performance.memory;
            if (!mem) {
                return null;
            }
            return {
                used_js_heap_size:  mem.usedJSHeapSize,
                total_js_heap_size: mem.totalJSHeapSize,
                js_heap_size_limit: mem.jsHeapSizeLimit
            };
        })())
    "#;

    match evaluate_in_tab(&state, &tab_id, script).await {
        Ok(json_str) => match serde_json::from_str::<MemoryResponse>(&json_str) {
            Ok(data) => Json(ApiResponse::success(data)).into_response(),
            Err(e) => (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ApiResponse::<()>::error(format!("Parse error: {}", e))),
            )
                .into_response(),
        },
        Err(err_response) => err_response.into_response(),
    }
}

// ============================================================================
// Router
// ============================================================================

/// Build the performance debug sub-router.
///
/// Mount this via `.merge(performance_routes())` in the main router.
pub fn performance_routes() -> Router<AppState> {
    Router::new()
        .route("/debug/performance/timing", get(timing))
        .route("/debug/performance/resources", get(resources))
        .route("/debug/performance/vitals", get(vitals))
        .route("/debug/performance/memory", get(memory))
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_timing_response_serialization() {
        let resp = TimingResponse {
            redirect_time_ms: 0.0,
            dns_time_ms: 12.5,
            connect_time_ms: 45.0,
            ttfb_ms: 120.0,
            response_time_ms: 30.0,
            dom_interactive_ms: 350.0,
            dom_content_loaded_ms: 400.0,
            load_complete_ms: 600.0,
            raw: serde_json::json!({ "name": "https://example.com", "entryType": "navigation" }),
        };

        let json = serde_json::to_string(&resp).expect("serialization should succeed");
        assert!(json.contains("\"redirect_time_ms\""));
        assert!(json.contains("\"dns_time_ms\""));
        assert!(json.contains("\"ttfb_ms\""));
        assert!(json.contains("\"raw\""));
    }

    #[test]
    fn test_resource_entry_serialization() {
        let entry = ResourceEntry {
            name: "https://example.com/style.css".to_string(),
            entry_type: "resource".to_string(),
            start_time: 50.0,
            duration: 20.0,
            transfer_size: 1024.0,
            encoded_body_size: 900.0,
            decoded_body_size: 3600.0,
            initiator_type: "link".to_string(),
        };

        let json = serde_json::to_string(&entry).expect("serialization should succeed");
        assert!(json.contains("\"name\""));
        assert!(json.contains("\"initiator_type\""));
        assert!(json.contains("\"transfer_size\""));
    }

    #[test]
    fn test_resources_response_serialization() {
        let resp = ResourcesResponse {
            count: 1,
            entries: vec![ResourceEntry {
                name: "https://example.com/app.js".to_string(),
                entry_type: "resource".to_string(),
                start_time: 100.0,
                duration: 80.0,
                transfer_size: 4096.0,
                encoded_body_size: 4000.0,
                decoded_body_size: 12000.0,
                initiator_type: "script".to_string(),
            }],
        };

        let json = serde_json::to_string(&resp).expect("serialization should succeed");
        assert!(json.contains("\"count\":1"));
        assert!(json.contains("\"entries\""));
    }

    #[test]
    fn test_vitals_response_serialization_with_nulls() {
        let resp = VitalsResponse {
            lcp_ms: None,
            fcp_ms: Some(450.0),
            cls: 0.05,
            ttfb_ms: Some(95.0),
        };

        let json = serde_json::to_string(&resp).expect("serialization should succeed");
        assert!(json.contains("\"lcp_ms\":null"));
        assert!(json.contains("\"fcp_ms\":450.0"));
        assert!(json.contains("\"cls\":0.05"));
        assert!(json.contains("\"ttfb_ms\":95.0"));
    }

    #[test]
    fn test_memory_response_serialization() {
        let resp = MemoryResponse {
            used_js_heap_size: 10_000_000.0,
            total_js_heap_size: 20_000_000.0,
            js_heap_size_limit: 2_147_483_648.0,
        };

        let json = serde_json::to_string(&resp).expect("serialization should succeed");
        assert!(json.contains("\"used_js_heap_size\""));
        assert!(json.contains("\"total_js_heap_size\""));
        assert!(json.contains("\"js_heap_size_limit\""));
    }

    #[test]
    fn test_resources_query_default_limit() {
        // Verify the default limit is applied when no value is provided via serde
        let json = r#"{}"#;
        let q: ResourcesQuery = serde_json::from_str(json).expect("deserialize should succeed");
        assert_eq!(q.limit, 100);
    }

    #[test]
    fn test_resources_query_custom_limit() {
        let json = r#"{"limit": 25}"#;
        let q: ResourcesQuery = serde_json::from_str(json).expect("deserialize should succeed");
        assert_eq!(q.limit, 25);
    }
}
