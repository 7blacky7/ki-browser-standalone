//! API routes for multi-agent session management and tab ownership.
//!
//! Provides endpoints for agent registration/unregistration, listing agents,
//! and claiming/releasing exclusive tab ownership in multi-agent scenarios.

use axum::{
    extract::{Path, State},
    http::StatusCode,
    response::IntoResponse,
    routing::{get, post},
    Json, Router,
};
use serde::{Deserialize, Serialize};
use tracing::info;
use uuid::Uuid;

use crate::api::routes::ApiResponse;
use crate::api::server::AppState;
use crate::stealth::StealthConfig;

// ============================================================================
// Request/Response Types
// ============================================================================

/// Request body for POST /session/register
#[derive(Debug, Deserialize)]
pub struct RegisterAgentRequest {
    /// Human-readable name for the agent (e.g. "scraper-1")
    pub name: String,
}

/// Response for agent registration containing the assigned UUID
#[derive(Debug, Serialize)]
pub struct RegisterAgentResponse {
    pub agent_id: String,
    pub name: String,
    pub registered_at: String,
}

/// Request body for POST /session/unregister
#[derive(Debug, Deserialize)]
pub struct UnregisterAgentRequest {
    /// UUID of the agent to unregister
    pub agent_id: String,
}

/// Agent info returned in list responses
#[derive(Debug, Serialize)]
pub struct AgentInfoResponse {
    pub agent_id: String,
    pub name: String,
    pub registered_at: String,
    pub owned_tabs: Vec<String>,
}

/// Request body for POST /tabs/{tab_id}/claim and /tabs/{tab_id}/release
#[derive(Debug, Deserialize)]
pub struct TabOwnershipRequest {
    /// UUID of the agent claiming/releasing the tab
    pub agent_id: String,
}

/// Extended tab info including agent ownership
#[derive(Debug, Serialize)]
pub struct TabOwnershipResponse {
    pub tab_id: String,
    pub owner_agent_id: Option<String>,
}

/// Request to create a tab with optional stealth profile
#[derive(Debug, Deserialize)]
pub struct NewTabWithStealthRequest {
    #[serde(default)]
    pub url: Option<String>,
    #[serde(default)]
    pub active: Option<bool>,
    /// Optional stealth profile: "random", "consistent:<seed>", or omit for none
    #[serde(default)]
    pub stealth_profile: Option<String>,
}

// ============================================================================
// Route Handlers
// ============================================================================

/// POST /session/register - Register a new AI agent
async fn register_agent(
    State(state): State<AppState>,
    Json(request): Json<RegisterAgentRequest>,
) -> impl IntoResponse {
    let agent = state.agent_registry.register(request.name);

    info!("Registered agent: {} ({})", agent.name, agent.id);

    (
        StatusCode::CREATED,
        Json(ApiResponse::success(RegisterAgentResponse {
            agent_id: agent.id.to_string(),
            name: agent.name,
            registered_at: agent.registered_at.to_rfc3339(),
        })),
    )
}

/// POST /session/unregister - Unregister an AI agent
async fn unregister_agent(
    State(state): State<AppState>,
    Json(request): Json<UnregisterAgentRequest>,
) -> impl IntoResponse {
    let agent_id = match Uuid::parse_str(&request.agent_id) {
        Ok(id) => id,
        Err(_) => {
            return (
                StatusCode::BAD_REQUEST,
                Json(ApiResponse::<()>::error("Invalid agent_id format")),
            )
                .into_response();
        }
    };

    match state.agent_registry.unregister(agent_id) {
        Ok(agent) => {
            info!("Unregistered agent: {} ({})", agent.name, agent.id);
            Json(ApiResponse::success(())).into_response()
        }
        Err(e) => (
            StatusCode::NOT_FOUND,
            Json(ApiResponse::<()>::error(e.to_string())),
        )
            .into_response(),
    }
}

/// GET /session/agents - List all registered agents
async fn list_agents(State(state): State<AppState>) -> impl IntoResponse {
    let agents: Vec<AgentInfoResponse> = state
        .agent_registry
        .list_agents()
        .into_iter()
        .map(|a| AgentInfoResponse {
            agent_id: a.id.to_string(),
            name: a.name,
            registered_at: a.registered_at.to_rfc3339(),
            owned_tabs: a.owned_tabs.iter().map(|id| id.to_string()).collect(),
        })
        .collect();

    Json(ApiResponse::success(agents))
}

