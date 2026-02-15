//! Session management for browser state persistence
//!
//! Provides session lifecycle management for AI agents, including
//! per-session tab tracking, key-value storage, cookie management,
//! navigation history, and named state snapshots for checkpoint/restore
//! workflows.

use std::collections::HashMap;
use std::sync::Arc;

use chrono::Utc;
use serde::{Deserialize, Serialize};
use tokio::sync::RwLock;
use tracing::{debug, info, warn};
use uuid::Uuid;

// ============================================================================
// Session Data Types
// ============================================================================

/// A browser session with persistent state.
///
/// Sessions group tabs, cookies, storage, and history into a logical
/// unit that can be saved, restored, and shared between API calls.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Session {
    /// Unique session identifier (UUID v4)
    pub id: String,

    /// Optional human-readable session name
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,

    /// ISO 8601 timestamp of session creation
    pub created_at: String,

    /// ISO 8601 timestamp of last activity
    pub last_activity: String,

    /// Tab IDs belonging to this session
    pub tabs: Vec<String>,

    /// Custom key-value storage for agent use
    pub storage: HashMap<String, serde_json::Value>,

    /// Cookies captured in this session
    pub cookies: Vec<CookieInfo>,

    /// Navigation history across all session tabs
    pub history: Vec<HistoryEntry>,

    /// Named state snapshots for checkpoint/restore
    pub snapshots: Vec<SessionSnapshot>,
}

/// Information about a browser cookie.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CookieInfo {
    /// Cookie name
    pub name: String,

    /// Cookie value
    pub value: String,

    /// Domain the cookie belongs to
    pub domain: String,

    /// Cookie path
    pub path: String,

    /// Expiration timestamp (ISO 8601), if any
    #[serde(skip_serializing_if = "Option::is_none")]
    pub expires: Option<String>,

    /// Whether the cookie is HTTP-only
    pub http_only: bool,

    /// Whether the cookie requires HTTPS
    pub secure: bool,

    /// SameSite attribute value
    #[serde(skip_serializing_if = "Option::is_none")]
    pub same_site: Option<String>,
}

/// A single entry in the navigation history.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HistoryEntry {
    /// URL that was visited
    pub url: String,

    /// Page title at the time of the visit
    #[serde(skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,

    /// ISO 8601 timestamp of the visit
    pub timestamp: String,

    /// Tab that performed the navigation
    pub tab_id: String,
}

/// A named snapshot of session state at a point in time.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SessionSnapshot {
    /// Snapshot name (unique within the session)
    pub name: String,

    /// Optional description of what this snapshot captures
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,

    /// ISO 8601 timestamp of snapshot creation
    pub created_at: String,

    /// Per-tab state at the time of the snapshot
    pub tab_states: Vec<TabSnapshot>,
}

/// Captured state of a single tab within a snapshot.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TabSnapshot {
    /// The tab's identifier
    pub tab_id: String,

    /// URL the tab was displaying
    pub url: String,

    /// Page title at snapshot time
    #[serde(skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,

    /// Cookies associated with this tab's domain
    pub cookies: Vec<CookieInfo>,

    /// Contents of `window.localStorage`
    pub local_storage: HashMap<String, String>,

    /// Contents of `window.sessionStorage`
    pub session_storage: HashMap<String, String>,
}

// ============================================================================
// Session Manager
// ============================================================================

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

// ============================================================================
// JavaScript Helpers for Cookie / Storage Extraction
// ============================================================================

/// JavaScript generation utilities for extracting and restoring
/// browser state (cookies, localStorage, sessionStorage).
impl SessionManager {
    /// Generate JavaScript that extracts all cookies visible to the page.
    ///
    /// Returns a self-invoking function that produces a JSON string array
    /// of cookie objects. Note: `httpOnly` cookies are not accessible from
    /// JS; use the CDP `Network.getCookies` command for those.
    pub fn get_cookies_script() -> &'static str {
        r#"(() => {
    const cookies = [];
    if (!document.cookie) return JSON.stringify(cookies);

    document.cookie.split(';').forEach(pair => {
        const trimmed = pair.trim();
        if (!trimmed) return;
        const eqIdx = trimmed.indexOf('=');
        if (eqIdx < 0) return;

        const name = decodeURIComponent(trimmed.substring(0, eqIdx).trim());
        const value = decodeURIComponent(trimmed.substring(eqIdx + 1).trim());

        cookies.push({
            name: name,
            value: value,
            domain: window.location.hostname,
            path: '/',
            expires: null,
            http_only: false,
            secure: window.location.protocol === 'https:',
            same_site: null
        });
    });

