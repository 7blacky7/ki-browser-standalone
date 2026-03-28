//! Miscellaneous route handlers: health check, API toggle/status, and CDP
//! remote debugging info endpoints.

use axum::{
    extract::{Path, State},
    http::StatusCode,
    response::IntoResponse,
    Json,
};
use serde::Serialize;
use tracing::{info, warn};
use uuid::Uuid;

use crate::api::cdp_mapping::{CdpTargetInfo, CdpTargetLookupResponse, CdpTargetsResponse};
use crate::api::server::AppState;
use super::types::*;

/// GET /health - Health check endpoint
#[utoipa::path(
    get,
    path = "/health",
    tag = "health",
    responses(
        (status = 200, description = "Server is healthy", body = HealthResponse)
    )
)]
pub async fn health_check(State(state): State<AppState>) -> impl IntoResponse {
    let api_enabled = state.is_enabled().await;

    Json(ApiResponse::success(HealthResponse {
        status: "healthy".to_string(),
        version: env!("CARGO_PKG_VERSION").to_string(),
        api_enabled,
    }))
}

/// POST /api/toggle - Toggle API enabled state
#[utoipa::path(
    post,
    path = "/api/toggle",
    tag = "api",
    request_body = ApiToggleRequest,
    responses(
        (status = 200, description = "API state toggled", body = ApiStatusResponse)
    )
)]
pub async fn toggle_api(
    State(state): State<AppState>,
    Json(request): Json<ApiToggleRequest>,
) -> impl IntoResponse {
    state.set_enabled(request.enabled).await;

    info!("API {} by request", if request.enabled { "enabled" } else { "disabled" });

    Json(ApiResponse::success(ApiStatusResponse {
        enabled: request.enabled,
        port: 0, // Port info not available here
        connected_clients: state.ws_handler.client_count().await,
    }))
}

/// GET /api/status - Get current API status
#[utoipa::path(
    get,
    path = "/api/status",
    tag = "api",
    responses(
        (status = 200, description = "Current API status", body = ApiStatusResponse)
    )
)]
pub async fn api_status(State(state): State<AppState>) -> impl IntoResponse {
    let enabled = state.is_enabled().await;
    let connected_clients = state.ws_handler.client_count().await;

    Json(ApiResponse::success(ApiStatusResponse {
        enabled,
        port: 0, // Port info not available here
        connected_clients,
    }))
}

/// GET /cdp - Returns CDP remote debugging connection info for Playwright/DevTools integration
pub(crate) async fn cdp_info(
    State(state): State<AppState>,
) -> impl IntoResponse {
    let cdp_port = state.cdp_port.unwrap_or(9222);
    let base = format!("http://127.0.0.1:{}", cdp_port);
    Json(serde_json::json!({
        "success": true,
        "data": {
            "base_url": base,
            "json_list": format!("{}/json/list", base),
            "json_version": format!("{}/json/version", base),
            "ws_base": format!("ws://127.0.0.1:{}", cdp_port),
            "port": cdp_port
        }
    }))
}

/// GET /cdp/targets - List all CDP targets with their mapped ki-browser tab UUIDs.
///
/// Returns remote debugging connection info and all known tab-to-target mappings,
/// enabling external CDP clients to discover which WebSocket URL corresponds to
/// which ki-browser tab.
pub async fn cdp_targets(State(state): State<AppState>) -> impl IntoResponse {
    if !state.is_enabled().await {
        return (
            StatusCode::SERVICE_UNAVAILABLE,
            Json(ApiResponse::<CdpTargetsResponse>::error("API is disabled")),
        )
            .into_response();
    }

    let mapping = &state.cdp_mapping;
    let browser_state = state.browser_state.read().await;

    let targets: Vec<CdpTargetInfo> = mapping
        .all_mappings()
        .into_iter()
        .map(|(tab_uuid, target_id)| {
            let tab_id_str = tab_uuid.to_string();
            let (url, title) = browser_state
                .tabs
                .get(&tab_id_str)
                .map(|t| (t.url.clone(), t.title.clone()))
                .unwrap_or_else(|| ("unknown".to_string(), "Unknown".to_string()));

            CdpTargetInfo {
                tab_id: tab_id_str,
                target_id: target_id.clone(),
                target_type: "page".to_string(),
                ws_url: mapping.target_ws_url(&target_id),
                url,
                title,
            }
        })
        .collect();

    Json(ApiResponse::success(CdpTargetsResponse {
        remote_debugging_port: mapping.remote_debugging_port(),
        browser_ws_url: mapping.browser_ws_url(),
        targets,
    }))
    .into_response()
}

