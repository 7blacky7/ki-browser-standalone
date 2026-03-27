//! DevTools shared-state synchronisation between the main GUI frame and the
//! deferred DevTools OS window.
//!
//! `update_devtools_shared_state` is called once per frame from
//! `KiBrowserApp::update()` and pushes current page metadata and tab list
//! into the `Arc<DevToolsShared>` so the DevTools window always has fresh data
//! without requiring direct access to `KiBrowserApp`.

use std::sync::Arc;

use uuid::Uuid;

use crate::gui::devtools::{DevToolsShared, DevToolsTabInfo, PageInfo};

// ---------------------------------------------------------------------------
// Lightweight tab descriptor (avoids exposing internal GuiTab to this module)
// ---------------------------------------------------------------------------

/// Minimal tab information needed by the DevTools shared-state update.
///
/// Populated from `GuiTab` fields in `browser_app.rs` so that
/// `update_devtools_shared_state` does not need to depend on `GuiTab` directly.
pub(super) struct TabSnapshot {
    pub id: Uuid,
    pub title: String,
    pub url: String,
    pub is_loading: bool,
    pub can_go_back: bool,
    pub can_go_forward: bool,
}

// ---------------------------------------------------------------------------
// update_devtools_shared_state
// ---------------------------------------------------------------------------

/// Sync the `DevToolsShared` Arc with the current GUI state.
///
/// Called once per eframe render frame so the DevTools OS window always shows
/// accurate page title, URL, loading state, history flags, and tab list.
/// All writes go through `Mutex::lock`; poisoned locks are silently skipped.
pub(super) fn update_devtools_shared_state(
    devtools_shared: &Arc<DevToolsShared>,
    active_tab_snapshot: Option<&TabSnapshot>,
    tabs: &[TabSnapshot],
    active_tab_index: usize,
    api_port: u16,
) {
    // Update PageInfo
    if let Ok(mut pi) = devtools_shared.page_info.lock() {
        *pi = PageInfo {
            title: active_tab_snapshot
                .map(|t| t.title.clone())
                .unwrap_or_default(),
            url: active_tab_snapshot
                .map(|t| t.url.clone())
                .unwrap_or_default(),
            is_loading: active_tab_snapshot
                .map(|t| t.is_loading)
                .unwrap_or(false),
            can_go_back: active_tab_snapshot
                .map(|t| t.can_go_back)
                .unwrap_or(false),
            can_go_forward: active_tab_snapshot
                .map(|t| t.can_go_forward)
                .unwrap_or(false),
            api_port,
            tab_count: tabs.len(),
        };
    }

    // Update tab list
    if let Ok(mut tab_list) = devtools_shared.tabs.lock() {
        *tab_list = tabs
            .iter()
            .enumerate()
            .map(|(i, t)| DevToolsTabInfo {
                id: t.id,
                title: t.title.clone(),
                url: t.url.clone(),
                is_loading: t.is_loading,
                is_active: i == active_tab_index,
            })
            .collect();
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::gui::devtools::DevToolsShared;

    fn make_tab(title: &str, url: &str, is_loading: bool) -> TabSnapshot {
        TabSnapshot {
            id: Uuid::new_v4(),
            title: title.to_string(),
            url: url.to_string(),
            is_loading,
            can_go_back: false,
            can_go_forward: false,
        }
    }

    #[test]
    fn test_update_devtools_shared_state_no_tabs() {
        let shared = Arc::new(DevToolsShared::default());
        update_devtools_shared_state(&shared, None, &[], 0, 8080);

        let pi = shared.page_info.lock().unwrap();
        assert!(pi.title.is_empty());
        assert!(pi.url.is_empty());
        assert!(!pi.is_loading);
        assert_eq!(pi.api_port, 8080);
        assert_eq!(pi.tab_count, 0);
    }

    #[test]
    fn test_update_devtools_shared_state_with_active_tab() {
        let shared = Arc::new(DevToolsShared::default());
        let tab = make_tab("Example", "https://example.com", false);
        let tabs = vec![make_tab("Example", "https://example.com", false)];

        update_devtools_shared_state(&shared, Some(&tab), &tabs, 0, 9000);

        let pi = shared.page_info.lock().unwrap();
        assert_eq!(pi.title, "Example");
        assert_eq!(pi.url, "https://example.com");
        assert!(!pi.is_loading);
        assert_eq!(pi.api_port, 9000);
        assert_eq!(pi.tab_count, 1);
    }

    #[test]
    fn test_update_devtools_shared_state_active_tab_marker() {
        let shared = Arc::new(DevToolsShared::default());
        let t1 = make_tab("Tab1", "https://rust-lang.org", false);
        let tabs = vec![
            make_tab("Tab0", "about:blank", false),
            make_tab("Tab1", "https://rust-lang.org", false),
        ];

        // Active tab is index 1
        update_devtools_shared_state(&shared, Some(&t1), &tabs, 1, 8080);

        let tab_list = shared.tabs.lock().unwrap();
        assert_eq!(tab_list.len(), 2);
        assert!(!tab_list[0].is_active);
        assert!(tab_list[1].is_active);
    }

    #[test]
    fn test_update_devtools_shared_state_loading_state() {
        let shared = Arc::new(DevToolsShared::default());
        let tab = make_tab("Loading...", "https://example.com", true);
        let tabs = vec![make_tab("Loading...", "https://example.com", true)];

        update_devtools_shared_state(&shared, Some(&tab), &tabs, 0, 8080);

        let pi = shared.page_info.lock().unwrap();
        assert!(pi.is_loading);
    }
}
