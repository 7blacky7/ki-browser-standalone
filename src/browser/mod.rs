//! Browser module providing core browser automation functionality.
//!
//! This module contains abstractions for browser engine management, tab handling,
//! DOM access, and screenshot capture capabilities.
//!
//! # Submodules
//!
//! - [`engine`] - Browser engine abstraction and configuration
//! - [`tab`] - Tab management and state tracking
//! - [`dom`] - DOM element access and manipulation
//! - [`screenshot`] - Screenshot capture functionality
//! - [`structured_data`] - Structured data extraction (JSON-LD, OpenGraph, microdata)
//! - [`content_extractor`] - Intelligent content extraction and page structure analysis
//! - [`forms`] - Form detection, analysis, and auto-fill
//! - [`cef_input`] - CEF-specific native input simulation (requires `cef-browser` feature)
//! - [`cef_render`] - CEF offscreen rendering (requires `cef-browser` feature)
//! - [`cef_engine`] - CEF browser engine implementation (requires `cef-browser` feature)

pub mod annotate;
pub mod content_extractor;
pub mod dom;
pub mod dom_snapshot;
pub mod engine;
pub mod forms;
pub mod screenshot;
pub mod structured_data;
pub mod tab;
pub mod tab_lock;
pub mod vision;


#[cfg(feature = "cef-browser")]
pub mod cef_input;

#[cfg(feature = "cef-browser")]
pub mod cef_render;

/// CEF browser engine implementation (requires `cef-browser` feature).
#[cfg(feature = "cef-browser")]
pub mod cef_engine;

/// CEF headless runner for windowless/offscreen browser operation (requires `cef-browser` feature).
#[cfg(feature = "cef-browser")]
pub mod cef_headless;

// Re-export commonly used types for convenience
pub use content_extractor::{
    ContentExtractor, ExtractedContent, NavElement, PageSection, PageStructure, PageType,
    SectionRole,
};
pub use dom::{BoundingBox, DomAccessor, DomElement, FrameInfo, MockDomAccessor};
pub use dom_snapshot::{DomNode, DomSnapshot, SnapshotConfig, ViewportInfo};
pub use forms::{
    FieldOption, FieldType, FormButton, FormField, FormFillError, FormFillRequest, FormFillResult,
    FormHandler, FormInfo, FormValidationResult, ValidationError,
};
pub use engine::{BrowserConfig, BrowserEngine, MockBrowserEngine};
pub use screenshot::{ClipRegion, ScreenshotFormat, ScreenshotOptions};
pub use structured_data::{
    AlternateUrl, MetaData, MicrodataItem, OpenGraphData, StructuredDataExtractor,
    StructuredPageData, TwitterCardData,
};
pub use tab::{Tab, TabManager, TabStatus};
pub use tab_lock::TabLockManager;
pub use vision::{VisionLabel, VisionOverlay};


#[cfg(feature = "cef-browser")]
pub use cef_render::{CefRenderHandler, DirtyRect, OffScreenRenderHandler, ScreenInfo};

#[cfg(feature = "cef-browser")]
pub use cef_engine::{CefBrowserEngine, CefBrowserEventSender};

#[cfg(feature = "cef-browser")]
pub use cef_headless::HeadlessRunner;

#[cfg(feature = "cef-browser")]
pub use cef_input::{
    CefEventSender, CefInputConfig, CefInputHandler, CefKeyEvent, CefKeyEventType,
    CefMouseButton, CefMouseEvent,
};
