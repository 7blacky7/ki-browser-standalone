//! Internal CEF tab representation and lifecycle management.
//!
//! Contains the `CefTab` struct which tracks the state of each browser tab
//! including its CEF browser instance, URL, frame buffer for off-screen
//! rendering, and readiness status.

use cef::Browser;
use parking_lot::RwLock;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::Arc;
use uuid::Uuid;

use crate::browser::tab::{Tab, TabStatus};

/// Internal representation of a CEF browser tab.
///
/// Tracks the CEF browser instance, current URL, page title, loading status,
/// and the off-screen rendering frame buffer. The browser field is set
/// asynchronously after the `on_after_created` callback fires.
pub(crate) struct CefTab {
    /// Unique identifier for the tab.
    pub(crate) id: Uuid,
    /// CEF browser instance (set asynchronously after on_after_created).
    pub(crate) browser: Option<Browser>,
    /// CEF browser identifier used as CDP TargetId for remote debugging.
    pub(crate) browser_id: Option<i32>,
    /// Current URL of the tab.
    pub(crate) url: String,
    /// Page title.
    pub(crate) title: String,
    /// Current status.
    pub(crate) status: TabStatus,
    /// Last rendered frame buffer (BGRA format).
    pub(crate) frame_buffer: Arc<RwLock<Vec<u8>>>,
    /// Frame dimensions.
    pub(crate) frame_size: Arc<RwLock<(u32, u32)>>,
    /// Whether the tab is ready for interaction.
    pub(crate) is_ready: AtomicBool,
    /// Whether the browser can navigate back in history.
    pub(crate) can_go_back: AtomicBool,
    /// Whether the browser can navigate forward in history.
    pub(crate) can_go_forward: AtomicBool,
    /// Shared viewport dimensions for the render handler (updated on resize).
    pub(crate) viewport_size: Arc<RwLock<(u32, u32)>>,
    /// Frame version counter, incremented on every on_paint callback.
    /// Used by the video stream encoder to detect new frames.
    pub(crate) frame_version: Arc<AtomicU64>,
}

impl CefTab {
    /// Creates a new CefTab with the given ID, URL, and shared frame buffer references.
    pub(crate) fn new(
        id: Uuid,
        url: String,
        frame_buffer: Arc<RwLock<Vec<u8>>>,
        frame_size: Arc<RwLock<(u32, u32)>>,
        viewport_size: Arc<RwLock<(u32, u32)>>,
        frame_version: Arc<AtomicU64>,
    ) -> Self {
        Self {
            id,
            browser: None,
            browser_id: None,
            url,
            title: String::new(),
            status: TabStatus::Loading,
            frame_buffer,
            frame_size,
            is_ready: AtomicBool::new(false),
            can_go_back: AtomicBool::new(false),
            can_go_forward: AtomicBool::new(false),
            viewport_size,
            frame_version,
        }
    }

    /// Stores the CEF browser reference once the browser has been created.
    pub(crate) fn set_browser(&mut self, browser: Browser) {
        self.browser = Some(browser);
    }

    /// Converts the internal CefTab to the public Tab type for API consumers.
    pub(crate) fn to_tab(&self) -> Tab {
        let mut tab = Tab::new(self.url.clone());
        // Override the auto-generated ID with our tracked ID
        tab.id = self.id;
        tab.title = self.title.clone();
        tab.status = self.status.clone();
        if self.is_ready.load(Ordering::SeqCst) {
            tab.set_ready();
        }
        tab
    }
}
