//! Agent registry for multi-agent session management.
//!
//! Provides a thread-safe registry for AI agents that connect to the browser.
//! Each agent receives a unique UUID and can claim ownership of browser tabs
//! to prevent concurrent access conflicts in multi-agent scenarios.

use std::collections::HashMap;
use std::sync::Arc;

use chrono::{DateTime, Utc};
use parking_lot::RwLock;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::error::{BrowserError, BrowserResult};

/// Information about a registered AI agent.
///
/// Tracks agent identity, registration time, and which tabs the agent
/// currently owns for exclusive access.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentInfo {
    /// Unique agent identifier (UUID v4).
    pub id: Uuid,

    /// Human-readable agent name (e.g. "scraper-1", "form-filler").
    pub name: String,

    /// Timestamp when the agent registered with the browser.
    pub registered_at: DateTime<Utc>,

    /// Tab IDs currently owned by this agent for exclusive access.
    pub owned_tabs: Vec<Uuid>,
}

/// Thread-safe registry for managing connected AI agents.
///
/// Agents must register before claiming tab ownership. The registry
/// uses `parking_lot::RwLock` for efficient concurrent read access
/// with exclusive write access.
#[derive(Debug, Clone)]
pub struct AgentRegistry {
    agents: Arc<RwLock<HashMap<Uuid, AgentInfo>>>,
}

impl Default for AgentRegistry {
    fn default() -> Self {
        Self::new()
    }
}

