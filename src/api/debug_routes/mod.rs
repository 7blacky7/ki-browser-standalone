//! Debug route handlers for website inspection and debugging.
//!
//! Provides REST endpoints for:
//! - **Console**: Captured browser console messages (log, warn, error)
//! - **Cookies**: CRUD operations for browser cookies
//! - **CSS Inspector**: Computed styles, matched rules, box model
//! - **Network**: Request/response capture via JS instrumentation
//! - **Performance**: Navigation timing, resource timing, Web Vitals, memory

pub mod types;
pub mod captcha;
pub mod console;
pub mod consent;
pub mod cookies;
pub mod css_inspector;
pub mod network;
pub mod performance;
pub mod popups;

use axum::Router;
use crate::api::server::AppState;

pub use console::{ConsoleLogBuffer, ConsoleLogEntry, create_log_entry};

/// Combined router for all debug endpoints under /debug/*.
pub fn debug_routes() -> Router<AppState> {
    Router::new()
        .merge(captcha::captcha_routes())
        .merge(console::console_routes())
        .merge(consent::consent_routes())
        .merge(cookies::cookie_routes())
        .merge(css_inspector::css_routes())
        .merge(network::network_routes())
        .merge(performance::performance_routes())
        .merge(popups::popup_routes())
}