/// GET /cdp/target/:tab_id - Look up the CDP TargetId for a specific ki-browser tab UUID.
///
/// Returns the CDP target identifier and WebSocket URL for connecting to the
/// specified tab via Chrome DevTools Protocol.
pub async fn cdp_target_by_tab(
    State(state): State<AppState>,
    Path(tab_id): Path<String>,
) -> impl IntoResponse {
    if !state.is_enabled().await {
        return (
            StatusCode::SERVICE_UNAVAILABLE,
            Json(ApiResponse::<CdpTargetLookupResponse>::error("API is disabled")),
        )
            .into_response();
    }

    let uuid = match Uuid::parse_str(&tab_id) {
        Ok(u) => u,
        Err(_) => {
            return (
                StatusCode::BAD_REQUEST,
                Json(ApiResponse::<CdpTargetLookupResponse>::error(
                    "Invalid tab UUID format",
                )),
            )
                .into_response();
        }
    };

    let mapping = &state.cdp_mapping;

    match mapping.get_target_id(&uuid) {
        Some(target_id) => {
            let ws_url = mapping.target_ws_url(&target_id);
            Json(ApiResponse::success(CdpTargetLookupResponse {
                tab_id: tab_id.clone(),
                target_id,
                ws_url,
            }))
            .into_response()
        }
        None => {
            warn!("CDP target lookup failed: no mapping for tab {}", tab_id);
            (
                StatusCode::NOT_FOUND,
                Json(ApiResponse::<CdpTargetLookupResponse>::error(format!(
                    "No CDP target mapping found for tab: {}",
                    tab_id
                ))),
            )
                .into_response()
        }
    }
}

// ─── API Discovery ───────────────────────────────────────────────────────────

#[derive(Serialize)]
struct EndpointInfo {
    method: &'static str,
    path: &'static str,
    description: &'static str,
}

#[derive(Serialize)]
struct EndpointCategory {
    name: &'static str,
    endpoints: Vec<EndpointInfo>,
}

#[derive(Serialize)]
struct DocsInfo {
    swagger_ui: &'static str,
    openapi_json: &'static str,
}

#[derive(Serialize)]
struct EndpointsData {
    total: usize,
    categories: Vec<EndpointCategory>,
    docs: DocsInfo,
}

