//! REST API route registry for the ki-browser HTTP server.
//!
//! This module declares all route sub-modules, re-exports public types used
//! by callers outside the `routes` module, and wires every handler into the
//! axum `Router` via `create_router`.
//!
//! Sub-module responsibilities:
//! - `types`      – shared request/response envelope and shared DTOs
//! - `misc`       – health check, API status, API toggle
//! - `navigation` – navigate, go_back, go_forward, reload
//! - `tabs`       – list, create, close tabs
//! - `dom`        – find_element, evaluate JavaScript
//! - `input`      – click, type_text, scroll
//! - `screenshot` – capture screenshot

pub mod dom;
pub mod input;
pub mod misc;
pub mod navigation;
pub mod screenshot;
pub mod tabs;
pub mod types;

// Re-export the shared API envelope so callers can use `routes::ApiResponse`.
pub use types::ApiResponse;

use axum::{
    routing::{get, post},
    Router,
};

use crate::api::server::AppState;

/// Build the axum `Router` with all browser-control API routes attached.
///
/// The returned router must be layered with CORS and tracing middleware by the
/// caller (`ApiServer::build_router`) before being bound to a TCP listener.
pub fn create_router(state: AppState) -> Router {
    Router::new()
        // Health / observability
        .route("/health", get(misc::health_check))
        // API management
        .route("/api/toggle", post(misc::toggle_api))
        .route("/api/status", get(misc::api_status))
        // Tab management
        .route("/tabs", get(tabs::list_tabs))
        .route("/tabs/new", post(tabs::create_tab))
        .route("/tabs/close", post(tabs::close_tab))
        // Navigation
        .route("/navigate", post(navigation::navigate))
        .route("/go_back", post(navigation::go_back))
        .route("/go_forward", post(navigation::go_forward))
        .route("/reload", post(navigation::reload))
        // Input injection
        .route("/click", post(input::click))
        .route("/type", post(input::type_text))
        .route("/scroll", post(input::scroll))
        // Screenshot capture
        .route("/screenshot", get(screenshot::screenshot))
        // DOM interaction
        .route("/dom/element", get(dom::find_element))
        .route("/evaluate", post(dom::evaluate))
        // WebSocket endpoint is registered separately by ApiServer
        .with_state(state)
}
