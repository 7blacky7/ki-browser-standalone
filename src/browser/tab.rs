//! Tab management module for browser automation.
//!
//! This module provides structures and utilities for managing browser tabs,
//! including tab state tracking and thread-safe tab collections.
//!
//! # Example
//!
//! ```rust,ignore
//! use ki_browser::browser::tab::{TabManager, TabStatus};
//!
//! let manager = TabManager::new();
//! let tab = manager.new_tab("https://example.com".to_string());
//! println!("Tab ID: {}, Status: {:?}", tab.id, tab.status);
//! ```

use chrono::{DateTime, Utc};
use parking_lot::RwLock;
use std::collections::HashMap;
use uuid::Uuid;

/// Represents the current status of a browser tab.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TabStatus {
    /// Tab is currently loading content.
    Loading,
    /// Tab has finished loading and is ready for interaction.
    Ready,
    /// Tab encountered an error during loading or operation.
    Error(String),
    /// Tab has been closed.
    Closed,
}

impl Default for TabStatus {
    fn default() -> Self {
        Self::Loading
    }
}

impl std::fmt::Display for TabStatus {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            TabStatus::Loading => write!(f, "Loading"),
            TabStatus::Ready => write!(f, "Ready"),
            TabStatus::Error(msg) => write!(f, "Error: {}", msg),
            TabStatus::Closed => write!(f, "Closed"),
        }
    }
}

/// Represents a browser tab with its associated metadata.
#[derive(Debug, Clone)]
pub struct Tab {
    /// Unique identifier for the tab.
    pub id: Uuid,

    /// Current URL of the tab.
    pub url: String,

    /// Page title (may be empty while loading).
    pub title: String,

    /// Timestamp when the tab was created.
    pub created_at: DateTime<Utc>,

    /// Current status of the tab.
    pub status: TabStatus,

    /// Timestamp of the last navigation or status change.
    pub last_updated: DateTime<Utc>,

    /// Whether this tab is the currently active (focused) tab.
    pub is_active: bool,

    /// Optional error message if the tab encountered an error.
    pub error_message: Option<String>,
}

impl Tab {
    /// Creates a new tab with the specified URL.
    ///
    /// The tab starts in the `Loading` status with an empty title.
    ///
    /// # Arguments
    ///
    /// * `url` - The initial URL for the tab
    pub fn new(url: String) -> Self {
        let now = Utc::now();
        Self {
            id: Uuid::new_v4(),
            url,
            title: String::new(),
            created_at: now,
            status: TabStatus::Loading,
            last_updated: now,
            is_active: false,
            error_message: None,
        }
    }

    /// Creates a new tab with a specific ID (useful for testing).
    pub fn with_id(id: Uuid, url: String) -> Self {
        let now = Utc::now();
        Self {
            id,
            url,
            title: String::new(),
            created_at: now,
            status: TabStatus::Loading,
            last_updated: now,
            is_active: false,
            error_message: None,
        }
    }

    /// Updates the tab's URL and sets status to Loading.
    pub fn navigate(&mut self, url: String) {
        self.url = url;
        self.status = TabStatus::Loading;
        self.last_updated = Utc::now();
        self.error_message = None;
    }

    /// Sets the tab status to Ready.
    pub fn set_ready(&mut self) {
        self.status = TabStatus::Ready;
        self.last_updated = Utc::now();
    }

    /// Sets the tab status to Error with the given message.
    pub fn set_error(&mut self, message: String) {
        self.error_message = Some(message.clone());
        self.status = TabStatus::Error(message);
        self.last_updated = Utc::now();
    }

    /// Sets the tab status to Closed.
    pub fn set_closed(&mut self) {
        self.status = TabStatus::Closed;
        self.last_updated = Utc::now();
    }

    /// Returns true if the tab is ready for interaction.
    pub fn is_ready(&self) -> bool {
        matches!(self.status, TabStatus::Ready)
    }

    /// Returns true if the tab has encountered an error.
    pub fn has_error(&self) -> bool {
        matches!(self.status, TabStatus::Error(_))
    }

    /// Returns true if the tab is closed.
    pub fn is_closed(&self) -> bool {
        matches!(self.status, TabStatus::Closed)
    }

    /// Returns the duration since the tab was created.
    pub fn age(&self) -> chrono::Duration {
        Utc::now() - self.created_at
    }
}

/// Thread-safe manager for browser tabs.
///
/// Provides methods for creating, closing, and managing browser tabs
/// with thread-safe access using `parking_lot::RwLock`.
pub struct TabManager {
    /// Internal storage for tabs.
    tabs: RwLock<HashMap<Uuid, Tab>>,

    /// ID of the currently active tab, if any.
    active_tab: RwLock<Option<Uuid>>,

    /// Maximum number of tabs allowed (0 = unlimited).
    max_tabs: usize,
}

