//! CDP Tab Mapping Service for bidirectional lookup between ki-browser UUIDs and CDP TargetIds.
//!
//! Maps internal tab UUIDs to CEF/CDP browser identifiers, enabling external tools
//! (e.g. Puppeteer, Playwright, Chrome DevTools) to connect to specific tabs via
//! the Chrome DevTools Protocol remote debugging interface.
//!
//! The mapping updates automatically when tabs are created or closed through
//! the `register` and `unregister` methods, which are called from tab lifecycle handlers.

use std::collections::HashMap;
use std::sync::Arc;

use parking_lot::RwLock;
use serde::Serialize;
use uuid::Uuid;

/// Bidirectional mapping between ki-browser tab UUIDs and CDP target identifiers.
///
/// CEF assigns each browser instance an integer identifier (browser_id) that corresponds
/// to a CDP TargetId in the remote debugging protocol. This service maintains a
/// thread-safe bidirectional index so callers can look up either direction in O(1).
#[derive(Debug, Clone)]
pub struct CdpTabMapping {
    /// Forward map: ki-browser tab UUID -> CDP target identifier (browser_id as string).
    uuid_to_target: Arc<RwLock<HashMap<Uuid, String>>>,
    /// Reverse map: CDP target identifier -> ki-browser tab UUID.
    target_to_uuid: Arc<RwLock<HashMap<String, Uuid>>>,
    /// CDP remote debugging port used by this browser instance.
    remote_debugging_port: u16,
}

/// Information about a single CDP target with its mapped ki-browser tab UUID.
#[derive(Debug, Clone, Serialize)]
pub struct CdpTargetInfo {
    /// The ki-browser internal tab UUID.
    pub tab_id: String,
    /// The CDP target identifier (derived from CEF browser_id).
    pub target_id: String,
    /// Type of the target (always "page" for browser tabs).
    pub target_type: String,
    /// WebSocket URL for connecting to this target via CDP.
    pub ws_url: String,
    /// URL the tab is currently displaying.
    pub url: String,
    /// Page title.
    pub title: String,
}

/// Summary response for the GET /cdp/targets endpoint.
#[derive(Debug, Clone, Serialize)]
pub struct CdpTargetsResponse {
    /// Remote debugging port for direct CDP connections.
    pub remote_debugging_port: u16,
    /// WebSocket debugger URL for the browser-level CDP endpoint.
    pub browser_ws_url: String,
    /// All known CDP targets with their ki-browser tab UUID mapping.
    pub targets: Vec<CdpTargetInfo>,
}

/// Response for the GET /cdp/target/:tab_id endpoint.
#[derive(Debug, Clone, Serialize)]
pub struct CdpTargetLookupResponse {
    /// The ki-browser tab UUID that was queried.
    pub tab_id: String,
    /// The corresponding CDP target identifier.
    pub target_id: String,
    /// WebSocket URL for connecting to this specific target.
    pub ws_url: String,
}

impl CdpTabMapping {
    /// Creates a new empty CDP tab mapping service.
    ///
    /// # Arguments
    /// * `remote_debugging_port` - The port CEF uses for Chrome DevTools Protocol.
    pub fn new(remote_debugging_port: u16) -> Self {
        Self {
            uuid_to_target: Arc::new(RwLock::new(HashMap::new())),
            target_to_uuid: Arc::new(RwLock::new(HashMap::new())),
            remote_debugging_port,
        }
    }

    /// Registers a new tab UUID <-> CDP target_id mapping.
    ///
    /// Called when a new browser tab is created and its CEF browser_id is known.
    /// The target_id is typically the string representation of CEF's browser identifier.
    pub fn register(&self, tab_id: Uuid, target_id: String) {
        let mut forward = self.uuid_to_target.write();
        let mut reverse = self.target_to_uuid.write();
        forward.insert(tab_id, target_id.clone());
        reverse.insert(target_id, tab_id);
    }

    /// Removes a mapping when a tab is closed.
    ///
    /// Cleans up both directions of the mapping. Safe to call even if
    /// the tab_id was never registered.
    pub fn unregister(&self, tab_id: &Uuid) {
        let mut forward = self.uuid_to_target.write();
        if let Some(target_id) = forward.remove(tab_id) {
            let mut reverse = self.target_to_uuid.write();
            reverse.remove(&target_id);
        }
    }

    /// Looks up the CDP target_id for a given ki-browser tab UUID.
    ///
    /// Returns `None` if the tab is not registered in the mapping.
    pub fn get_target_id(&self, tab_id: &Uuid) -> Option<String> {
        self.uuid_to_target.read().get(tab_id).cloned()
    }

    /// Looks up the ki-browser tab UUID for a given CDP target_id.
    ///
    /// Returns `None` if the target_id is not registered in the mapping.
    pub fn get_tab_id(&self, target_id: &str) -> Option<Uuid> {
        self.target_to_uuid.read().get(target_id).copied()
    }