    return JSON.stringify(cookies);
})()"#
    }

    /// Generate JavaScript that sets a single cookie from a `CookieInfo`.
    ///
    /// The returned script calls `document.cookie = ...` with the
    /// appropriate attributes.
    pub fn set_cookie_script(cookie: &CookieInfo) -> String {
        let mut parts = vec![format!(
            "{}={}",
            js_encode_uri_component(&cookie.name),
            js_encode_uri_component(&cookie.value)
        )];

        if !cookie.domain.is_empty() {
            parts.push(format!("domain={}", cookie.domain));
        }
        if !cookie.path.is_empty() {
            parts.push(format!("path={}", cookie.path));
        }
        if cookie.secure {
            parts.push("secure".to_string());
        }
        if let Some(ref same_site) = cookie.same_site {
            parts.push(format!("samesite={}", same_site));
        }
        if let Some(ref expires) = cookie.expires {
            parts.push(format!("expires={}", expires));
        }

        let cookie_str = parts.join("; ");
        format!(
            r#"(() => {{
    document.cookie = "{}";
    return true;
}})()"#,
            cookie_str.replace('\\', "\\\\").replace('"', "\\\"")
        )
    }

    /// Generate JavaScript that reads all `localStorage` entries.
    ///
    /// Returns a JSON object mapping keys to values.
    pub fn get_local_storage_script() -> &'static str {
        r#"(() => {
    const data = {};
    try {
        for (let i = 0; i < localStorage.length; i++) {
            const key = localStorage.key(i);
            if (key !== null) {
                data[key] = localStorage.getItem(key);
            }
        }
    } catch (e) {
        // localStorage may be blocked by security policy
        return JSON.stringify({ __error: e.message });
    }
    return JSON.stringify(data);
})()"#
    }

    /// Generate JavaScript that restores `localStorage` from a map.
    pub fn set_local_storage_script(entries: &HashMap<String, String>) -> String {
        let json = serde_json::to_string(entries).unwrap_or_else(|_| "{}".to_string());
        format!(
            r#"(() => {{
    try {{
        const entries = JSON.parse('{}');
        for (const [key, value] of Object.entries(entries)) {{
            localStorage.setItem(key, value);
        }}
        return true;
    }} catch (e) {{
        return false;
    }}
}})()"#,
            json.replace('\\', "\\\\").replace('\'', "\\'")
        )
    }

    /// Generate JavaScript that reads all `sessionStorage` entries.
    ///
    /// Returns a JSON object mapping keys to values.
    pub fn get_session_storage_script() -> &'static str {
        r#"(() => {
    const data = {};
    try {
        for (let i = 0; i < sessionStorage.length; i++) {
            const key = sessionStorage.key(i);
            if (key !== null) {
                data[key] = sessionStorage.getItem(key);
            }
        }
    } catch (e) {
        // sessionStorage may be blocked by security policy
        return JSON.stringify({ __error: e.message });
    }
    return JSON.stringify(data);
})()"#
    }

    /// Generate JavaScript that restores `sessionStorage` from a map.
    pub fn set_session_storage_script(entries: &HashMap<String, String>) -> String {
        let json = serde_json::to_string(entries).unwrap_or_else(|_| "{}".to_string());
        format!(
            r#"(() => {{
    try {{
        const entries = JSON.parse('{}');
        for (const [key, value] of Object.entries(entries)) {{
            sessionStorage.setItem(key, value);
        }}
        return true;
    }} catch (e) {{
        return false;
    }}
}})()"#,
            json.replace('\\', "\\\\").replace('\'', "\\'")
        )
    }

    /// Generate JavaScript that clears all cookies for the current domain.
    pub fn clear_cookies_script() -> &'static str {
        r#"(() => {
    const cookies = document.cookie.split(';');
    const paths = ['/', window.location.pathname];
    const domain = window.location.hostname;
    const domainParts = domain.split('.');

    // Build list of domain variations to try
    const domains = ['', domain];
    for (let i = 1; i < domainParts.length; i++) {
        domains.push('.' + domainParts.slice(i).join('.'));
    }

    let cleared = 0;
    cookies.forEach(cookie => {
        const eqIdx = cookie.indexOf('=');
        if (eqIdx < 0) return;
        const name = cookie.substring(0, eqIdx).trim();
        if (!name) return;

        // Try clearing with various domain/path combinations
        domains.forEach(d => {
            paths.forEach(p => {
                let str = name + '=; expires=Thu, 01 Jan 1970 00:00:00 GMT; path=' + p;
                if (d) str += '; domain=' + d;
                document.cookie = str;
            });
        });
        cleared++;
    });

    return JSON.stringify({ cleared: cleared });
})()"#
    }

    /// Generate JavaScript that captures a complete tab state snapshot
    /// (cookies, localStorage, sessionStorage) in one call.
    pub fn capture_tab_state_script() -> &'static str {
        r#"(() => {
    const state = {
        url: window.location.href,
        title: document.title,
        cookies: [],
        local_storage: {},
        session_storage: {}
    };

    // Cookies
    if (document.cookie) {
        document.cookie.split(';').forEach(pair => {
            const trimmed = pair.trim();
            if (!trimmed) return;
            const eqIdx = trimmed.indexOf('=');
            if (eqIdx < 0) return;
            state.cookies.push({
                name: decodeURIComponent(trimmed.substring(0, eqIdx).trim()),
                value: decodeURIComponent(trimmed.substring(eqIdx + 1).trim()),
                domain: window.location.hostname,
                path: '/',
                expires: null,
                http_only: false,
                secure: window.location.protocol === 'https:',
                same_site: null
            });
        });
    }

    // localStorage
    try {
        for (let i = 0; i < localStorage.length; i++) {
            const key = localStorage.key(i);
            if (key !== null) {
                state.local_storage[key] = localStorage.getItem(key);
            }
        }
    } catch (e) { /* blocked */ }

    // sessionStorage
    try {
        for (let i = 0; i < sessionStorage.length; i++) {
            const key = sessionStorage.key(i);
            if (key !== null) {
                state.session_storage[key] = sessionStorage.getItem(key);
            }
        }
    } catch (e) { /* blocked */ }

    return JSON.stringify(state);
})()"#
    }
}