impl Default for TabManager {
    fn default() -> Self {
        Self::new()
    }
}

impl TabManager {
    /// Creates a new TabManager with no tab limit.
    pub fn new() -> Self {
        Self {
            tabs: RwLock::new(HashMap::new()),
            active_tab: RwLock::new(None),
            max_tabs: 0,
        }
    }

    /// Creates a new TabManager with a maximum tab limit.
    ///
    /// # Arguments
    ///
    /// * `max_tabs` - Maximum number of tabs allowed (0 = unlimited)
    pub fn with_max_tabs(max_tabs: usize) -> Self {
        Self {
            tabs: RwLock::new(HashMap::new()),
            active_tab: RwLock::new(None),
            max_tabs,
        }
    }

    /// Creates a new tab with the specified URL.
    ///
    /// # Arguments
    ///
    /// * `url` - The initial URL for the tab
    ///
    /// # Returns
    ///
    /// The created Tab, or an error if the tab limit is reached.
    pub fn new_tab(&self, url: String) -> Result<Tab, TabManagerError> {
        let mut tabs = self.tabs.write();

        if self.max_tabs > 0 && tabs.len() >= self.max_tabs {
            return Err(TabManagerError::MaxTabsReached(self.max_tabs));
        }

        let tab = Tab::new(url);
        let tab_id = tab.id;
        tabs.insert(tab_id, tab.clone());

        // If this is the first tab, make it active
        let mut active = self.active_tab.write();
        if active.is_none() {
            *active = Some(tab_id);
        }

        Ok(tab)
    }

    /// Closes a tab by its ID.
    ///
    /// # Arguments
    ///
    /// * `tab_id` - The UUID of the tab to close
    ///
    /// # Returns
    ///
    /// The closed Tab, or an error if not found.
    pub fn close_tab(&self, tab_id: Uuid) -> Result<Tab, TabManagerError> {
        let mut tabs = self.tabs.write();

        let mut tab = tabs
            .remove(&tab_id)
            .ok_or(TabManagerError::TabNotFound(tab_id))?;

        tab.set_closed();

        // Update active tab if needed
        let mut active = self.active_tab.write();
        if *active == Some(tab_id) {
            // Set active to another tab or None
            *active = tabs.keys().next().copied();
        }

        Ok(tab)
    }

    /// Gets a clone of a tab by its ID.
    ///
    /// # Arguments
    ///
    /// * `tab_id` - The UUID of the tab to retrieve
    pub fn get_tab(&self, tab_id: Uuid) -> Option<Tab> {
        self.tabs.read().get(&tab_id).cloned()
    }

    /// Gets a mutable reference to a tab for modification.
    ///
    /// This method acquires a write lock, so use sparingly.
    pub fn with_tab_mut<F, R>(&self, tab_id: Uuid, f: F) -> Option<R>
    where
        F: FnOnce(&mut Tab) -> R,
    {
        let mut tabs = self.tabs.write();
        tabs.get_mut(&tab_id).map(f)
    }

    /// Returns all tabs as a vector.
    pub fn get_all_tabs(&self) -> Vec<Tab> {
        self.tabs.read().values().cloned().collect()
    }

    /// Returns the number of open tabs.
    pub fn tab_count(&self) -> usize {
        self.tabs.read().len()
    }

    /// Returns the currently active tab, if any.
    pub fn get_active_tab(&self) -> Option<Tab> {
        let active_id = *self.active_tab.read();
        active_id.and_then(|id| self.get_tab(id))
    }

    /// Sets the active tab by its ID.
    ///
    /// # Arguments
    ///
    /// * `tab_id` - The UUID of the tab to make active
    ///
    /// # Returns
    ///
    /// Error if the tab doesn't exist.
    pub fn set_active_tab(&self, tab_id: Uuid) -> Result<(), TabManagerError> {
        let tabs = self.tabs.read();
        if !tabs.contains_key(&tab_id) {
            return Err(TabManagerError::TabNotFound(tab_id));
        }
        drop(tabs);

        let mut active = self.active_tab.write();
        *active = Some(tab_id);

        // Update is_active flag on tabs
        let mut tabs = self.tabs.write();
        for (id, tab) in tabs.iter_mut() {
            tab.is_active = *id == tab_id;
        }

        Ok(())
    }

    /// Clears the active tab selection.
    pub fn clear_active_tab(&self) {
        let mut active = self.active_tab.write();
        *active = None;

        let mut tabs = self.tabs.write();
        for tab in tabs.values_mut() {
            tab.is_active = false;
        }
    }

    /// Returns all tabs with a specific status.
    pub fn get_tabs_by_status(&self, status: &TabStatus) -> Vec<Tab> {
        self.tabs
            .read()
            .values()
            .filter(|t| std::mem::discriminant(&t.status) == std::mem::discriminant(status))
            .cloned()
            .collect()
    }