/// GET /api/endpoints - List all available API endpoints grouped by category
#[utoipa::path(
    get,
    path = "/api/endpoints",
    tag = "api",
    responses(
        (status = 200, description = "List of all available API endpoints")
    )
)]
pub async fn list_endpoints() -> impl IntoResponse {
    let categories = vec![
        EndpointCategory {
            name: "Health & Status",
            endpoints: vec![
                EndpointInfo { method: "GET", path: "/health", description: "Health check" },
                EndpointInfo { method: "GET", path: "/api/status", description: "API status (enabled, port, clients)" },
                EndpointInfo { method: "POST", path: "/api/toggle", description: "API aktivieren/deaktivieren" },
                EndpointInfo { method: "GET", path: "/api/endpoints", description: "Alle API-Endpoints auflisten (dieser Endpoint)" },
            ],
        },
        EndpointCategory {
            name: "Tabs",
            endpoints: vec![
                EndpointInfo { method: "GET", path: "/tabs", description: "Liste aller offenen Tabs" },
                EndpointInfo { method: "POST", path: "/tabs/new", description: "Neuen Tab erstellen (optional: url)" },
                EndpointInfo { method: "POST", path: "/tabs/close", description: "Tab schliessen (tab_id)" },
            ],
        },
        EndpointCategory {
            name: "Navigation & Interaction",
            endpoints: vec![
                EndpointInfo { method: "POST", path: "/navigate", description: "Zu URL navigieren (tab_id, url)" },
                EndpointInfo { method: "POST", path: "/click", description: "Klick auf Element oder Koordinaten (tab_id, selector|x+y)" },
                EndpointInfo { method: "POST", path: "/drag", description: "Drag-Operation (tab_id, from_x/y, to_x/y)" },
                EndpointInfo { method: "POST", path: "/type", description: "Text eingeben (tab_id, selector, text)" },
                EndpointInfo { method: "POST", path: "/evaluate", description: "JavaScript ausfuehren (tab_id, script)" },
                EndpointInfo { method: "POST", path: "/scroll", description: "Seite scrollen (tab_id, direction, amount)" },
                EndpointInfo { method: "GET", path: "/screenshot", description: "Screenshot als PNG/JPEG binary (?tab_id, ?format, ?raw=false fuer JSON)" },
                EndpointInfo { method: "GET", path: "/frames", description: "Frame-Baum abrufen (?tab_id)" },
            ],
        },
        EndpointCategory {
            name: "DOM",
            endpoints: vec![
                EndpointInfo { method: "GET", path: "/dom/element", description: "Element per CSS-Selector finden (?tab_id, ?selector)" },
                EndpointInfo { method: "POST", path: "/dom/annotate", description: "Interaktive Elemente annotieren (tab_id)" },
                EndpointInfo { method: "GET", path: "/dom/snapshot", description: "DOM Snapshot als JSON (?tab_id)" },
            ],
        },
        EndpointCategory {
            name: "DOM Extraction",
            endpoints: vec![
                EndpointInfo { method: "POST", path: "/dom/extract-structured-data", description: "Strukturierte Daten extrahieren (tab_id, schema)" },
                EndpointInfo { method: "POST", path: "/dom/extract-content", description: "Seiten-Inhalt extrahieren (tab_id)" },
                EndpointInfo { method: "POST", path: "/dom/analyze-structure", description: "Seiten-Struktur analysieren (tab_id)" },
                EndpointInfo { method: "POST", path: "/dom/forms", description: "Formulare erkennen (tab_id)" },
                EndpointInfo { method: "POST", path: "/dom/fill-form", description: "Formular ausfuellen (tab_id, form_index, values)" },
                EndpointInfo { method: "POST", path: "/dom/validate-form", description: "Formular validieren (tab_id, form_index)" },
            ],
        },
        EndpointCategory {
            name: "Sessions & Cookies",
            endpoints: vec![
                EndpointInfo { method: "POST", path: "/session/start", description: "Neue Session starten" },
                EndpointInfo { method: "GET", path: "/session/list", description: "Alle Sessions auflisten" },
                EndpointInfo { method: "GET", path: "/session/:id", description: "Session abrufen" },
                EndpointInfo { method: "DELETE", path: "/session/:id", description: "Session loeschen" },
                EndpointInfo { method: "POST", path: "/session/:id/storage", description: "Session Storage setzen" },
                EndpointInfo { method: "GET", path: "/session/:id/storage/:key", description: "Storage-Wert abrufen" },
                EndpointInfo { method: "POST", path: "/session/:id/snapshot", description: "Session Snapshot erstellen" },
                EndpointInfo { method: "GET", path: "/session/:id/snapshots", description: "Snapshots auflisten" },
                EndpointInfo { method: "GET", path: "/tabs/:tab_id/cookies", description: "Cookies eines Tabs abrufen" },
                EndpointInfo { method: "POST", path: "/tabs/:tab_id/cookies", description: "Cookies setzen" },
                EndpointInfo { method: "GET", path: "/tabs/:tab_id/local-storage", description: "Local Storage abrufen" },
            ],
        },
        EndpointCategory {
            name: "Multi-Agent",
            endpoints: vec![
                EndpointInfo { method: "POST", path: "/session/register", description: "Agent registrieren (agent_id)" },
                EndpointInfo { method: "POST", path: "/session/unregister", description: "Agent abmelden (agent_id)" },
                EndpointInfo { method: "GET", path: "/session/agents", description: "Registrierte Agenten auflisten" },
                EndpointInfo { method: "POST", path: "/tabs/:tab_id/claim", description: "Tab fuer Agent beanspruchen" },
                EndpointInfo { method: "POST", path: "/tabs/:tab_id/release", description: "Tab freigeben" },
            ],
        },
        EndpointCategory {
            name: "Batch",
            endpoints: vec![
                EndpointInfo { method: "POST", path: "/batch", description: "Mehrere Operationen in einem Request" },
                EndpointInfo { method: "POST", path: "/batch/navigate-and-extract", description: "Navigieren + Extrahieren kombiniert" },
            ],
        },
        EndpointCategory {
            name: "Vision & OCR",
            endpoints: vec![
                EndpointInfo { method: "GET", path: "/vision/annotated", description: "Annotierter Screenshot mit Overlay" },
                EndpointInfo { method: "GET", path: "/vision/labels", description: "Vision Labels" },
                EndpointInfo { method: "GET", path: "/ocr/engines", description: "Verfuegbare OCR-Engines" },
                EndpointInfo { method: "POST", path: "/ocr/run", description: "OCR ausfuehren (tab_id, ?engine)" },
            ],
        },
        EndpointCategory {
            name: "GUI",
            endpoints: vec![
                EndpointInfo { method: "POST", path: "/gui/toggle", description: "GUI-Fenster umschalten" },
                EndpointInfo { method: "POST", path: "/gui/show", description: "GUI anzeigen" },
                EndpointInfo { method: "POST", path: "/gui/hide", description: "GUI verbergen" },
                EndpointInfo { method: "GET", path: "/gui/status", description: "GUI Status" },
            ],
        },
        EndpointCategory {
            name: "CDP (Chrome DevTools Protocol)",
            endpoints: vec![
                EndpointInfo { method: "GET", path: "/cdp", description: "CDP Remote-Debugging Verbindungsinfo" },
                EndpointInfo { method: "GET", path: "/cdp/targets", description: "Alle CDP Targets mit Tab-Mapping" },
                EndpointInfo { method: "GET", path: "/cdp/target/:tab_id", description: "CDP Target fuer bestimmten Tab" },
            ],
        },
        EndpointCategory {
            name: "Debug: CAPTCHA",
            endpoints: vec![
                EndpointInfo { method: "POST", path: "/debug/captcha/detect", description: "CAPTCHA auf Seite erkennen (tab_id)" },
                EndpointInfo { method: "POST", path: "/debug/captcha/solve", description: "CAPTCHA-Loesungsschritte (tab_id)" },
            ],
        },
        EndpointCategory {
            name: "Debug: Console",
            endpoints: vec![
                EndpointInfo { method: "GET", path: "/debug/console", description: "Console Logs abrufen (?tab_id)" },
                EndpointInfo { method: "POST", path: "/debug/console/start", description: "Console Capture starten" },
                EndpointInfo { method: "POST", path: "/debug/console/stop", description: "Console Capture stoppen" },
            ],
        },
        EndpointCategory {
            name: "Debug: Cookies",
            endpoints: vec![
                EndpointInfo { method: "GET", path: "/debug/cookies/:tab_id", description: "Alle Cookies auflisten" },
                EndpointInfo { method: "GET", path: "/debug/cookies/:tab_id/:name", description: "Einzelnes Cookie abrufen" },
                EndpointInfo { method: "POST", path: "/debug/cookies/:tab_id/set", description: "Cookie setzen" },
                EndpointInfo { method: "DELETE", path: "/debug/cookies/:tab_id/:name", description: "Cookie loeschen" },
                EndpointInfo { method: "DELETE", path: "/debug/cookies/:tab_id", description: "Alle Cookies loeschen" },
            ],
        },
        EndpointCategory {
            name: "Debug: CSS",
            endpoints: vec![
                EndpointInfo { method: "POST", path: "/debug/css/computed", description: "Berechnete CSS-Styles eines Elements" },
                EndpointInfo { method: "POST", path: "/debug/css/matched", description: "Matched CSS Rules" },
                EndpointInfo { method: "POST", path: "/debug/css/box-model", description: "Box Model Information" },
            ],
        },
        EndpointCategory {
            name: "Debug: Network",
            endpoints: vec![
                EndpointInfo { method: "POST", path: "/debug/network/start", description: "Network Capture starten" },
                EndpointInfo { method: "POST", path: "/debug/network/stop", description: "Network Capture stoppen" },
                EndpointInfo { method: "GET", path: "/debug/network/requests", description: "Erfasste Requests abrufen" },
            ],
        },
        EndpointCategory {
            name: "Debug: Performance",
            endpoints: vec![
                EndpointInfo { method: "GET", path: "/debug/performance/timing", description: "Navigation Timing Metriken" },
                EndpointInfo { method: "GET", path: "/debug/performance/resources", description: "Resource Timing" },
                EndpointInfo { method: "GET", path: "/debug/performance/vitals", description: "Core Web Vitals" },
                EndpointInfo { method: "GET", path: "/debug/performance/memory", description: "Memory-Nutzung" },
            ],
        },
        EndpointCategory {
            name: "Debug: Consent & Popups",
            endpoints: vec![
                EndpointInfo { method: "POST", path: "/debug/consent/accept", description: "Cookie-Consent automatisch akzeptieren" },
                EndpointInfo { method: "GET", path: "/debug/popups", description: "Erkannte Popups auflisten" },
            ],
        },
        EndpointCategory {
            name: "WebSocket",
            endpoints: vec![
                EndpointInfo { method: "GET", path: "/ws", description: "WebSocket fuer Echtzeit-Events" },
                EndpointInfo { method: "GET", path: "/ws/viewer", description: "WebSocket fuer Live-Viewer Stream" },
            ],
        },
    ];

    let total: usize = categories.iter().map(|c| c.endpoints.len()).sum();

    Json(ApiResponse::success(EndpointsData {
        total,
        categories,
        docs: DocsInfo {
            swagger_ui: "/swagger-ui/",
            openapi_json: "/api-doc/openapi.json",
        },
    }))
}
