//! Route handlers for batch operations and session management
//!
//! Provides Axum HTTP handlers for executing batch browser commands
//! (sequential or parallel) and managing sessions with cookie, storage,
//! and snapshot support.
//!
//! Submodules:
//! - `types`: Request/response DTOs
//! - `helpers`: IPC response parsing utilities
//! - `batch_handlers`: Batch command execution (sequential + parallel)
//! - `session_handlers`: Session lifecycle, storage, cookies, snapshots

pub mod types;
mod helpers;
mod batch_handlers;
mod session_handlers;

pub use types::*;

use axum::{
    routing::{delete, get, post},
    Router,
};

use crate::api::server::AppState;
use crate::api::session::SessionManager;

/// Lazy-initialized global session manager.
///
/// Since `AppState` must not be modified, we use a global `SessionManager`
/// instance protected by `Arc<RwLock<>>` for thread-safe access from
/// async handlers.
static SESSION_MANAGER: once_cell::sync::Lazy<SessionManager> =
    once_cell::sync::Lazy::new(SessionManager::new);

/// Build the batch and session router.
///
/// All routes use `AppState` as the Axum state for IPC access.
pub fn batch_session_routes() -> Router<AppState> {
    Router::new()
        // Batch operations
        .route("/batch", post(batch_handlers::execute_batch))
        .route("/batch/navigate-and-extract", post(batch_handlers::batch_navigate_extract))
        // Session lifecycle
        .route("/session/start", post(session_handlers::create_session))
        .route("/session/list", get(session_handlers::list_sessions))
        .route("/session/:id", get(session_handlers::get_session))
        .route("/session/:id", delete(session_handlers::delete_session))
        // Session key-value storage
        .route("/session/:id/storage", post(session_handlers::set_storage))
        .route("/session/:id/storage/:key", get(session_handlers::get_storage))
        // Cookie management via JS injection
        .route("/tabs/:tab_id/cookies", get(session_handlers::get_cookies))
        .route("/tabs/:tab_id/cookies", post(session_handlers::set_cookies))
        // LocalStorage via JS injection
        .route("/tabs/:tab_id/local-storage", get(session_handlers::get_local_storage))
        // Session snapshots
        .route("/session/:id/snapshot", post(session_handlers::create_snapshot))
        .route("/session/:id/snapshots", get(session_handlers::list_snapshots))
}

#[cfg(test)]
mod tests {
    use super::*;
    use super::helpers::*;
    use std::collections::HashMap;
    use crate::api::session::CookieInfo;

    #[test]
    fn test_parse_cookies_from_response_none() {
        let result = parse_cookies_from_response(None);
        assert!(result.is_empty());
    }