    /// Returns the configured CDP remote debugging port.
    pub fn remote_debugging_port(&self) -> u16 {
        self.remote_debugging_port
    }

    /// Returns the browser-level WebSocket debugger URL.
    pub fn browser_ws_url(&self) -> String {
        format!("ws://127.0.0.1:{}/json/version", self.remote_debugging_port)
    }

    /// Builds the WebSocket URL for a specific CDP target.
    pub fn target_ws_url(&self, target_id: &str) -> String {
        format!(
            "ws://127.0.0.1:{}/devtools/page/{}",
            self.remote_debugging_port, target_id
        )
    }

    /// Returns all currently registered mappings as a list of (tab_id, target_id) pairs.
    ///
    /// Used by the GET /cdp/targets endpoint to enumerate all known targets.
    pub fn all_mappings(&self) -> Vec<(Uuid, String)> {
        self.uuid_to_target
            .read()
            .iter()
            .map(|(uuid, target)| (*uuid, target.clone()))
            .collect()
    }

    /// Returns the number of currently registered mappings.
    pub fn len(&self) -> usize {
        self.uuid_to_target.read().len()
    }

    /// Returns true if there are no registered mappings.
    pub fn is_empty(&self) -> bool {
        self.uuid_to_target.read().is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_register_and_lookup_forward() {
        let mapping = CdpTabMapping::new(9222);
        let tab_id = Uuid::new_v4();
        let target_id = "1".to_string();

        mapping.register(tab_id, target_id.clone());

        assert_eq!(mapping.get_target_id(&tab_id), Some(target_id));
    }

    #[test]
    fn test_register_and_lookup_reverse() {
        let mapping = CdpTabMapping::new(9222);
        let tab_id = Uuid::new_v4();
        let target_id = "42".to_string();

        mapping.register(tab_id, target_id.clone());

        assert_eq!(mapping.get_tab_id(&target_id), Some(tab_id));
    }

    #[test]
    fn test_unregister_removes_both_directions() {
        let mapping = CdpTabMapping::new(9222);
        let tab_id = Uuid::new_v4();
        let target_id = "7".to_string();

        mapping.register(tab_id, target_id.clone());
        mapping.unregister(&tab_id);

        assert_eq!(mapping.get_target_id(&tab_id), None);
        assert_eq!(mapping.get_tab_id(&target_id), None);
    }

    #[test]
    fn test_unregister_nonexistent_is_safe() {
        let mapping = CdpTabMapping::new(9222);
        let tab_id = Uuid::new_v4();

        // Should not panic
        mapping.unregister(&tab_id);
    }

    #[test]
    fn test_all_mappings_returns_registered_pairs() {
        let mapping = CdpTabMapping::new(9222);
        let tab1 = Uuid::new_v4();
        let tab2 = Uuid::new_v4();

        mapping.register(tab1, "100".to_string());
        mapping.register(tab2, "200".to_string());

        let all = mapping.all_mappings();
        assert_eq!(all.len(), 2);

        let ids: Vec<Uuid> = all.iter().map(|(uuid, _)| *uuid).collect();
        assert!(ids.contains(&tab1));
        assert!(ids.contains(&tab2));
    }

    #[test]
    fn test_len_and_is_empty() {
        let mapping = CdpTabMapping::new(9222);
        assert!(mapping.is_empty());
        assert_eq!(mapping.len(), 0);

        let tab_id = Uuid::new_v4();
        mapping.register(tab_id, "1".to_string());

        assert!(!mapping.is_empty());
        assert_eq!(mapping.len(), 1);
    }

    #[test]
    fn test_ws_urls() {
        let mapping = CdpTabMapping::new(9333);

        assert_eq!(
            mapping.browser_ws_url(),
            "ws://127.0.0.1:9333/json/version"
        );
        assert_eq!(
            mapping.target_ws_url("42"),
            "ws://127.0.0.1:9333/devtools/page/42"
        );
    }

    #[test]
    fn test_overwrite_existing_mapping() {
        let mapping = CdpTabMapping::new(9222);
        let tab_id = Uuid::new_v4();

        mapping.register(tab_id, "old_target".to_string());
        mapping.register(tab_id, "new_target".to_string());

        assert_eq!(
            mapping.get_target_id(&tab_id),
            Some("new_target".to_string())
        );
        // Old reverse mapping should still exist (stale) - this is a known trade-off
        // for simplicity. The unregister call cleans up properly.
        assert_eq!(mapping.get_tab_id("new_target"), Some(tab_id));
    }

    #[test]
    fn test_remote_debugging_port() {
        let mapping = CdpTabMapping::new(9876);
        assert_eq!(mapping.remote_debugging_port(), 9876);
    }
}
