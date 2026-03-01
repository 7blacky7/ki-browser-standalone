//! Tesseract OCR engine via the leptess crate (C bindings to libtesseract).
//!
//! Requires libtesseract + libleptonica installed on the system.
//! Compiled conditionally when the 'ocr' feature is enabled in Cargo.toml.
//! Without the feature, the engine reports itself as unavailable and returns
//! an error on any recognition attempt.

use super::{OcrEngine, OcrRegion, OcrResponse};
#[cfg(feature = "ocr")]
use super::{crop_png, OcrResult};
#[cfg(feature = "ocr")]
use std::time::Instant;

/// Tesseract OCR engine using leptess C bindings for text recognition.
pub struct TesseractEngine {
    available: bool,
}

impl Default for TesseractEngine {
    fn default() -> Self {
        Self::new()
    }
}

impl TesseractEngine {
    /// Probes whether libtesseract is usable by attempting to create a session.
    pub fn new() -> Self {
        #[cfg(feature = "ocr")]
        {
            let available = leptess::LepTess::new(None, "eng").is_ok();
            Self { available }
        }
        #[cfg(not(feature = "ocr"))]
        {
            Self { available: false }
        }
    }
}

impl OcrEngine for TesseractEngine {
    fn name(&self) -> &str {
        "tesseract"
    }

    fn is_available(&self) -> bool {
        self.available
    }

    fn version(&self) -> Option<String> {
        if self.available {
            Some("leptess 0.14".into())
        } else {
            None
        }
    }

    fn recognize(
        &self,
        png_data: &[u8],
        region: Option<OcrRegion>,
    ) -> Result<OcrResponse, String> {
        #[cfg(feature = "ocr")]
        {
            if !self.available {
                return Err("Tesseract not available (libtesseract init failed)".into());
            }
            let start = Instant::now();
            let data = if let Some(ref r) = region {
                crop_png(png_data, r)?
            } else {
                png_data.to_vec()
            };

            let mut lt = leptess::LepTess::new(None, "eng")
                .map_err(|e| format!("Tesseract init: {}", e))?;
            lt.set_image_from_mem(&data)
                .map_err(|e| format!("Tesseract set_image: {}", e))?;
            let full_text = lt
                .get_utf8_text()
                .map_err(|e| format!("Tesseract get_text: {}", e))?;

            Ok(OcrResponse {
                engine: "tesseract".into(),
                results: vec![OcrResult {
                    text: full_text.trim().to_string(),
                    confidence: 0.0,
                    x: region.as_ref().map(|r| r.x as f32).unwrap_or(0.0),
                    y: region.as_ref().map(|r| r.y as f32).unwrap_or(0.0),
                    w: 0.0,
                    h: 0.0,
                }],
                full_text: full_text.trim().to_string(),
                duration_ms: start.elapsed().as_millis() as u64,
            })
        }
        #[cfg(not(feature = "ocr"))]
        {
            let _ = (png_data, region);
            Err("Tesseract not available (feature 'ocr' not enabled)".into())
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_tesseract_engine_new() {
        let engine = TesseractEngine::new();
        // Without the 'ocr' feature or libtesseract, this should be false
        assert_eq!(engine.name(), "tesseract");
    }

    #[test]
    fn test_tesseract_version_matches_availability() {
        let engine = TesseractEngine::new();
        if engine.is_available() {
            assert!(engine.version().is_some());
        } else {
            assert!(engine.version().is_none());
        }
    }

    #[test]
    fn test_tesseract_recognize_without_feature_returns_error() {
        let engine = TesseractEngine::new();
        if !engine.is_available() {
            let result = engine.recognize(&[], None);
            assert!(result.is_err());
        }
    }
}