    /// Closes all tabs.
    pub fn close_all_tabs(&self) {
        let mut tabs = self.tabs.write();
        tabs.clear();

        let mut active = self.active_tab.write();
        *active = None;
    }

    /// Returns tabs sorted by creation time (oldest first).
    pub fn get_tabs_by_age(&self) -> Vec<Tab> {
        let mut tabs: Vec<Tab> = self.tabs.read().values().cloned().collect();
        tabs.sort_by_key(|t| t.created_at);
        tabs
    }
}

/// Errors that can occur during tab management operations.
#[derive(Debug, Clone, thiserror::Error)]
pub enum TabManagerError {
    /// The requested tab was not found.
    #[error("Tab not found: {0}")]
    TabNotFound(Uuid),

    /// The maximum number of tabs has been reached.
    #[error("Maximum number of tabs ({0}) reached")]
    MaxTabsReached(usize),

    /// Generic operation error.
    #[error("Tab operation failed: {0}")]
    OperationFailed(String),
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_tab_creation() {
        let tab = Tab::new("https://example.com".to_string());
        assert_eq!(tab.url, "https://example.com");
        assert!(tab.title.is_empty());
        assert!(matches!(tab.status, TabStatus::Loading));
        assert!(!tab.is_active);
    }

    #[test]
    fn test_tab_status_transitions() {
        let mut tab = Tab::new("https://example.com".to_string());

        assert!(matches!(tab.status, TabStatus::Loading));

        tab.set_ready();
        assert!(tab.is_ready());

        tab.navigate("https://other.com".to_string());
        assert!(matches!(tab.status, TabStatus::Loading));
        assert_eq!(tab.url, "https://other.com");

        tab.set_error("Network error".to_string());
        assert!(tab.has_error());
        assert_eq!(tab.error_message, Some("Network error".to_string()));

        tab.set_closed();
        assert!(tab.is_closed());
    }

    #[test]
    fn test_tab_manager_basic_operations() {
        let manager = TabManager::new();

        let tab1 = manager.new_tab("https://example.com".to_string()).unwrap();
        let tab2 = manager.new_tab("https://other.com".to_string()).unwrap();

        assert_eq!(manager.tab_count(), 2);

        // First tab should be active
        let active = manager.get_active_tab().unwrap();
        assert_eq!(active.id, tab1.id);

        // Change active tab
        manager.set_active_tab(tab2.id).unwrap();
        let active = manager.get_active_tab().unwrap();
        assert_eq!(active.id, tab2.id);

        // Close a tab
        manager.close_tab(tab1.id).unwrap();
        assert_eq!(manager.tab_count(), 1);
        assert!(manager.get_tab(tab1.id).is_none());
    }

    #[test]
    fn test_tab_manager_max_tabs() {
        let manager = TabManager::with_max_tabs(2);

        manager.new_tab("https://1.com".to_string()).unwrap();
        manager.new_tab("https://2.com".to_string()).unwrap();

        let result = manager.new_tab("https://3.com".to_string());
        assert!(matches!(result, Err(TabManagerError::MaxTabsReached(2))));
    }

    #[test]
    fn test_tab_manager_with_tab_mut() {
        let manager = TabManager::new();
        let tab = manager.new_tab("https://example.com".to_string()).unwrap();

        manager.with_tab_mut(tab.id, |t| {
            t.title = "Example".to_string();
            t.set_ready();
        });

        let updated = manager.get_tab(tab.id).unwrap();
        assert_eq!(updated.title, "Example");
        assert!(updated.is_ready());
    }

    #[test]
    fn test_tab_manager_get_tabs_by_status() {
        let manager = TabManager::new();

        let tab1 = manager.new_tab("https://1.com".to_string()).unwrap();
        let tab2 = manager.new_tab("https://2.com".to_string()).unwrap();

        manager.with_tab_mut(tab1.id, |t| t.set_ready());

        let loading_tabs = manager.get_tabs_by_status(&TabStatus::Loading);
        let ready_tabs = manager.get_tabs_by_status(&TabStatus::Ready);

        assert_eq!(loading_tabs.len(), 1);
        assert_eq!(loading_tabs[0].id, tab2.id);
        assert_eq!(ready_tabs.len(), 1);
        assert_eq!(ready_tabs[0].id, tab1.id);
    }

    #[test]
    fn test_tab_status_display() {
        assert_eq!(TabStatus::Loading.to_string(), "Loading");
        assert_eq!(TabStatus::Ready.to_string(), "Ready");
        assert_eq!(
            TabStatus::Error("Test error".to_string()).to_string(),
            "Error: Test error"
        );
        assert_eq!(TabStatus::Closed.to_string(), "Closed");
    }
}
