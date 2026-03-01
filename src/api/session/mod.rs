//! Session management for browser state persistence
//!
//! Provides session lifecycle management for AI agents, including
//! per-session tab tracking, key-value storage, cookie management,
//! navigation history, and named state snapshots for checkpoint/restore
//! workflows.
//!
//! Submodules:
//! - `types`: Core data structures (Session, CookieInfo, TabSnapshot, etc.)
//! - `manager`: Thread-safe SessionManager with async operations
//! - `scripts`: JavaScript generation for cookie/storage extraction

pub mod types;
pub mod manager;
mod scripts;

pub use types::*;
pub use manager::SessionManager;

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;
    use chrono::Utc;
    use super::scripts::js_encode_uri_component;

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

        assert!(!manager.delete_session(&id).await);
    }

    #[tokio::test]
    async fn test_add_and_remove_tab() {
        let manager = SessionManager::new();
        let session = manager.create_session(None).await;
        let id = session.id.clone();

        assert!(manager.add_tab(&id, "tab_1".to_string()).await);
        assert!(manager.add_tab(&id, "tab_2".to_string()).await);
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

        assert!(manager.set_storage(&id, "key1".to_string(), serde_json::json!("value1")).await);
        assert!(manager.set_storage(&id, "key2".to_string(), serde_json::json!(42)).await);

        assert_eq!(manager.get_storage(&id, "key1").await, Some(serde_json::json!("value1")));
        assert_eq!(manager.get_storage(&id, "key2").await, Some(serde_json::json!(42)));
        assert!(manager.get_storage(&id, "missing").await.is_none());

        assert!(manager.delete_storage(&id, "key1").await);
        assert!(manager.get_storage(&id, "key1").await.is_none());
        assert!(!manager.delete_storage(&id, "key1").await);
    }

    #[tokio::test]
    async fn test_storage_on_missing_session() {
        let manager = SessionManager::new();
        assert!(!manager.set_storage("nope", "k".to_string(), serde_json::json!(1)).await);
        assert!(manager.get_storage("nope", "k").await.is_none());
        assert!(!manager.delete_storage("nope", "k").await);
    }

    #[tokio::test]
    async fn test_set_cookies() {
        let manager = SessionManager::new();
        let session = manager.create_session(None).await;
        let id = session.id.clone();

        let cookies = vec![CookieInfo {
            name: "session_id".to_string(),
            value: "abc123".to_string(),
            domain: "example.com".to_string(),
            path: "/".to_string(),
            expires: None,
            http_only: true,
            secure: true,
            same_site: Some("Lax".to_string()),
        }];

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

        manager.add_history(&id, HistoryEntry {
            url: "https://example.com".to_string(),
            title: Some("Example".to_string()),
            timestamp: Utc::now().to_rfc3339(),
            tab_id: "tab_1".to_string(),
        }).await;

        manager.add_history(&id, HistoryEntry {
            url: "https://example.com/page2".to_string(),
            title: Some("Page 2".to_string()),
            timestamp: Utc::now().to_rfc3339(),
            tab_id: "tab_1".to_string(),
        }).await;

        let session = manager.get_session(&id).await.unwrap();
        assert_eq!(session.history.len(), 2);
        assert_eq!(session.history[0].url, "https://example.com");
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

        let snapshot = manager.create_snapshot(&id, "before_login".to_string(), Some("State before login flow".to_string()), tab_states).await;
        assert!(snapshot.is_some());
        let snapshot = snapshot.unwrap();
        assert_eq!(snapshot.name, "before_login");
        assert_eq!(snapshot.tab_states[0].local_storage.get("theme"), Some(&"dark".to_string()));

        assert!(manager.get_snapshot(&id, "before_login").await.is_some());
        assert!(manager.get_snapshot(&id, "nonexistent").await.is_none());
    }

    #[tokio::test]
    async fn test_list_and_delete_snapshots() {
        let manager = SessionManager::new();
        let session = manager.create_session(None).await;
        let id = session.id.clone();

        manager.create_snapshot(&id, "snap1".to_string(), None, vec![]).await;
        manager.create_snapshot(&id, "snap2".to_string(), None, vec![]).await;

        assert_eq!(manager.list_snapshots(&id).await.len(), 2);

        assert!(manager.delete_snapshot(&id, "snap1").await);
        let names = manager.list_snapshots(&id).await;
        assert_eq!(names.len(), 1);
        assert_eq!(names[0], "snap2");

        assert!(!manager.delete_snapshot(&id, "snap1").await);
    }

    #[tokio::test]
    async fn test_snapshot_on_missing_session() {
        let manager = SessionManager::new();
        assert!(manager.create_snapshot("nope", "snap".to_string(), None, vec![]).await.is_none());
    }

    #[tokio::test]
    async fn test_touch_updates_last_activity() {
        let manager = SessionManager::new();
        let session = manager.create_session(None).await;
        let id = session.id.clone();
        let original_activity = session.last_activity.clone();

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
    }

    #[test]
    fn test_get_cookies_script_content() {
        let script = SessionManager::get_cookies_script();
        assert!(script.contains("document.cookie"));
        assert!(script.contains("JSON.stringify"));
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
        assert!(script.contains("secure"));
        assert!(script.contains("samesite=Lax"));
    }

    #[test]
    fn test_capture_tab_state_script_content() {
        let script = SessionManager::capture_tab_state_script();
        assert!(script.contains("localStorage"));
        assert!(script.contains("sessionStorage"));
        assert!(script.contains("document.title"));
    }

    #[test]
    fn test_js_encode_uri_component() {
        assert_eq!(js_encode_uri_component("hello"), "hello");
        assert_eq!(js_encode_uri_component("a=b"), "a%3Db");
        assert_eq!(js_encode_uri_component("a;b"), "a%3Bb");
        assert_eq!(js_encode_uri_component("a b"), "a%20b");
        assert_eq!(js_encode_uri_component("100%"), "100%25");
    }

    #[test]
    fn test_default_session_manager() {
        let _manager = SessionManager::default();
    }
}
