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

/// Availability information for an OCR engine on this system.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OcrEngineInfo {
    /// Engine identifier (e.g. "tesseract", "paddleocr", "surya").
    pub name: String,
    /// Whether the engine's runtime dependencies are available.
    pub available: bool,
    /// Engine or binding version string, if available.
    pub version: Option<String>,
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

/// Returns availability information for every registered OCR engine.
pub fn engine_info() -> Vec<OcrEngineInfo> {
    all_engines()
        .iter()
        .map(|e| OcrEngineInfo {
            name: e.name().to_string(),
            available: e.is_available(),
            version: e.version(),
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

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
        };
        let json = serde_json::to_string(&info).unwrap();
        assert!(json.contains("tesseract"));
        assert!(json.contains("true"));
    }
}
