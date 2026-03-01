//! Request and response types for batch operations and session management routes.

use serde::{Deserialize, Serialize};

/// Request body for creating a new session.
#[derive(Debug, Deserialize)]
pub struct CreateSessionRequest {
    /// Optional human-readable session name.
    #[serde(default)]
    pub name: Option<String>,
}

/// Request body for setting a key-value pair in session storage.
#[derive(Debug, Deserialize)]
pub struct SetStorageRequest {
    /// Storage key.
    pub key: String,
    /// Storage value (arbitrary JSON).
    pub value: serde_json::Value,
}

/// Response for a storage get operation.
#[derive(Debug, Serialize)]
pub struct StorageValueResponse {
    pub key: String,
    pub value: serde_json::Value,
}

/// Request body for setting a cookie via JavaScript.
#[derive(Debug, Deserialize)]
pub struct SetCookieRequest {
    pub name: String,
    pub value: String,
    #[serde(default)]
    pub domain: Option<String>,
    #[serde(default = "default_path")]
    pub path: String,
    #[serde(default)]
    pub expires: Option<String>,
    #[serde(default)]
    pub secure: bool,
    #[serde(default)]
    pub same_site: Option<String>,
}

fn default_path() -> String {
    "/".to_string()
}

/// Request body for creating a session snapshot.
#[derive(Debug, Deserialize)]
pub struct CreateSnapshotRequest {
    /// Snapshot name (must be unique within the session).
    pub name: String,
    /// Optional description.
    #[serde(default)]
    pub description: Option<String>,
}

/// Summary information about a snapshot (used in list responses).
#[derive(Debug, Serialize)]
pub struct SnapshotSummary {
    pub name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    pub created_at: String,
    pub tab_count: usize,
}
