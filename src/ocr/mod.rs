//! OCR engine abstraction with support for Tesseract, PaddleOCR, and Surya.
//!
//! Each engine implements the `OcrEngine` trait. Results include recognized text,
//! confidence scores, and bounding boxes for per-element OCR.
//!
//! Tesseract uses native C bindings via leptess (feature-gated behind `ocr`).
//! PaddleOCR and Surya call their respective Python packages via subprocess,
//! gracefully returning `available: false` when the Python package is missing.

pub mod paddleocr;
pub mod surya;
pub mod tesseract;

use std::collections::HashMap;

use serde::{Deserialize, Serialize};

/// A single OCR recognition result for a detected text region.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OcrResult {
    /// Recognized text content.
    pub text: String,
    /// Engine-reported confidence score (0.0 to 1.0).
    pub confidence: f32,
    /// Bounding box X origin (pixels from left).
    pub x: f32,
    /// Bounding box Y origin (pixels from top).
    pub y: f32,
    /// Bounding box width in pixels.
    pub w: f32,
    /// Bounding box height in pixels.
    pub h: f32,
}

/// Full OCR response returned by an engine after recognition.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OcrResponse {
    /// Name of the engine that produced the result.
    pub engine: String,
    /// Per-region recognition results with bounding boxes.
    pub results: Vec<OcrResult>,
    /// Concatenated full text from all regions.
    pub full_text: String,
    /// Wall-clock duration of the recognition in milliseconds.
    pub duration_ms: u64,
}

/// Availability information for an OCR engine on this system, including
/// self-documenting metadata so the consuming AI can pick the right engine
/// without external docs.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OcrEngineInfo {
    /// Engine identifier (e.g. "tesseract", "paddleocr", "surya").
    pub name: String,
    /// Whether the engine's runtime dependencies are available.
    pub available: bool,
    /// Engine or binding version string, if available.
    pub version: Option<String>,
    /// Human-readable description of what the engine does.
    pub description: String,
    /// Short hint describing the workload this engine is best suited for.
    pub best_for: String,
    /// Relative speed class: "fast", "medium" or "slow".
    pub speed: String,
    /// Whether the engine uses GPU acceleration.
    pub gpu_accelerated: bool,
    /// Languages / language coverage the engine supports.
    pub languages: String,
}

/// Static, self-documenting metadata for a single engine, independent of
/// whether the engine is currently installed/available.
struct EngineMeta {
    description: &'static str,
    best_for: &'static str,
    speed: &'static str,
    gpu_accelerated: bool,
    languages: &'static str,
}

/// Returns the static metadata for a given engine name. Unknown names fall
/// back to a neutral placeholder so callers never panic.
fn engine_meta(name: &str) -> EngineMeta {
    match name {
        "tesseract" => EngineMeta {
            description: "Fast classic OCR engine via native leptess/Tesseract bindings.",
            best_for: "clean printed text",
            speed: "fast",
            gpu_accelerated: false,
            languages: "eng+deu",
        },
        "paddleocr" => EngineMeta {
            description: "PaddleOCR Python engine for dense, multilingual text and tables.",
            best_for: "dense or multilingual text, tables",
            speed: "medium",
            gpu_accelerated: true,
            languages: "80+ languages",
        },
        "surya" => EngineMeta {
            description: "Surya engine with the strongest layout and multilingual recognition.",
            best_for: "complex layouts, 90+ languages",
            speed: "slow",
            gpu_accelerated: true,
            languages: "90+ languages",
        },
        _ => EngineMeta {
            description: "Unknown OCR engine.",
            best_for: "unknown",
            speed: "medium",
            gpu_accelerated: false,
            languages: "unknown",
        },
    }
}

/// Runtime configuration controlling which OCR engines are enabled, both
/// globally and per browser tab. Stored in `AppState` behind an `RwLock` so
/// engines can be toggled at runtime without a restart.
#[derive(Debug, Clone, Default)]
pub struct OcrRuntimeConfig {
    /// Global enable/disable state per engine name (`engine -> enabled`).
    pub global: HashMap<String, bool>,
    /// Per-tab overrides: `tab_id -> (engine -> enabled)`.
    pub per_tab: HashMap<String, HashMap<String, bool>>,
}

impl OcrRuntimeConfig {
    /// Creates a config with every registered engine globally enabled.
    pub fn with_all_enabled() -> Self {
        let global = engine_info()
            .into_iter()
            .map(|info| (info.name, true))
            .collect();
        Self {
            global,
            per_tab: HashMap::new(),
        }
    }
}

/// Trait implemented by each OCR engine backend.
pub trait OcrEngine: Send + Sync {
    /// Returns the engine identifier (e.g. "tesseract").
    fn name(&self) -> &str;
    /// Checks whether the engine's runtime dependencies are present.
    fn is_available(&self) -> bool;
    /// Returns the engine/binding version, if available.
    fn version(&self) -> Option<String>;
    /// Runs OCR on PNG image data, optionally restricted to a region of interest.
    fn recognize(&self, png_data: &[u8], region: Option<OcrRegion>) -> Result<OcrResponse, String>;
}

/// Optional rectangular region of interest for cropping before OCR.
#[derive(Debug, Clone)]
pub struct OcrRegion {
    /// X origin (pixels from left edge).
    pub x: u32,
    /// Y origin (pixels from top edge).
    pub y: u32,
    /// Width of the region in pixels.
    pub w: u32,
    /// Height of the region in pixels.
    pub h: u32,
}

