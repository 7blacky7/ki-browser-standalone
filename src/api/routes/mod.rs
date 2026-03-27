//! REST API routes and handlers
//!
//! Defines all HTTP endpoints for browser control operations.
//! Handlers are organized into submodules by concern:
//! - `types`: Request/response DTOs
//! - `tabs`: Tab lifecycle (list, create, close)
//! - `navigation`: Page interaction (navigate, click, type, scroll, evaluate, screenshot)
//! - `dom`: DOM queries (find element, annotate, snapshot, frames)
//! - `misc`: Health check, API toggle/status, CDP info

pub mod types;
pub mod tabs;
pub mod navigation;
pub mod dom;
pub mod misc;

// Re-export all types for backward compatibility
pub use types::*;

// Re-export all handler functions for use in create_router and external references
pub use tabs::{list_tabs, create_tab, close_tab};
pub use navigation::{navigate, click, drag, type_text, evaluate, screenshot, scroll};
pub use dom::{find_element, annotate_elements, dom_snapshot, get_frames};
pub use misc::{health_check, toggle_api, api_status, cdp_targets, cdp_target_by_tab};
pub(crate) use misc::cdp_info;

use axum::{
    routing::{get, post},
    Router,
};

use crate::api::server::AppState;
use crate::api::websocket;
use crate::api::agent_routes::agent_routes;
use crate::api::gui_routes::gui_routes;
use crate::api::vision_routes::vision_routes;
use crate::api::extraction_routes::extraction_routes;
use crate::api::batch_routes::batch_session_routes;
use crate::api::debug_routes::debug_routes;

/// Create the API router with all routes configured
pub fn create_router(state: AppState) -> Router {
    Router::new()
        // Health check
        .route("/health", get(health_check))

        // Tab management
        .route("/tabs", get(list_tabs))
        .route("/tabs/new", post(create_tab))
        .route("/tabs/close", post(close_tab))

        // Navigation and interaction
        .route("/navigate", post(navigate))
        .route("/click", post(click))
        .route("/drag", post(drag))
        .route("/type", post(type_text))
        .route("/evaluate", post(evaluate))
        .route("/screenshot", get(screenshot))
        .route("/scroll", post(scroll))
        .route("/frames", get(get_frames))

        // DOM operations
        .route("/dom/element", get(find_element))
        .route("/dom/annotate", post(annotate_elements))
        .route("/dom/snapshot", get(dom_snapshot))

        // CDP remote debugging info
        .route("/cdp", get(cdp_info))

        // CDP tab mapping
        .route("/cdp/targets", get(cdp_targets))
        .route("/cdp/target/:tab_id", get(cdp_target_by_tab))

        // API management
        .route("/api/toggle", post(toggle_api))
        .route("/api/status", get(api_status))

        // DOM extraction routes (structured data, content, forms)
        .merge(extraction_routes())

        // Batch operations and session management routes
        .merge(batch_session_routes())

        // Multi-agent session management and tab ownership
        .merge(agent_routes())

        // Vision overlay for KI agent annotated screenshots
        .merge(vision_routes())

        // GUI window visibility control (toggle, show, hide, status)
        .merge(gui_routes())

        // OCR endpoints
        .merge(crate::api::ocr_routes::ocr_routes())

        // Debug/DevTools endpoints (console, cookies, CSS, network, performance)
        .merge(debug_routes())

        // WebSocket endpoints
        .route("/ws", get(websocket::ws_handler))
        .route("/ws/viewer", get(crate::api::viewer_stream::viewer_ws_handler))

        .with_state(state)
}