/// POST /tabs/{tab_id}/claim - Claim exclusive ownership of a tab
async fn claim_tab(
    State(state): State<AppState>,
    Path(tab_id_str): Path<String>,
    Json(request): Json<TabOwnershipRequest>,
) -> impl IntoResponse {
    let tab_id = match Uuid::parse_str(&tab_id_str) {
        Ok(id) => id,
        Err(_) => {
            return (
                StatusCode::BAD_REQUEST,
                Json(ApiResponse::<()>::error("Invalid tab_id format")),
            )
                .into_response();
        }
    };

    let agent_id = match Uuid::parse_str(&request.agent_id) {
        Ok(id) => id,
        Err(_) => {
            return (
                StatusCode::BAD_REQUEST,
                Json(ApiResponse::<()>::error("Invalid agent_id format")),
            )
                .into_response();
        }
    };

    if !state.agent_registry.is_registered(agent_id) {
        return (
            StatusCode::FORBIDDEN,
            Json(ApiResponse::<()>::error("Agent is not registered")),
        )
            .into_response();
    }

    // Claim in the browser tab manager (uses parking_lot, so no await needed)
    let _tab_manager = crate::browser::tab::TabManager::new();
    // We need to use the browser_state to find the tab — tabs in AppState
    // are tracked by string ID, but our TabManager uses Uuid. For the claim
    // operation we track ownership directly in the agent registry.

    // Record the ownership in the agent registry
    match state.agent_registry.add_owned_tab(agent_id, tab_id) {
        Ok(()) => {
            info!("Agent {} claimed tab {}", agent_id, tab_id);
            Json(ApiResponse::success(TabOwnershipResponse {
                tab_id: tab_id.to_string(),
                owner_agent_id: Some(agent_id.to_string()),
            }))
            .into_response()
        }
        Err(e) => (
            StatusCode::BAD_REQUEST,
            Json(ApiResponse::<()>::error(e.to_string())),
        )
            .into_response(),
    }
}

/// POST /tabs/{tab_id}/release - Release ownership of a tab
async fn release_tab(
    State(state): State<AppState>,
    Path(tab_id_str): Path<String>,
    Json(request): Json<TabOwnershipRequest>,
) -> impl IntoResponse {
    let tab_id = match Uuid::parse_str(&tab_id_str) {
        Ok(id) => id,
        Err(_) => {
            return (
                StatusCode::BAD_REQUEST,
                Json(ApiResponse::<()>::error("Invalid tab_id format")),
            )
                .into_response();
        }
    };

    let agent_id = match Uuid::parse_str(&request.agent_id) {
        Ok(id) => id,
        Err(_) => {
            return (
                StatusCode::BAD_REQUEST,
                Json(ApiResponse::<()>::error("Invalid agent_id format")),
            )
                .into_response();
        }
    };

    match state.agent_registry.remove_owned_tab(agent_id, tab_id) {
        Ok(()) => {
            info!("Agent {} released tab {}", agent_id, tab_id);
            Json(ApiResponse::success(TabOwnershipResponse {
                tab_id: tab_id.to_string(),
                owner_agent_id: None,
            }))
            .into_response()
        }
        Err(e) => (
            StatusCode::BAD_REQUEST,
            Json(ApiResponse::<()>::error(e.to_string())),
        )
            .into_response(),
    }
}

// ============================================================================
// Router
// ============================================================================

/// Create router for agent session management endpoints.
///
/// Mounts:
/// - POST /session/register
/// - POST /session/unregister
/// - GET  /session/agents
/// - POST /tabs/:tab_id/claim
/// - POST /tabs/:tab_id/release
pub fn agent_routes() -> Router<AppState> {
    Router::new()
        .route("/session/register", post(register_agent))
        .route("/session/unregister", post(unregister_agent))
        .route("/session/agents", get(list_agents))
        .route("/tabs/:tab_id/claim", post(claim_tab))
        .route("/tabs/:tab_id/release", post(release_tab))
}

// ============================================================================
// Stealth Helper
// ============================================================================

/// Parse a stealth profile string into a StealthConfig.
///
/// Supports:
/// - `"random"` -> fully randomized fingerprint
/// - `"consistent:<seed>"` -> deterministic fingerprint from seed
/// - anything else -> returns None
pub fn parse_stealth_profile(profile: &str) -> Option<StealthConfig> {
    if profile == "random" {
        Some(StealthConfig::random())
    } else { profile.strip_prefix("consistent:").map(StealthConfig::consistent) }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_stealth_profile_random() {
        let config = parse_stealth_profile("random");
        assert!(config.is_some());
    }

    #[test]
    fn test_parse_stealth_profile_consistent() {
        let config = parse_stealth_profile("consistent:my-seed");
        assert!(config.is_some());
    }

    #[test]
    fn test_parse_stealth_profile_consistent_deterministic() {
        let c1 = parse_stealth_profile("consistent:seed-123").unwrap();
        let c2 = parse_stealth_profile("consistent:seed-123").unwrap();
        assert_eq!(c1.fingerprint.user_agent, c2.fingerprint.user_agent);
    }

    #[test]
    fn test_parse_stealth_profile_unknown_returns_none() {
        assert!(parse_stealth_profile("unknown").is_none());
        assert!(parse_stealth_profile("").is_none());
    }
}
