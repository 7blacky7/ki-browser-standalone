//! REST endpoints for GUI window visibility control.
//!
//! Provides POST /gui/toggle, POST /gui/show, POST /gui/hide, and
//! GET /gui/status so that external callers (scripts, KI agents) can
//! show or hide the browser window at runtime without restarting the
//! process. The window stays in RAM -- only its visibility changes.

use axum::{
    extract::State,
    http::StatusCode,
    response::IntoResponse,
    routing::{get, post},
    Json, Router,
};
use serde::Serialize;

use crate::api::routes::ApiResponse;
use crate::api::server::AppState;

/// Response payload for all GUI visibility endpoints.
#[derive(Debug, Serialize)]
pub struct GuiStatusResponse {
    /// Current visibility: "visible", "hidden", or "disabled".
    pub visibility: String,
}

// ============================================================================
// Handlers
// ============================================================================

/// POST /gui/toggle - Toggle GUI window between visible and hidden.
///
/// If the GUI was started, toggles between visible/hidden.
/// If running in headless mode, returns 409 Conflict.
#[cfg(feature = "gui")]
async fn gui_toggle(State(state): State<AppState>) -> impl IntoResponse {
    match &state.gui_handle {
        Some(handle) => {
            let new_vis = handle.toggle();
            info!("GUI toggled to {:?}", new_vis);
            Json(ApiResponse::success(GuiStatusResponse {
                visibility: format!("{:?}", new_vis).to_lowercase(),
            })).into_response()
        }
        None => (
            StatusCode::CONFLICT,
            Json(ApiResponse::<GuiStatusResponse>::error(
                "GUI is not available (headless mode)",
            )),
        ).into_response(),
    }
}

/// POST /gui/show - Show the GUI window (restore from hidden/minimized).
#[cfg(feature = "gui")]
async fn gui_show(State(state): State<AppState>) -> impl IntoResponse {
    match &state.gui_handle {
        Some(handle) => {
            handle.show();
            info!("GUI shown via API");
            Json(ApiResponse::success(GuiStatusResponse {
                visibility: "visible".to_string(),
            })).into_response()
        }
        None => (
            StatusCode::CONFLICT,
            Json(ApiResponse::<GuiStatusResponse>::error(
                "GUI is not available (headless mode)",
            )),
        ).into_response(),
    }
}

/// POST /gui/hide - Hide the GUI window (minimize / withdraw).
#[cfg(feature = "gui")]
async fn gui_hide(State(state): State<AppState>) -> impl IntoResponse {
    match &state.gui_handle {
        Some(handle) => {
            handle.hide();
            info!("GUI hidden via API");
            Json(ApiResponse::success(GuiStatusResponse {
                visibility: "hidden".to_string(),
            })).into_response()
        }
        None => (
            StatusCode::CONFLICT,
            Json(ApiResponse::<GuiStatusResponse>::error(
                "GUI is not available (headless mode)",
            )),
        ).into_response(),
    }
}

/// GET /gui/status - Returns the current GUI visibility state.
#[cfg(feature = "gui")]
async fn gui_status(State(state): State<AppState>) -> impl IntoResponse {
    let visibility = match &state.gui_handle {
        Some(handle) => format!("{:?}", handle.visibility()).to_lowercase(),
        None => "disabled".to_string(),
    };

    Json(ApiResponse::success(GuiStatusResponse { visibility }))
}

/// Fallback handlers when the `gui` feature is not compiled in.
/// Always returns "disabled" status or 409 Conflict for mutations.
#[cfg(not(feature = "gui"))]
async fn gui_toggle(State(_state): State<AppState>) -> impl IntoResponse {
    (
        StatusCode::CONFLICT,
        Json(ApiResponse::<GuiStatusResponse>::error(
            "GUI feature not compiled in",
        )),
    ).into_response()
}

#[cfg(not(feature = "gui"))]
async fn gui_show(State(_state): State<AppState>) -> impl IntoResponse {
    (
        StatusCode::CONFLICT,
        Json(ApiResponse::<GuiStatusResponse>::error(
            "GUI feature not compiled in",
        )),
    ).into_response()
}

#[cfg(not(feature = "gui"))]
async fn gui_hide(State(_state): State<AppState>) -> impl IntoResponse {
    (
        StatusCode::CONFLICT,
        Json(ApiResponse::<GuiStatusResponse>::error(
            "GUI feature not compiled in",
        )),
    ).into_response()
}

#[cfg(not(feature = "gui"))]
async fn gui_status(State(_state): State<AppState>) -> impl IntoResponse {
    Json(ApiResponse::success(GuiStatusResponse {
        visibility: "disabled".to_string(),
    }))
}

// ============================================================================
// Router
// ============================================================================

/// Creates the router fragment for GUI visibility control endpoints.
pub fn gui_routes() -> Router<AppState> {
    Router::new()
        .route("/gui/toggle", post(gui_toggle))
        .route("/gui/show", post(gui_show))
        .route("/gui/hide", post(gui_hide))
        .route("/gui/status", get(gui_status))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_gui_status_response_serialization() {
        let resp = GuiStatusResponse {
            visibility: "visible".to_string(),
        };
        let json = serde_json::to_string(&resp).expect("serialize");
        assert!(json.contains("\"visibility\":\"visible\""));
    }

    #[test]
    fn test_gui_status_response_hidden() {
        let resp = GuiStatusResponse {
            visibility: "hidden".to_string(),
        };
        let json = serde_json::to_string(&resp).expect("serialize");
        assert!(json.contains("\"visibility\":\"hidden\""));
    }

    #[test]
    fn test_gui_status_response_disabled() {
        let resp = GuiStatusResponse {
            visibility: "disabled".to_string(),
        };
        let json = serde_json::to_string(&resp).expect("serialize");
        assert!(json.contains("\"visibility\":\"disabled\""));
    }
}
