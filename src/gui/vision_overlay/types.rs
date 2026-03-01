//! Types for the vision overlay system: element detection, hit testing, and overlay rendering state.
//!
//! Defines the core data structures used across all vision overlay sub-modules:
//! the active tactic selector (`VisionMode`), per-element bounding box data
//! (`OverlayElement` with optional OCR text), async loading state (`OverlayState`),
//! and the persistent session state (`VisionOverlayState`).

use std::sync::{Arc, Mutex};
use egui::Color32;

/// Which vision tactic is currently active as an overlay.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum VisionMode {
    Off,
    /// Numbered labels for all interactive elements (red boxes + ID badges).
    VisionLabels,
    /// Color-coded element types (links=blue, buttons=green, inputs=orange).
    DomAnnotate,
    /// DOM snapshot tree with bounding boxes for all visible nodes.
    DomSnapshot,
    /// Detected forms highlighted with field indicators.
    Forms,
}

impl VisionMode {
    /// Short display label for the UI toggle buttons.
    pub fn label(&self) -> &'static str {
        match self {
            Self::Off => "Aus",
            Self::VisionLabels => "Vision Labels",
            Self::DomAnnotate => "DOM Annotate",
            Self::DomSnapshot => "DOM Snapshot",
            Self::Forms => "Formulare",
        }
    }

    /// Descriptive tooltip for the UI.
    pub fn description(&self) -> &'static str {
        match self {
            Self::Off => "Kein Overlay aktiv",
            Self::VisionLabels => "Nummerierte Labels fuer alle interaktiven Elemente",
            Self::DomAnnotate => "Farbige Markierungen nach Element-Typ",
            Self::DomSnapshot => "Bounding Boxes fuer alle sichtbaren DOM-Knoten",
            Self::Forms => "Erkannte Formulare und Felder hervorgehoben",
        }
    }

    /// All non-Off modes; used to render the mode selector in the GUI toolbar.
    pub fn all_active() -> &'static [VisionMode] {
        &[
            Self::VisionLabels,
            Self::DomAnnotate,
            Self::DomSnapshot,
            Self::Forms,
        ]
    }
}

/// A single overlay element to draw on the viewport.
///
/// Coordinates (`x`, `y`, `w`, `h`) are in webpage pixels as returned by the API.
/// The renderer scales them to screen pixels using the current viewport scale factor.
#[derive(Clone, Debug)]
pub struct OverlayElement {
    pub id: u32,
    pub x: f32,
    pub y: f32,
    pub w: f32,
    pub h: f32,
    pub label: String,
    pub color: Color32,
    pub element_type: String,
    /// OCR text recognized by Tesseract engine (populated by background enrichment).
    pub ocr_tesseract: Option<String>,
    /// OCR text recognized by PaddleOCR engine (populated by background enrichment).
    pub ocr_paddleocr: Option<String>,
    /// OCR text recognized by Surya engine (populated by background enrichment).
    pub ocr_surya: Option<String>,
}

/// State for async overlay data loading from the browser API.
pub enum OverlayState {
    Empty,
    Loading,
    Loaded(Vec<OverlayElement>),
    Error(String),
}

/// Shared overlay state for background-thread loading and OCR enrichment.
pub type SharedOverlay = Arc<Mutex<OverlayState>>;

/// Persistent session state for vision overlays.
///
/// Holds the active mode, the shared overlay data handle, and the viewport
/// dimensions at the time the overlay was fetched (needed for coordinate scaling).
pub struct VisionOverlayState {
    pub mode: VisionMode,
    pub overlay: SharedOverlay,
    /// Viewport dimensions used when the overlay was fetched (for coordinate scaling).
    pub source_viewport: (f32, f32),
}

impl Default for VisionOverlayState {
    fn default() -> Self {
        Self {
            mode: VisionMode::Off,
            overlay: Arc::new(Mutex::new(OverlayState::Empty)),
            source_viewport: (1280.0, 720.0),
        }
    }
}

impl VisionOverlayState {
    /// Returns a cloned `Arc` handle to the shared overlay state for use in background threads.
    pub fn overlay_handle(&self) -> SharedOverlay {
        self.overlay.clone()
    }

    /// Transitions the overlay to the `Loading` state, indicating a pending API request.
    pub fn set_loading(&self) {
        if let Ok(mut s) = self.overlay.lock() {
            *s = OverlayState::Loading;
        }
    }

    /// Resets the overlay to `Empty`, clearing any previously loaded elements.
    pub fn clear(&self) {
        if let Ok(mut s) = self.overlay.lock() {
            *s = OverlayState::Empty;
        }
    }
}
