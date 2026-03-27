//! Type definitions for the DevTools window: sections, vision tactics,
//! shared data containers, OCR result types, and the action queue enum.
//!
//! All types here are `pub` so they can be re-exported through the module root
//! and consumed by the main application and renderer sub-modules.

use std::sync::{Arc, Mutex};

use egui::Color32;
use uuid::Uuid;

// ---------------------------------------------------------------------------
// Section / VisionTactic enums
// ---------------------------------------------------------------------------

/// Which section is active in the DevTools window.
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Section {
    PageInfo,
    Source,
    KiVision,
    Tabs,
}

/// Which KI vision tactic is selected for analysis.
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum VisionTactic {
    Annotated,
    Labels,
    DomSnapshot,
    DomAnnotate,
    StructuredData,
    ContentExtract,
    StructureAnalysis,
    Forms,
    Ocr,
}

impl VisionTactic {
    /// Human-readable label shown in the tactic selector grid.
    pub fn label(&self) -> &'static str {
        match self {
            Self::Annotated => "Vision Annotated",
            Self::Labels => "Vision Labels",
            Self::DomSnapshot => "DOM Snapshot",
            Self::DomAnnotate => "DOM Annotate",
            Self::StructuredData => "Structured Data",
            Self::ContentExtract => "Content Extract",
            Self::StructureAnalysis => "Seitenstruktur",
            Self::Forms => "Formulare",
            Self::Ocr => "OCR",
        }
    }

    /// Short description of what the tactic analyses/returns.
    pub fn description(&self) -> &'static str {
        match self {
            Self::Annotated => "Screenshot mit nummerierten Element-Overlays",
            Self::Labels => "JSON-Liste aller erkannten Elemente mit Rollen",
            Self::DomSnapshot => "Vollstaendiger DOM-Tree mit Bounding Boxes",
            Self::DomAnnotate => "Farbig markierte Element-Typen (Links, Buttons, Inputs)",
            Self::StructuredData => "JSON-LD, OpenGraph, Meta-Tags, Microdata",
            Self::ContentExtract => "Hauptinhalt der Seite (Readability)",
            Self::StructureAnalysis => "Seitenstruktur, Sektionen, Seitentyp",
            Self::Forms => "Erkannte Formulare mit Feldern",
            Self::Ocr => "Text-Erkennung via Tesseract, PaddleOCR, Surya",
        }
    }

    /// Accent colour for this tactic used in the selector grid and header.
    pub fn color(&self) -> Color32 {
        match self {
            Self::Annotated => Color32::from_rgb(255, 100, 100),
            Self::Labels => Color32::from_rgb(255, 150, 80),
            Self::DomSnapshot => Color32::from_rgb(100, 200, 255),
            Self::DomAnnotate => Color32::from_rgb(100, 255, 100),
            Self::StructuredData => Color32::from_rgb(200, 150, 255),
            Self::ContentExtract => Color32::from_rgb(255, 220, 100),
            Self::StructureAnalysis => Color32::from_rgb(100, 220, 200),
            Self::Forms => Color32::from_rgb(255, 180, 200),
            Self::Ocr => Color32::from_rgb(255, 255, 100),
        }
    }

    /// Returns all available tactics in display order.
    pub fn all() -> &'static [VisionTactic] {
        &[
            Self::Annotated,
            Self::Labels,
            Self::DomSnapshot,
            Self::DomAnnotate,
            Self::StructuredData,
            Self::ContentExtract,
            Self::StructureAnalysis,
            Self::Forms,
            Self::Ocr,
        ]
    }
}

// ---------------------------------------------------------------------------
// Shared data containers
// ---------------------------------------------------------------------------

/// Info about a single browser tab for the Tabs section display.
#[derive(Clone)]
pub struct DevToolsTabInfo {
    pub id: Uuid,
    pub title: String,
    pub url: String,
    pub is_loading: bool,
    pub is_active: bool,
}

/// Info about the currently active page for the PageInfo section display.
#[derive(Clone, Default)]
pub struct PageInfo {
    pub title: String,
    pub url: String,
    pub is_loading: bool,
    pub can_go_back: bool,
    pub can_go_forward: bool,
    pub api_port: u16,
    pub tab_count: usize,
}

/// Shared container for async text fetching (source code, vision results).
pub type SharedText = Arc<Mutex<TextState>>;

/// State for async-loaded text content fetched from REST API endpoints.
pub enum TextState {
    Empty,
    Loading,
    Loaded(String),
    Error(String),
}

/// Shared container for async image fetching (annotated screenshots, OCR overlays).
pub type SharedImage = Arc<Mutex<ImageState>>;

/// State for async-loaded PNG image data fetched from the vision API.
pub enum ImageState {
    Empty,
    Loading,
    Loaded(Vec<u8>),
    Error(String),
}

/// OCR engine selection for the DevTools (which engines to run concurrently).
#[derive(Clone)]
pub struct OcrConfig {
    pub tesseract: bool,
    pub paddleocr: bool,
    pub surya: bool,
}

impl Default for OcrConfig {
    fn default() -> Self {
        Self { tesseract: true, paddleocr: true, surya: true }
    }
}

/// A single OCR text region with bounding box coordinates for overlay rendering.
#[derive(Clone)]
pub struct OcrDisplayRegion {
    pub text: String,
    pub confidence: f32,
    pub x: f32,
    pub y: f32,
    pub w: f32,
    pub h: f32,
}

/// A single OCR engine result for display in DevTools, including per-region data.
#[derive(Clone)]
pub struct OcrDisplayResult {
    pub engine: String,
    pub full_text: String,
    pub result_count: usize,
    pub duration_ms: u64,
    pub error: Option<String>,
    /// Per-region bounding boxes for overlay rendering in the annotated screenshot.
    pub regions: Vec<OcrDisplayRegion>,
}

/// Action requested by the DevTools window, queued for the main app to handle each frame.
pub enum DevToolsAction {
    /// Request to load the page source code for the active tab.
    LoadSource(Uuid),
    /// Switch to a specific tab by index.
    SwitchToTab(usize),
    /// Close a tab by index.
    CloseTab(usize),
    /// Run a KI vision tactic via REST API for the given tab.
    RunVisionTactic {
        tactic: &'static str,
        tab_id: Uuid,
    },
    /// Run OCR with the selected engines for the given tab.
    RunOcr {
        engines: Vec<String>,
        tab_id: Uuid,
    },
}