    #[test]
    fn test_parse_cookies_from_response_string() {
        let json = serde_json::json!(r#"[{"name":"sid","value":"abc","domain":"example.com","path":"/","expires":null,"http_only":false,"secure":true,"same_site":null}]"#);
        let result = parse_cookies_from_response(Some(json));
        assert_eq!(result.len(), 1);
        assert_eq!(result[0].name, "sid");
        assert_eq!(result[0].value, "abc");
    }

    #[test]
    fn test_parse_cookies_from_response_array() {
        let json = serde_json::json!([{
            "name": "token",
            "value": "xyz",
            "domain": "test.com",
            "path": "/",
            "http_only": false,
            "secure": false
        }]);
        let result = parse_cookies_from_response(Some(json));
        assert_eq!(result.len(), 1);
        assert_eq!(result[0].name, "token");
    }

    #[test]
    fn test_parse_storage_from_response_none() {
        let result = parse_storage_from_response(None);
        assert!(result.is_empty());
    }

    #[test]
    fn test_parse_storage_from_response_string() {
        let json = serde_json::json!(r#"{"theme":"dark","lang":"en"}"#);
        let result = parse_storage_from_response(Some(json));
        assert_eq!(result.len(), 2);
        assert_eq!(result.get("theme"), Some(&"dark".to_string()));
        assert_eq!(result.get("lang"), Some(&"en".to_string()));
    }

    #[test]
    fn test_parse_storage_from_response_object() {
        let json = serde_json::json!({"key1": "val1", "key2": "val2"});
        let result = parse_storage_from_response(Some(json));
        assert_eq!(result.len(), 2);
    }

    #[test]
    fn test_parse_tab_snapshot_string_data() {
        let data = serde_json::json!(
            r#"{"url":"https://example.com","title":"Example","cookies":[],"local_storage":{"theme":"dark"},"session_storage":{}}"#
        );
        let snapshot = parse_tab_snapshot("tab_1", data);
        assert_eq!(snapshot.tab_id, "tab_1");
        assert_eq!(snapshot.url, "https://example.com");
        assert_eq!(snapshot.title, Some("Example".to_string()));
        assert_eq!(
            snapshot.local_storage.get("theme"),
            Some(&"dark".to_string())
        );
        assert!(snapshot.cookies.is_empty());
        assert!(snapshot.session_storage.is_empty());
    }

    #[test]
    fn test_parse_tab_snapshot_object_data() {
        let data = serde_json::json!({
            "url": "https://test.com",
            "title": "Test",
            "cookies": [{
                "name": "sid",
                "value": "123",
                "domain": "test.com",
                "path": "/",
                "http_only": false,
                "secure": true
            }],
            "local_storage": {},
            "session_storage": {"token": "abc"}
        });
        let snapshot = parse_tab_snapshot("tab_2", data);
        assert_eq!(snapshot.url, "https://test.com");
        assert_eq!(snapshot.cookies.len(), 1);
        assert_eq!(
            snapshot.session_storage.get("token"),
            Some(&"abc".to_string())
        );
    }

    #[test]
    fn test_parse_tab_snapshot_null_data() {
        let snapshot = parse_tab_snapshot("tab_x", serde_json::Value::Null);
        assert_eq!(snapshot.tab_id, "tab_x");
        assert_eq!(snapshot.url, "");
        assert!(snapshot.title.is_none());
        assert!(snapshot.cookies.is_empty());
    }

    #[test]
    fn test_create_session_request_deserialization() {
        let json = r#"{"name": "My Session"}"#;
        let req: CreateSessionRequest = serde_json::from_str(json).unwrap();
        assert_eq!(req.name, Some("My Session".to_string()));

        let json_empty = r#"{}"#;
        let req: CreateSessionRequest = serde_json::from_str(json_empty).unwrap();
        assert!(req.name.is_none());
    }

    #[test]
    fn test_set_storage_request_deserialization() {
        let json = r#"{"key": "results", "value": [1, 2, 3]}"#;
        let req: SetStorageRequest = serde_json::from_str(json).unwrap();
        assert_eq!(req.key, "results");
        assert_eq!(req.value, serde_json::json!([1, 2, 3]));
    }

    #[test]
    fn test_set_cookie_request_deserialization() {
        let json = r#"{"name": "token", "value": "abc123", "secure": true}"#;
        let req: SetCookieRequest = serde_json::from_str(json).unwrap();
        assert_eq!(req.name, "token");
        assert_eq!(req.value, "abc123");
        assert!(req.secure);
        assert_eq!(req.path, "/"); // default
        assert!(req.domain.is_none());
    }

    #[test]
    fn test_create_snapshot_request_deserialization() {
        let json = r#"{"name": "before_login", "description": "State before login"}"#;
        let req: CreateSnapshotRequest = serde_json::from_str(json).unwrap();
        assert_eq!(req.name, "before_login");
        assert_eq!(
            req.description,
            Some("State before login".to_string())
        );
    }

    #[test]
    fn test_snapshot_summary_serialization() {
        let summary = SnapshotSummary {
            name: "checkpoint1".to_string(),
            description: None,
            created_at: "2026-01-01T00:00:00Z".to_string(),
            tab_count: 3,
        };
        let json = serde_json::to_string(&summary).unwrap();
        assert!(json.contains("checkpoint1"));
        assert!(json.contains("tab_count"));
        assert!(!json.contains("description"));
    }
}
