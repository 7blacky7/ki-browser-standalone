//! Popup URL tracking endpoint.
//!
//! When CEF blocks a window.open() popup, the target URL is stored.
//! This endpoint lets agents retrieve those URLs and navigate to them.

use axum::{
    extract::State,
    response::IntoResponse,
    routing::get,
    Json, Router,
};
use serde::Serialize;

use crate::api::routes::ApiResponse;
use crate::api::server::AppState;

#[derive(Debug, Serialize)]
pub struct PopupEntry {
    pub source_tab_id: String,
    pub url: String,
    pub age_seconds: f64,
}

#[derive(Debug, Serialize)]
pub struct PopupsResponse {
    pub popups: Vec<PopupEntry>,
    pub count: usize,
}

/// GET /debug/popups — List all intercepted popup URLs (newest first).
async fn list_popups(State(_state): State<AppState>) -> impl IntoResponse {
    #[cfg(feature = "cef-browser")]
    {
        let store = crate::browser::cef_engine::POPUP_URL_STORE.lock();
        let now = std::time::Instant::now();
        let popups: Vec<PopupEntry> = store
            .iter()
            .rev()
            .map(|(tab_id, url, time)| PopupEntry {
                source_tab_id: tab_id.to_string(),
                url: url.clone(),
                age_seconds: now.duration_since(*time).as_secs_f64(),
            })
            .collect();
        let count = popups.len();
        Json(ApiResponse::success(PopupsResponse { popups, count }))
    }

    #[cfg(not(feature = "cef-browser"))]
    {
        Json(ApiResponse::success(PopupsResponse {
            popups: vec![],
            count: 0,
        }))
    }
}

pub fn popup_routes() -> Router<AppState> {
    Router::new().route("/debug/popups", get(list_popups))
}
