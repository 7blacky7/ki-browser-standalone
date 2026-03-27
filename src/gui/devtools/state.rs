//! Arc-wrapped mutable state for the DevTools deferred OS viewport,
//! shared between the main GUI thread and background analysis threads.
//!
//! All fields use `Arc<AtomicBool>` or `Arc<Mutex<T>>` because
//! `show_viewport_deferred` requires its closure to be `Send + Sync + 'static`.

use std::sync::atomic::AtomicBool;
use std::sync::{Arc, Mutex};

use super::types::*;

// ---------------------------------------------------------------------------
// DevToolsState — all fields Arc-wrapped for cross-thread sharing
// ---------------------------------------------------------------------------

/// Persistent state for the DevTools window.
///
/// Every field is wrapped in `Arc<AtomicBool>` or `Arc<Mutex<T>>` because
/// `show_viewport_deferred` requires its closure to be `Send + Sync + 'static`.
pub struct DevToolsState {
    /// Whether the DevTools OS window should be shown.
    pub open: Arc<AtomicBool>,
    /// Active section tab inside DevTools.
    pub section: Arc<Mutex<Section>>,
    /// HTML source code state (loaded asynchronously via REST API).
    pub source: SharedText,
    /// Currently selected KI vision tactic.
    pub vision_tactic: Arc<Mutex<VisionTactic>>,
    /// Vision text/JSON result (loaded asynchronously via REST API).
    pub vision_text: SharedText,
    /// Vision image result (loaded asynchronously via REST API).
    pub vision_image: SharedImage,
    /// Cached egui texture handle for vision annotated screenshot (avoids re-decode each frame).
    pub vision_texture: Arc<Mutex<Option<egui::TextureHandle>>>,
    /// Queued actions to be drained by the main app each frame.
    pub actions: Arc<Mutex<Vec<DevToolsAction>>>,
    /// OCR engine configuration (which engines are enabled).
    pub ocr_config: Arc<Mutex<OcrConfig>>,
    /// OCR results per engine (populated asynchronously after OCR run).
    pub ocr_results: Arc<Mutex<Vec<OcrDisplayResult>>>,
    /// OCR annotated screenshot with bounding boxes drawn on it.
    pub ocr_image: SharedImage,
    /// Cached egui texture handle for the OCR annotated screenshot.
    pub ocr_texture: Arc<Mutex<Option<egui::TextureHandle>>>,
}

impl Default for DevToolsState {
    fn default() -> Self {
        Self {
            open: Arc::new(AtomicBool::new(false)),
            section: Arc::new(Mutex::new(Section::PageInfo)),
            source: Arc::new(Mutex::new(TextState::Empty)),
            vision_tactic: Arc::new(Mutex::new(VisionTactic::Annotated)),
            vision_text: Arc::new(Mutex::new(TextState::Empty)),
            vision_image: Arc::new(Mutex::new(ImageState::Empty)),
            vision_texture: Arc::new(Mutex::new(None)),
            actions: Arc::new(Mutex::new(Vec::new())),
            ocr_config: Arc::new(Mutex::new(OcrConfig::default())),
            ocr_results: Arc::new(Mutex::new(Vec::new())),
            ocr_image: Arc::new(Mutex::new(ImageState::Empty)),
            ocr_texture: Arc::new(Mutex::new(None)),
        }
    }
}

impl DevToolsState {
    /// Sets the source text to `Loaded` state after a successful fetch.
    pub fn set_source(&self, source: String) {
        if let Ok(mut s) = self.source.lock() {
            *s = TextState::Loaded(source);
        }
    }

    /// Sets the source text to `Loading` state while a fetch is in progress.
    pub fn set_source_loading(&self) {
        if let Ok(mut s) = self.source.lock() {
            *s = TextState::Loading;
        }
    }

    /// Sets the source text to `Error` state when a fetch fails.
    pub fn set_source_error(&self, err: String) {
        if let Ok(mut s) = self.source.lock() {
            *s = TextState::Error(err);
        }
    }

    /// Returns a clone of the shared source state handle for background threads.
    pub fn source_handle(&self) -> SharedText {
        self.source.clone()
    }

    /// Returns the vision text handle for background analysis threads.
    pub fn vision_text_handle(&self) -> SharedText {
        self.vision_text.clone()
    }

    /// Returns the vision image handle for background analysis threads.
    pub fn vision_image_handle(&self) -> SharedImage {
        self.vision_image.clone()
    }