impl AgentRegistry {
    /// Create a new empty agent registry.
    pub fn new() -> Self {
        Self {
            agents: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    /// Register a new agent and return its assigned UUID.
    ///
    /// # Arguments
    /// * `name` - Human-readable name for the agent
    ///
    /// # Returns
    /// The newly created `AgentInfo` with a fresh UUID.
    pub fn register(&self, name: String) -> AgentInfo {
        let agent = AgentInfo {
            id: Uuid::new_v4(),
            name,
            registered_at: Utc::now(),
            owned_tabs: Vec::new(),
        };

        let result = agent.clone();
        self.agents.write().insert(agent.id, agent);
        result
    }

    /// Unregister an agent and release all its owned tabs.
    ///
    /// # Arguments
    /// * `agent_id` - UUID of the agent to unregister
    ///
    /// # Returns
    /// The removed `AgentInfo`, or an error if the agent was not found.
    pub fn unregister(&self, agent_id: Uuid) -> BrowserResult<AgentInfo> {
        self.agents
            .write()
            .remove(&agent_id)
            .ok_or_else(|| BrowserError::SessionError(format!("Agent not found: {}", agent_id)))
    }

    /// Look up an agent by its UUID.
    ///
    /// # Returns
    /// A clone of the `AgentInfo`, or `None` if not registered.
    pub fn get_agent(&self, agent_id: Uuid) -> Option<AgentInfo> {
        self.agents.read().get(&agent_id).cloned()
    }

    /// List all currently registered agents.
    pub fn list_agents(&self) -> Vec<AgentInfo> {
        self.agents.read().values().cloned().collect()
    }

    /// Record that an agent now owns a specific tab.
    ///
    /// Called internally when a tab is successfully claimed.
    pub fn add_owned_tab(&self, agent_id: Uuid, tab_id: Uuid) -> BrowserResult<()> {
        let mut agents = self.agents.write();
        let agent = agents.get_mut(&agent_id).ok_or_else(|| {
            BrowserError::SessionError(format!("Agent not found: {}", agent_id))
        })?;

        if !agent.owned_tabs.contains(&tab_id) {
            agent.owned_tabs.push(tab_id);
        }
        Ok(())
    }

    /// Remove a tab from an agent's ownership list.
    ///
    /// Called internally when a tab is released.
    pub fn remove_owned_tab(&self, agent_id: Uuid, tab_id: Uuid) -> BrowserResult<()> {
        let mut agents = self.agents.write();
        let agent = agents.get_mut(&agent_id).ok_or_else(|| {
            BrowserError::SessionError(format!("Agent not found: {}", agent_id))
        })?;

        agent.owned_tabs.retain(|id| *id != tab_id);
        Ok(())
    }

    /// Check whether an agent is registered.
    pub fn is_registered(&self, agent_id: Uuid) -> bool {
        self.agents.read().contains_key(&agent_id)
    }

    /// Return the number of registered agents.
    pub fn agent_count(&self) -> usize {
        self.agents.read().len()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_register_agent_returns_info() {
        let registry = AgentRegistry::new();
        let agent = registry.register("test-agent".to_string());

        assert_eq!(agent.name, "test-agent");
        assert!(agent.owned_tabs.is_empty());
        assert_eq!(registry.agent_count(), 1);
    }

    #[test]
    fn test_register_multiple_agents_unique_ids() {
        let registry = AgentRegistry::new();
        let a1 = registry.register("agent-1".to_string());
        let a2 = registry.register("agent-2".to_string());

        assert_ne!(a1.id, a2.id);
        assert_eq!(registry.agent_count(), 2);
    }

    #[test]
    fn test_unregister_agent_succeeds() {
        let registry = AgentRegistry::new();
        let agent = registry.register("temp".to_string());
        let id = agent.id;

        let removed = registry.unregister(id).unwrap();
        assert_eq!(removed.name, "temp");
        assert_eq!(registry.agent_count(), 0);
    }

    #[test]
    fn test_unregister_unknown_agent_returns_error() {
        let registry = AgentRegistry::new();
        let result = registry.unregister(Uuid::new_v4());
        assert!(result.is_err());
    }

    #[test]
    fn test_get_agent_found() {
        let registry = AgentRegistry::new();
        let agent = registry.register("lookup".to_string());

        let found = registry.get_agent(agent.id);
        assert!(found.is_some());
        assert_eq!(found.unwrap().name, "lookup");
    }

    #[test]
    fn test_get_agent_not_found() {
        let registry = AgentRegistry::new();
        assert!(registry.get_agent(Uuid::new_v4()).is_none());
    }

    #[test]
    fn test_list_agents_empty() {
        let registry = AgentRegistry::new();
        assert!(registry.list_agents().is_empty());
    }

    #[test]
    fn test_list_agents_returns_all() {
        let registry = AgentRegistry::new();
        registry.register("a".to_string());
        registry.register("b".to_string());
        registry.register("c".to_string());

        assert_eq!(registry.list_agents().len(), 3);
    }

    #[test]
    fn test_add_owned_tab_succeeds() {
        let registry = AgentRegistry::new();
        let agent = registry.register("owner".to_string());
        let tab_id = Uuid::new_v4();

        registry.add_owned_tab(agent.id, tab_id).unwrap();

        let updated = registry.get_agent(agent.id).unwrap();
        assert_eq!(updated.owned_tabs.len(), 1);
        assert_eq!(updated.owned_tabs[0], tab_id);
    }

    #[test]
    fn test_add_owned_tab_idempotent() {
        let registry = AgentRegistry::new();
        let agent = registry.register("owner".to_string());
        let tab_id = Uuid::new_v4();

        registry.add_owned_tab(agent.id, tab_id).unwrap();
        registry.add_owned_tab(agent.id, tab_id).unwrap();

        let updated = registry.get_agent(agent.id).unwrap();
        assert_eq!(updated.owned_tabs.len(), 1);
    }

    #[test]
    fn test_add_owned_tab_unknown_agent_returns_error() {
        let registry = AgentRegistry::new();
        let result = registry.add_owned_tab(Uuid::new_v4(), Uuid::new_v4());
        assert!(result.is_err());
    }

    #[test]
    fn test_remove_owned_tab_succeeds() {
        let registry = AgentRegistry::new();
        let agent = registry.register("owner".to_string());
        let tab_id = Uuid::new_v4();

        registry.add_owned_tab(agent.id, tab_id).unwrap();
        registry.remove_owned_tab(agent.id, tab_id).unwrap();

        let updated = registry.get_agent(agent.id).unwrap();
        assert!(updated.owned_tabs.is_empty());
    }

    #[test]
    fn test_is_registered_true() {
        let registry = AgentRegistry::new();
        let agent = registry.register("check".to_string());
        assert!(registry.is_registered(agent.id));
    }

    #[test]
    fn test_is_registered_false() {
        let registry = AgentRegistry::new();
        assert!(!registry.is_registered(Uuid::new_v4()));
    }

    #[test]
    fn test_default_creates_empty_registry() {
        let registry = AgentRegistry::default();
        assert_eq!(registry.agent_count(), 0);
    }
}
