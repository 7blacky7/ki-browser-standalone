//! Session data types for browser state persistence.
//!
//! Defines the core structures for sessions, cookies, navigation history,
//! and state snapshots used by the session management subsystem.

use std::collections::HashMap;

use serde::{Deserialize, Serialize};

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
