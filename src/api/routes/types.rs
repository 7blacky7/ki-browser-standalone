//! Shared request/response types for REST API route handlers.
//!
//! Contains the `ApiResponse` wrapper and `HealthResponse` used across all
//! route modules. Centralising these types avoids duplication and gives a
//! single place to extend the API envelope.

use serde::{Deserialize, Serialize};

use crate::api::server::TabState;

// ============================================================================
// Generic API Envelope
// ============================================================================

/// Standard JSON envelope returned by every REST API endpoint.
///
/// On success `success` is `true` and `data` contains the payload.
/// On failure `success` is `false` and `error` contains the human-readable
/// error message. The absent field is omitted from serialisation.
#[derive(Debug, Serialize)]
pub struct ApiResponse<T: Serialize> {
    pub success: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub data: Option<T>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

impl<T: Serialize> ApiResponse<T> {
    /// Build a successful response wrapping `data`.
    pub fn success(data: T) -> Self {
        Self {
            success: true,
            data: Some(data),
            error: None,
        }
    }

    /// Build an error response with the given human-readable `message`.
    pub fn error(message: impl Into<String>) -> ApiResponse<()> {
        ApiResponse {
            success: false,
            data: None,
            error: Some(message.into()),
        }
    }
}

// ============================================================================
// Health / Status Types
// ============================================================================

/// Response body for `GET /health` – indicates server liveness and build version.
#[derive(Debug, Serialize)]
pub struct HealthResponse {
    pub status: String,
    pub version: String,
    pub api_enabled: bool,
}

/// Response body for `GET /api/status` and `POST /api/toggle`.
#[derive(Debug, Serialize)]
pub struct ApiStatusResponse {
    pub enabled: bool,
    pub port: u16,
    pub connected_clients: usize,
}

/// Request body for `POST /api/toggle`.
#[derive(Debug, Deserialize)]
pub struct ApiToggleRequest {
    pub enabled: bool,
}

// ============================================================================
// Tab Types (shared with tabs module)
// ============================================================================

/// Snapshot of a single browser tab's runtime state exposed via the REST API.
#[derive(Debug, Serialize, Clone)]
pub struct TabInfo {
    pub id: String,
    pub url: String,
    pub title: String,
    pub is_loading: bool,
    pub is_active: bool,
    pub can_go_back: bool,
    pub can_go_forward: bool,
}

impl From<&TabState> for TabInfo {
    fn from(state: &TabState) -> Self {
        Self {
            id: state.id.clone(),
            url: state.url.clone(),
            title: state.title.clone(),
            is_loading: state.is_loading,
            is_active: false, // Set by caller based on active_tab_id
            can_go_back: state.can_go_back,
            can_go_forward: state.can_go_forward,
        }
    }
}