/// Minimal URI-component encoding for cookie values.
///
/// Encodes characters that are problematic in `document.cookie`
/// assignments: `=`, `;`, space, and `%`.
fn js_encode_uri_component(s: &str) -> String {
    let mut result = String::with_capacity(s.len());
    for ch in s.chars() {
        match ch {
            '%' => result.push_str("%25"),
            '=' => result.push_str("%3D"),
            ';' => result.push_str("%3B"),
            ' ' => result.push_str("%20"),
            _ => result.push(ch),
        }
    }
    result
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_create_session() {
        let manager = SessionManager::new();
        let session = manager.create_session(Some("Test Session".to_string())).await;

        assert!(!session.id.is_empty());
        assert_eq!(session.name, Some("Test Session".to_string()));
        assert!(session.tabs.is_empty());
        assert!(session.storage.is_empty());
        assert!(session.cookies.is_empty());
        assert!(session.history.is_empty());
        assert!(session.snapshots.is_empty());
    }

    #[tokio::test]
    async fn test_create_session_generates_unique_ids() {
        let manager = SessionManager::new();
        let s1 = manager.create_session(None).await;
        let s2 = manager.create_session(None).await;
        assert_ne!(s1.id, s2.id);
    }

    #[tokio::test]
    async fn test_get_session() {
        let manager = SessionManager::new();
        let session = manager.create_session(Some("Lookup".to_string())).await;
        let id = session.id.clone();

        let found = manager.get_session(&id).await;
        assert!(found.is_some());
        assert_eq!(found.unwrap().name, Some("Lookup".to_string()));

        let missing = manager.get_session("non-existent").await;
        assert!(missing.is_none());
    }

    #[tokio::test]
    async fn test_list_sessions() {
        let manager = SessionManager::new();
        assert!(manager.list_sessions().await.is_empty());

        manager.create_session(Some("A".to_string())).await;
        manager.create_session(Some("B".to_string())).await;

        let list = manager.list_sessions().await;
        assert_eq!(list.len(), 2);
    }

    #[tokio::test]
    async fn test_delete_session() {
        let manager = SessionManager::new();
        let session = manager.create_session(None).await;
        let id = session.id.clone();

        assert!(manager.delete_session(&id).await);
        assert!(manager.get_session(&id).await.is_none());

        // Deleting again returns false
        assert!(!manager.delete_session(&id).await);
    }

    #[tokio::test]
    async fn test_add_and_remove_tab() {
        let manager = SessionManager::new();
        let session = manager.create_session(None).await;
        let id = session.id.clone();

        assert!(manager.add_tab(&id, "tab_1".to_string()).await);
        assert!(manager.add_tab(&id, "tab_2".to_string()).await);

        // Adding the same tab again should not duplicate
        assert!(manager.add_tab(&id, "tab_1".to_string()).await);

        let session = manager.get_session(&id).await.unwrap();
        assert_eq!(session.tabs.len(), 2);

        assert!(manager.remove_tab(&id, "tab_1").await);
        let session = manager.get_session(&id).await.unwrap();
        assert_eq!(session.tabs.len(), 1);
        assert_eq!(session.tabs[0], "tab_2");
    }

    #[tokio::test]
    async fn test_tab_operations_on_missing_session() {
        let manager = SessionManager::new();
        assert!(!manager.add_tab("nope", "tab_1".to_string()).await);
        assert!(!manager.remove_tab("nope", "tab_1").await);
    }

    #[tokio::test]
    async fn test_storage_operations() {
        let manager = SessionManager::new();
        let session = manager.create_session(None).await;
        let id = session.id.clone();

        // Set values
        assert!(manager
            .set_storage(&id, "key1".to_string(), serde_json::json!("value1"))
            .await);
        assert!(manager
            .set_storage(&id, "key2".to_string(), serde_json::json!(42))
            .await);

        // Get values
        let v1 = manager.get_storage(&id, "key1").await;
        assert_eq!(v1, Some(serde_json::json!("value1")));

        let v2 = manager.get_storage(&id, "key2").await;
        assert_eq!(v2, Some(serde_json::json!(42)));

        // Missing key
        assert!(manager.get_storage(&id, "missing").await.is_none());

        // Delete key
        assert!(manager.delete_storage(&id, "key1").await);
        assert!(manager.get_storage(&id, "key1").await.is_none());

        // Delete missing key
        assert!(!manager.delete_storage(&id, "key1").await);
    }

    #[tokio::test]
    async fn test_storage_on_missing_session() {
        let manager = SessionManager::new();
        assert!(!manager
            .set_storage("nope", "k".to_string(), serde_json::json!(1))
            .await);
        assert!(manager.get_storage("nope", "k").await.is_none());
        assert!(!manager.delete_storage("nope", "k").await);
    }

    #[tokio::test]
    async fn test_set_cookies() {
        let manager = SessionManager::new();
        let session = manager.create_session(None).await;
        let id = session.id.clone();

        let cookies = vec![
            CookieInfo {
                name: "session_id".to_string(),
                value: "abc123".to_string(),
                domain: "example.com".to_string(),
                path: "/".to_string(),
                expires: None,
                http_only: true,
                secure: true,
                same_site: Some("Lax".to_string()),
            },
        ];

        assert!(manager.set_cookies(&id, cookies).await);

        let session = manager.get_session(&id).await.unwrap();
        assert_eq!(session.cookies.len(), 1);
        assert_eq!(session.cookies[0].name, "session_id");
    }

    #[tokio::test]
    async fn test_add_history() {
        let manager = SessionManager::new();
        let session = manager.create_session(None).await;
        let id = session.id.clone();

        manager
            .add_history(
                &id,
                HistoryEntry {
                    url: "https://example.com".to_string(),
                    title: Some("Example".to_string()),
                    timestamp: Utc::now().to_rfc3339(),
                    tab_id: "tab_1".to_string(),
                },
            )
            .await;

        manager
            .add_history(
                &id,
                HistoryEntry {
                    url: "https://example.com/page2".to_string(),
                    title: Some("Page 2".to_string()),
                    timestamp: Utc::now().to_rfc3339(),
                    tab_id: "tab_1".to_string(),
                },
            )
            .await;

        let session = manager.get_session(&id).await.unwrap();
        assert_eq!(session.history.len(), 2);
        assert_eq!(session.history[0].url, "https://example.com");
        assert_eq!(session.history[1].url, "https://example.com/page2");
    }

    #[tokio::test]
    async fn test_create_and_get_snapshot() {
        let manager = SessionManager::new();
        let session = manager.create_session(None).await;
        let id = session.id.clone();

        let tab_states = vec![TabSnapshot {
            tab_id: "tab_1".to_string(),
            url: "https://example.com".to_string(),
            title: Some("Example".to_string()),
            cookies: vec![],
            local_storage: HashMap::from([("theme".to_string(), "dark".to_string())]),
            session_storage: HashMap::new(),
        }];

        let snapshot = manager
            .create_snapshot(
                &id,
                "before_login".to_string(),
                Some("State before login flow".to_string()),
                tab_states,
            )
            .await;

        assert!(snapshot.is_some());
        let snapshot = snapshot.unwrap();
        assert_eq!(snapshot.name, "before_login");
        assert_eq!(snapshot.tab_states.len(), 1);
        assert_eq!(snapshot.tab_states[0].local_storage.get("theme"), Some(&"dark".to_string()));

        // Retrieve by name
        let found = manager.get_snapshot(&id, "before_login").await;
        assert!(found.is_some());

        // Missing snapshot
        let missing = manager.get_snapshot(&id, "nonexistent").await;
        assert!(missing.is_none());
    }

    #[tokio::test]
    async fn test_list_and_delete_snapshots() {
        let manager = SessionManager::new();
        let session = manager.create_session(None).await;
        let id = session.id.clone();

        manager
            .create_snapshot(&id, "snap1".to_string(), None, vec![])
            .await;
        manager
            .create_snapshot(&id, "snap2".to_string(), None, vec![])
            .await;

        let names = manager.list_snapshots(&id).await;
        assert_eq!(names.len(), 2);

        assert!(manager.delete_snapshot(&id, "snap1").await);
        let names = manager.list_snapshots(&id).await;
        assert_eq!(names.len(), 1);
        assert_eq!(names[0], "snap2");

        // Delete non-existent
        assert!(!manager.delete_snapshot(&id, "snap1").await);
    }

    #[tokio::test]
    async fn test_snapshot_on_missing_session() {
        let manager = SessionManager::new();
        let result = manager
            .create_snapshot("nope", "snap".to_string(), None, vec![])
            .await;
        assert!(result.is_none());
    }

    #[tokio::test]
    async fn test_touch_updates_last_activity() {
        let manager = SessionManager::new();
        let session = manager.create_session(None).await;
        let id = session.id.clone();
        let original_activity = session.last_activity.clone();

        // Small delay to ensure timestamp changes
        tokio::time::sleep(std::time::Duration::from_millis(10)).await;

        assert!(manager.touch(&id).await);
        let session = manager.get_session(&id).await.unwrap();
        assert!(session.last_activity >= original_activity);

        assert!(!manager.touch("nope").await);
    }

    #[tokio::test]
    async fn test_session_count() {
        let manager = SessionManager::new();
        assert_eq!(manager.session_count().await, 0);

        let s1 = manager.create_session(None).await;
        assert_eq!(manager.session_count().await, 1);

        manager.create_session(None).await;
        assert_eq!(manager.session_count().await, 2);

        manager.delete_session(&s1.id).await;
        assert_eq!(manager.session_count().await, 1);
    }

    #[test]
    fn test_session_serialization() {
        let session = Session {
            id: "test-id".to_string(),
            name: Some("Test".to_string()),
            created_at: "2026-01-01T00:00:00Z".to_string(),
            last_activity: "2026-01-01T00:00:00Z".to_string(),
            tabs: vec!["tab_1".to_string()],
            storage: HashMap::from([("key".to_string(), serde_json::json!("value"))]),
            cookies: vec![],
            history: vec![],
            snapshots: vec![],
        };

        let json = serde_json::to_string(&session).unwrap();
        let deserialized: Session = serde_json::from_str(&json).unwrap();
        assert_eq!(deserialized.id, "test-id");
        assert_eq!(deserialized.name, Some("Test".to_string()));
        assert_eq!(deserialized.tabs.len(), 1);
    }

    #[test]
    fn test_cookie_info_serialization() {
        let cookie = CookieInfo {
            name: "sid".to_string(),
            value: "abc".to_string(),
            domain: ".example.com".to_string(),
            path: "/".to_string(),
            expires: Some("Thu, 01 Jan 2030 00:00:00 GMT".to_string()),
            http_only: true,
            secure: true,
            same_site: Some("Strict".to_string()),
        };

        let json = serde_json::to_string(&cookie).unwrap();
        assert!(json.contains("sid"));
        assert!(json.contains("Strict"));

        let deserialized: CookieInfo = serde_json::from_str(&json).unwrap();
        assert_eq!(deserialized.name, "sid");
        assert!(deserialized.http_only);
    }

    #[test]
    fn test_history_entry_serialization() {
        let entry = HistoryEntry {
            url: "https://example.com".to_string(),
            title: Some("Example".to_string()),
            timestamp: "2026-01-01T12:00:00Z".to_string(),
            tab_id: "tab_1".to_string(),
        };

        let json = serde_json::to_string(&entry).unwrap();
        let deserialized: HistoryEntry = serde_json::from_str(&json).unwrap();
        assert_eq!(deserialized.url, "https://example.com");
        assert_eq!(deserialized.tab_id, "tab_1");
    }

    #[test]
    fn test_get_cookies_script_content() {
        let script = SessionManager::get_cookies_script();
        assert!(script.contains("document.cookie"));
        assert!(script.contains("JSON.stringify"));
        assert!(script.contains("decodeURIComponent"));
    }

    #[test]
    fn test_set_cookie_script_content() {
        let cookie = CookieInfo {
            name: "test".to_string(),
            value: "value".to_string(),
            domain: ".example.com".to_string(),
            path: "/app".to_string(),
            expires: Some("Thu, 01 Jan 2030 00:00:00 GMT".to_string()),
            http_only: false,
            secure: true,
            same_site: Some("Lax".to_string()),
        };

        let script = SessionManager::set_cookie_script(&cookie);
        assert!(script.contains("document.cookie"));
        assert!(script.contains("domain=.example.com"));
        assert!(script.contains("path=/app"));
        assert!(script.contains("secure"));
        assert!(script.contains("samesite=Lax"));
        assert!(script.contains("expires="));
    }

    #[test]
    fn test_set_cookie_script_minimal() {
        let cookie = CookieInfo {
            name: "simple".to_string(),
            value: "val".to_string(),
            domain: String::new(),
            path: "/".to_string(),
            expires: None,
            http_only: false,
            secure: false,
            same_site: None,
        };

        let script = SessionManager::set_cookie_script(&cookie);
        assert!(script.contains("document.cookie"));
        assert!(script.contains("path=/"));
        // Should NOT contain secure, samesite, or expires
        assert!(!script.contains("secure"));
        assert!(!script.contains("samesite"));
        // "expires" should not appear (no expiry set)
        assert!(!script.contains("expires="));
    }

    #[test]
    fn test_get_local_storage_script_content() {
        let script = SessionManager::get_local_storage_script();
        assert!(script.contains("localStorage"));
        assert!(script.contains("JSON.stringify"));
        assert!(script.contains("getItem"));
    }

    #[test]
    fn test_set_local_storage_script_content() {
        let entries = HashMap::from([
            ("theme".to_string(), "dark".to_string()),
            ("lang".to_string(), "en".to_string()),
        ]);
        let script = SessionManager::set_local_storage_script(&entries);
        assert!(script.contains("localStorage.setItem"));
        assert!(script.contains("JSON.parse"));
    }

    #[test]
    fn test_get_session_storage_script_content() {
        let script = SessionManager::get_session_storage_script();
        assert!(script.contains("sessionStorage"));
        assert!(script.contains("JSON.stringify"));
        assert!(script.contains("getItem"));
    }

    #[test]
    fn test_set_session_storage_script_content() {
        let entries = HashMap::from([("token".to_string(), "xyz".to_string())]);
        let script = SessionManager::set_session_storage_script(&entries);
        assert!(script.contains("sessionStorage.setItem"));
        assert!(script.contains("JSON.parse"));
    }

    #[test]
    fn test_clear_cookies_script_content() {
        let script = SessionManager::clear_cookies_script();
        assert!(script.contains("document.cookie"));
        assert!(script.contains("expires=Thu, 01 Jan 1970"));
        assert!(script.contains("JSON.stringify"));
    }

    #[test]
    fn test_capture_tab_state_script_content() {
        let script = SessionManager::capture_tab_state_script();
        assert!(script.contains("document.cookie"));
        assert!(script.contains("localStorage"));
        assert!(script.contains("sessionStorage"));
        assert!(script.contains("window.location.href"));
        assert!(script.contains("document.title"));
        assert!(script.contains("JSON.stringify"));
    }

    #[test]
    fn test_js_encode_uri_component() {
        assert_eq!(js_encode_uri_component("hello"), "hello");
        assert_eq!(js_encode_uri_component("a=b"), "a%3Db");
        assert_eq!(js_encode_uri_component("a;b"), "a%3Bb");
        assert_eq!(js_encode_uri_component("a b"), "a%20b");
        assert_eq!(js_encode_uri_component("100%"), "100%25");
        assert_eq!(
            js_encode_uri_component("key=val; path=/"),
            "key%3Dval%3B%20path%3D/"
        );
    }

    #[test]
    fn test_default_session_manager() {
        let _manager = SessionManager::default();
        // Just ensure Default trait works without panic
    }
}