/// Crops a PNG image to the specified region. Used by all engines before recognition.
pub fn crop_png(png_data: &[u8], region: &OcrRegion) -> Result<Vec<u8>, String> {
    use image::ImageFormat;
    let img =
        image::load_from_memory(png_data).map_err(|e| format!("Image decode failed: {}", e))?;
    let cropped = img.crop_imm(region.x, region.y, region.w, region.h);
    let mut buf = Vec::new();
    cropped
        .write_to(&mut std::io::Cursor::new(&mut buf), ImageFormat::Png)
        .map_err(|e| format!("Image encode failed: {}", e))?;
    Ok(buf)
}

/// Returns instances of all registered OCR engine backends.
pub fn all_engines() -> Vec<Box<dyn OcrEngine>> {
    vec![
        Box::new(tesseract::TesseractEngine::new()),
        Box::new(paddleocr::PaddleOcrEngine::new()),
        Box::new(surya::SuryaEngine::new()),
    ]
}

/// Returns availability information (including self-documenting metadata) for
/// every registered OCR engine.
pub fn engine_info() -> Vec<OcrEngineInfo> {
    all_engines()
        .iter()
        .map(|e| {
            let name = e.name().to_string();
            let meta = engine_meta(&name);
            OcrEngineInfo {
                available: e.is_available(),
                version: e.version(),
                description: meta.description.to_string(),
                best_for: meta.best_for.to_string(),
                speed: meta.speed.to_string(),
                gpu_accelerated: meta.gpu_accelerated,
                languages: meta.languages.to_string(),
                name,
            }
        })
        .collect()
}

/// Alias for [`engine_info`], exposing the full self-documenting engine catalog.
pub fn engine_catalog() -> Vec<OcrEngineInfo> {
    engine_info()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_ocr_runtime_config_with_all_enabled() {
        let cfg = OcrRuntimeConfig::with_all_enabled();
        assert_eq!(cfg.global.len(), 3);
        assert_eq!(cfg.global.get("tesseract"), Some(&true));
        assert_eq!(cfg.global.get("paddleocr"), Some(&true));
        assert_eq!(cfg.global.get("surya"), Some(&true));
        assert!(cfg.per_tab.is_empty());
    }

    #[test]
    fn test_engine_info_returns_all_three() {
        let info = engine_info();
        assert_eq!(info.len(), 3);
        assert_eq!(info[0].name, "tesseract");
        assert_eq!(info[1].name, "paddleocr");
        assert_eq!(info[2].name, "surya");
    }

    #[test]
    fn test_all_engines_returns_three() {
        let engines = all_engines();
        assert_eq!(engines.len(), 3);
    }

    #[test]
    fn test_ocr_region_clone() {
        let r = OcrRegion {
            x: 10,
            y: 20,
            w: 100,
            h: 50,
        };
        let r2 = r.clone();
        assert_eq!(r2.x, 10);
        assert_eq!(r2.y, 20);
        assert_eq!(r2.w, 100);
        assert_eq!(r2.h, 50);
    }

    #[test]
    fn test_ocr_result_serialization() {
        let result = OcrResult {
            text: "hello".into(),
            confidence: 0.95,
            x: 0.0,
            y: 0.0,
            w: 100.0,
            h: 20.0,
        };
        let json = serde_json::to_string(&result).unwrap();
        assert!(json.contains("hello"));
        assert!(json.contains("0.95"));
    }

    #[test]
    fn test_ocr_response_serialization() {
        let response = OcrResponse {
            engine: "test".into(),
            results: vec![],
            full_text: "sample text".into(),
            duration_ms: 42,
        };
        let json = serde_json::to_string(&response).unwrap();
        let deserialized: OcrResponse = serde_json::from_str(&json).unwrap();
        assert_eq!(deserialized.engine, "test");
        assert_eq!(deserialized.full_text, "sample text");
        assert_eq!(deserialized.duration_ms, 42);
    }

    #[test]
    fn test_ocr_engine_info_serialization() {
        let info = OcrEngineInfo {
            name: "tesseract".into(),
            available: true,
            version: Some("5.3".into()),
            description: "desc".into(),
            best_for: "clean printed text".into(),
            speed: "fast".into(),
            gpu_accelerated: false,
            languages: "eng+deu".into(),
        };
        let json = serde_json::to_string(&info).unwrap();
        assert!(json.contains("tesseract"));
        assert!(json.contains("true"));
    }

    #[test]
    fn test_engine_info_includes_metadata() {
        let info = engine_info();
        let tess = info.iter().find(|e| e.name == "tesseract").unwrap();
        assert_eq!(tess.speed, "fast");
        assert!(!tess.gpu_accelerated);
        assert!(tess.languages.contains("eng"));

        let surya = info.iter().find(|e| e.name == "surya").unwrap();
        assert_eq!(surya.speed, "slow");
        assert!(surya.gpu_accelerated);

        let paddle = info.iter().find(|e| e.name == "paddleocr").unwrap();
        assert_eq!(paddle.speed, "medium");
        assert!(paddle.gpu_accelerated);
    }

    #[test]
    fn test_engine_catalog_matches_engine_info() {
        assert_eq!(engine_catalog().len(), engine_info().len());
    }
}