    /// Returns the REST API tactic name string for the currently selected `VisionTactic`.
    pub fn current_vision_tactic_name(&self) -> &'static str {
        let tactic = self.vision_tactic.lock()
            .map(|t| *t)
            .unwrap_or(VisionTactic::Annotated);
        match tactic {
            VisionTactic::Annotated => "annotated",
            VisionTactic::Labels => "labels",
            VisionTactic::DomSnapshot => "dom_snapshot",
            VisionTactic::DomAnnotate => "dom_annotate",
            VisionTactic::StructuredData => "structured_data",
            VisionTactic::ContentExtract => "content_extract",
            VisionTactic::StructureAnalysis => "structure_analysis",
            VisionTactic::Forms => "forms",
            VisionTactic::Ocr => "ocr",
        }
    }

    /// Returns `true` if the current tactic produces an annotated image result
    /// rather than a text/JSON result.
    pub fn current_tactic_is_image(&self) -> bool {
        let tactic = self.vision_tactic.lock()
            .map(|t| *t)
            .unwrap_or(VisionTactic::Annotated);
        matches!(tactic, VisionTactic::Annotated | VisionTactic::DomAnnotate)
    }
}

// ---------------------------------------------------------------------------
// DevToolsShared — bundles state + page info + tabs for the deferred viewport
// ---------------------------------------------------------------------------

/// Shared container passed into the deferred viewport closure.
///
/// Groups the DevTools UI state together with page/tab info that the main app
/// updates each frame before the deferred viewport renders.
pub struct DevToolsShared {
    pub state: DevToolsState,
    /// Current page info, updated by the main app every frame before render.
    pub page_info: Arc<Mutex<PageInfo>>,
    /// Current tab list, updated by the main app every frame before render.
    pub tabs: Arc<Mutex<Vec<DevToolsTabInfo>>>,
}

impl Default for DevToolsShared {
    fn default() -> Self {
        Self {
            state: DevToolsState::default(),
            page_info: Arc::new(Mutex::new(PageInfo::default())),
            tabs: Arc::new(Mutex::new(Vec::new())),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::Ordering;
    use uuid::Uuid;

    #[test]
    fn test_devtools_state_default_is_closed() {
        let state = DevToolsState::default();
        assert!(!state.open.load(Ordering::Relaxed));
    }

    #[test]
    fn test_devtools_state_open_toggle() {
        let state = DevToolsState::default();
        state.open.store(true, Ordering::Relaxed);
        assert!(state.open.load(Ordering::Relaxed));
        state.open.store(false, Ordering::Relaxed);
        assert!(!state.open.load(Ordering::Relaxed));
    }

    #[test]
    fn test_devtools_shared_default() {
        let shared = DevToolsShared::default();
        assert!(!shared.state.open.load(Ordering::Relaxed));
        let pi = shared.page_info.lock().unwrap();
        assert!(pi.title.is_empty());
        let tabs = shared.tabs.lock().unwrap();
        assert!(tabs.is_empty());
    }

    #[test]
    fn test_set_source_loading_and_error() {
        let state = DevToolsState::default();
        state.set_source_loading();
        {
            let guard = state.source.lock().unwrap();
            assert!(matches!(*guard, TextState::Loading));
        }
        state.set_source_error("test error".to_string());
        {
            let guard = state.source.lock().unwrap();
            assert!(matches!(*guard, TextState::Error(ref e) if e == "test error"));
        }
    }

    #[test]
    fn test_set_source_loaded() {
        let state = DevToolsState::default();
        state.set_source("<html></html>".to_string());
        let guard = state.source.lock().unwrap();
        assert!(matches!(*guard, TextState::Loaded(ref s) if s == "<html></html>"));
    }

    #[test]
    fn test_current_vision_tactic_name_default() {
        let state = DevToolsState::default();
        assert_eq!(state.current_vision_tactic_name(), "annotated");
    }

    #[test]
    fn test_current_tactic_is_image_default() {
        let state = DevToolsState::default();
        assert!(state.current_tactic_is_image());
    }

    #[test]
    fn test_current_tactic_is_image_text_tactic() {
        let state = DevToolsState::default();
        *state.vision_tactic.lock().unwrap() = VisionTactic::Labels;
        assert!(!state.current_tactic_is_image());
    }

    #[test]
    fn test_actions_queue_drain() {
        let state = DevToolsState::default();
        {
            let mut actions = state.actions.lock().unwrap();
            actions.push(DevToolsAction::LoadSource(Uuid::nil()));
            actions.push(DevToolsAction::SwitchToTab(0));
        }
        let drained: Vec<DevToolsAction> = state.actions.lock().unwrap().drain(..).collect();
        assert_eq!(drained.len(), 2);
        assert!(state.actions.lock().unwrap().is_empty());
    }
}
