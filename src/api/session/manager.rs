//! Thread-safe session manager for concurrent API access.
//!
//! Provides session lifecycle operations (create, get, list, delete),
//! per-session tab tracking, key-value storage, cookie management,
//! navigation history, and named state snapshots.

use std::collections::HashMap;
use std::sync::Arc;

use chrono::Utc;
use tokio::sync::RwLock;
use tracing::{debug, info, warn};
use uuid::Uuid;

use super::types::*;

/// Thread-safe session manager for concurrent API access.
///
/// Wraps all mutable state in `Arc<RwLock<>>` so it can be shared
/// across Axum handler tasks safely.
#[derive(Debug, Clone)]
pub struct SessionManager {
    /// Map of session ID to session data
    sessions: Arc<RwLock<HashMap<String, Session>>>,
}

impl SessionManager {
    /// Create a new empty session manager.
    pub fn new() -> Self {
        Self {
            sessions: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    /// Create a new session with an optional human-readable name.
    ///
    /// Returns the newly created session.
    pub async fn create_session(&self, name: Option<String>) -> Session {
        let now = Utc::now().to_rfc3339();
        let session = Session {
            id: Uuid::new_v4().to_string(),
            name,
            created_at: now.clone(),
            last_activity: now,
            tabs: Vec::new(),
            storage: HashMap::new(),
            cookies: Vec::new(),
            history: Vec::new(),
            snapshots: Vec::new(),
        };

        let id = session.id.clone();
        let result = session.clone();

        let mut sessions = self.sessions.write().await;
        sessions.insert(id.clone(), session);

        info!("Created session: {}", id);
        result
    }

    /// Retrieve a session by ID.
    pub async fn get_session(&self, id: &str) -> Option<Session> {
        let sessions = self.sessions.read().await;
        sessions.get(id).cloned()
    }

    /// List all active sessions.
    pub async fn list_sessions(&self) -> Vec<Session> {
        let sessions = self.sessions.read().await;
        sessions.values().cloned().collect()
    }

    /// Delete a session by ID. Returns `true` if the session existed.
    pub async fn delete_session(&self, id: &str) -> bool {
        let mut sessions = self.sessions.write().await;
        let removed = sessions.remove(id).is_some();
        if removed {
            info!("Deleted session: {}", id);
        } else {
            warn!("Attempted to delete non-existent session: {}", id);
        }
        removed
    }

    /// Add a tab to a session. Updates `last_activity`.
    pub async fn add_tab(&self, session_id: &str, tab_id: String) -> bool {
        let mut sessions = self.sessions.write().await;
        if let Some(session) = sessions.get_mut(session_id) {
            if !session.tabs.contains(&tab_id) {
                session.tabs.push(tab_id.clone());
                debug!("Added tab {} to session {}", tab_id, session_id);
            }
            session.last_activity = Utc::now().to_rfc3339();
            true
        } else {
            false
        }
    }

    /// Remove a tab from a session. Updates `last_activity`.
    pub async fn remove_tab(&self, session_id: &str, tab_id: &str) -> bool {
        let mut sessions = self.sessions.write().await;
        if let Some(session) = sessions.get_mut(session_id) {
            session.tabs.retain(|t| t != tab_id);
            session.last_activity = Utc::now().to_rfc3339();
            debug!("Removed tab {} from session {}", tab_id, session_id);
            true
        } else {
            false
        }
    }

    /// Store a key-value pair in the session's custom storage.
    pub async fn set_storage(
        &self,
        session_id: &str,
        key: String,
        value: serde_json::Value,
    ) -> bool {
        let mut sessions = self.sessions.write().await;
        if let Some(session) = sessions.get_mut(session_id) {
            session.storage.insert(key, value);
            session.last_activity = Utc::now().to_rfc3339();
            true
        } else {
            false
        }
    }

    /// Retrieve a value from the session's custom storage.
    pub async fn get_storage(
        &self,
        session_id: &str,
        key: &str,
    ) -> Option<serde_json::Value> {
        let sessions = self.sessions.read().await;
        sessions
            .get(session_id)
            .and_then(|s| s.storage.get(key).cloned())
    }

    /// Delete a key from the session's custom storage.
    pub async fn delete_storage(&self, session_id: &str, key: &str) -> bool {
        let mut sessions = self.sessions.write().await;
        if let Some(session) = sessions.get_mut(session_id) {
            let removed = session.storage.remove(key).is_some();
            if removed {
                session.last_activity = Utc::now().to_rfc3339();
            }
            removed
        } else {
            false
        }
    }

    /// Update the cookies stored in a session.
    pub async fn set_cookies(&self, session_id: &str, cookies: Vec<CookieInfo>) -> bool {
        let mut sessions = self.sessions.write().await;
        if let Some(session) = sessions.get_mut(session_id) {
            session.cookies = cookies;
            session.last_activity = Utc::now().to_rfc3339();
            true
        } else {
            false
        }
    }

    /// Append a navigation history entry to the session.
    pub async fn add_history(&self, session_id: &str, entry: HistoryEntry) {
        let mut sessions = self.sessions.write().await;
        if let Some(session) = sessions.get_mut(session_id) {
            session.history.push(entry);
            session.last_activity = Utc::now().to_rfc3339();
        } else {
            warn!(
                "Cannot add history: session {} does not exist",
                session_id
            );
        }
    }

    /// Create a named state snapshot for the session.
    ///
    /// The caller must supply `tab_states` with the current state of
    /// each tab (URL, cookies, storage). Returns the snapshot if the
    /// session exists.
    pub async fn create_snapshot(
        &self,
        session_id: &str,
        name: String,
        description: Option<String>,
        tab_states: Vec<TabSnapshot>,
    ) -> Option<SessionSnapshot> {
        let mut sessions = self.sessions.write().await;
        if let Some(session) = sessions.get_mut(session_id) {
            let snapshot = SessionSnapshot {
                name: name.clone(),
                description,
                created_at: Utc::now().to_rfc3339(),
                tab_states,
            };
            let result = snapshot.clone();
            session.snapshots.push(snapshot);
            session.last_activity = Utc::now().to_rfc3339();

            info!("Created snapshot '{}' for session {}", name, session_id);
            Some(result)
        } else {
            warn!(
                "Cannot create snapshot: session {} does not exist",
                session_id
            );
            None
        }
    }

    /// Retrieve a named snapshot from a session.
    pub async fn get_snapshot(
        &self,
        session_id: &str,
        snapshot_name: &str,
    ) -> Option<SessionSnapshot> {
        let sessions = self.sessions.read().await;
        sessions.get(session_id).and_then(|s| {
            s.snapshots
                .iter()
                .find(|snap| snap.name == snapshot_name)
                .cloned()
        })
    }

    /// List all snapshot names for a session.
    pub async fn list_snapshots(&self, session_id: &str) -> Vec<String> {
        let sessions = self.sessions.read().await;
        sessions
            .get(session_id)
            .map(|s| s.snapshots.iter().map(|snap| snap.name.clone()).collect())
            .unwrap_or_default()
    }

    /// Delete a named snapshot from a session.
    pub async fn delete_snapshot(&self, session_id: &str, snapshot_name: &str) -> bool {
        let mut sessions = self.sessions.write().await;
        if let Some(session) = sessions.get_mut(session_id) {
            let before = session.snapshots.len();
            session.snapshots.retain(|s| s.name != snapshot_name);
            let removed = session.snapshots.len() < before;
            if removed {
                session.last_activity = Utc::now().to_rfc3339();
                info!(
                    "Deleted snapshot '{}' from session {}",
                    snapshot_name, session_id
                );
            }
            removed
        } else {
            false
        }
    }

    /// Touch the session to update `last_activity` without changing data.
    pub async fn touch(&self, session_id: &str) -> bool {
        let mut sessions = self.sessions.write().await;
        if let Some(session) = sessions.get_mut(session_id) {
            session.last_activity = Utc::now().to_rfc3339();
            true
        } else {
            false
        }
    }

    /// Return the number of active sessions.
    pub async fn session_count(&self) -> usize {
        let sessions = self.sessions.read().await;
        sessions.len()
    }
}

impl Default for SessionManager {
    fn default() -> Self {
        Self::new()
    }
}
