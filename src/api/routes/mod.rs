//! REST API routes and handlers for browser control operations.
//!
//! Organises all HTTP endpoints by resource type into sub-modules and
//! re-exports every public type and handler so that external callers
//! (server.rs, openapi.rs, etc.) can continue to use `crate::api::routes::*`
//! without any path changes.

use axum::{
    routing::{get, post},
    Router,
};

use crate::api::server::AppState;

// Sub-modules by resource concern
pub mod dom;
pub mod input;
pub mod misc;
pub mod navigation;
pub mod screenshot;
pub mod tabs;
pub mod types;

// Re-export all public types so external code needs no path changes
pub use types::{
    ApiResponse,
    ApiStatusResponse,
    ApiToggleRequest,
    BoundingBox,
    ClickRequest,
    CloseTabRequest,
    ElementInfo,
    EvaluateRequest,
    EvaluateResponse,
    FindElementQuery,
    HealthResponse,
    NavigateRequest,
    NewTabRequest,
    NewTabResponse,
    ScreenshotQuery,
    ScreenshotResponse,
    ScrollRequest,
    TabInfo,
    TabsResponse,
    TypeRequest,
};

// Re-export all handler functions so that `crate::api::routes::health_check`
// etc. continue to resolve (required by utoipa __path_* items if present).
pub use dom::find_element;
pub use input::{click, scroll, type_text};
pub use misc::{api_status, health_check, toggle_api};
pub use navigation::{evaluate, navigate};
pub use screenshot::screenshot;
pub use tabs::{close_tab, create_tab, list_tabs};

/// Build the axum Router with all API routes attached and app state injected.
///
/// Registers every REST endpoint under the appropriate HTTP method and path.
/// The WebSocket endpoint is wired separately by the server layer.
pub fn create_router(state: AppState) -> Router {
    Router::new()
        // Health check
        .route("/health", get(health_check))
        // Tab management
        .route("/tabs", get(list_tabs))
        .route("/tabs/new", post(create_tab))
        .route("/tabs/close", post(close_tab))
        // Navigation and JavaScript evaluation
        .route("/navigate", post(navigate))
        .route("/evaluate", post(evaluate))
        // Input interactions
        .route("/click", post(click))
        .route("/type", post(type_text))
        .route("/scroll", post(scroll))
        // Screenshot capture
        .route("/screenshot", get(screenshot))
        // DOM inspection
        .route("/dom/element", get(find_element))
        // API management
        .route("/api/toggle", post(toggle_api))
        .route("/api/status", get(api_status))
        .with_state(state)
}
