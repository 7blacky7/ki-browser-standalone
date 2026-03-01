//! PaddleOCR engine via Python subprocess invocation.
//!
//! Checks availability by running `python3 -c "import paddleocr"`.
//! Recognition writes the PNG to a temporary file, invokes a Python script
//! that runs PaddleOCR and outputs JSON to stdout, then parses the result.
//! Gracefully returns `available: false` when Python or paddleocr is missing.

use super::{crop_png, OcrEngine, OcrRegion, OcrResponse, OcrResult};
use std::process::Command;
use std::time::Instant;

/// PaddleOCR engine that delegates to the Python paddleocr package via subprocess.
pub struct PaddleOcrEngine {
    available: bool,
    version: Option<String>,
}

impl PaddleOcrEngine {
    /// Probes whether `python3 -c "import paddleocr"` succeeds.
    pub fn new() -> Self {
        let (available, version) = check_paddleocr_available();
        Self { available, version }
    }
}

/// Runs a quick Python import check to determine if paddleocr is installed.
fn check_paddleocr_available() -> (bool, Option<String>) {
    let output = Command::new("python3")
        .args(["-c", "import paddleocr; print(paddleocr.__version__)"])
        .output();
    match output {
        Ok(o) if o.status.success() => {
            let ver = String::from_utf8_lossy(&o.stdout).trim().to_string();
            let version = if ver.is_empty() { None } else { Some(ver) };
            (true, version)
        }
        _ => (false, None),
    }
}

/// Python script that runs PaddleOCR on a given image file and outputs JSON.
const PADDLE_SCRIPT: &str = r#"
import sys, json
from paddleocr import PaddleOCR

ocr = PaddleOCR(use_angle_cls=True, lang='en', show_log=False)
result = ocr.ocr(sys.argv[1], cls=True)

items = []
full_parts = []
if result and result[0]:
    for line in result[0]:
        bbox = line[0]
        text, confidence = line[1]
        x_min = min(p[0] for p in bbox)
        y_min = min(p[1] for p in bbox)
        x_max = max(p[0] for p in bbox)
        y_max = max(p[1] for p in bbox)
        items.append({
            "text": text,
            "confidence": float(confidence),
            "x": float(x_min),
            "y": float(y_min),
            "w": float(x_max - x_min),
            "h": float(y_max - y_min),
        })
        full_parts.append(text)

output = {"results": items, "full_text": " ".join(full_parts)}
print(json.dumps(output))
"#;

impl OcrEngine for PaddleOcrEngine {
    fn name(&self) -> &str {
        "paddleocr"
    }

    fn is_available(&self) -> bool {
        self.available
    }

    fn version(&self) -> Option<String> {
        self.version.clone()
    }

    fn recognize(
        &self,
        png_data: &[u8],
        region: Option<OcrRegion>,
    ) -> Result<OcrResponse, String> {
        if !self.available {
            return Err("PaddleOCR not available (python3 paddleocr not installed)".into());
        }
        let start = Instant::now();
        let data = if let Some(ref r) = region {
            crop_png(png_data, r)?
        } else {
            png_data.to_vec()
        };

        // Write PNG to a temporary file for the Python script to read
        let tmp = tempfile::Builder::new()
            .suffix(".png")
            .tempfile()
            .map_err(|e| format!("Tempfile creation failed: {}", e))?;
        std::fs::write(tmp.path(), &data)
            .map_err(|e| format!("Tempfile write failed: {}", e))?;

        let path_str = tmp.path().to_string_lossy().into_owned();
        let output = Command::new("python3")
            .args(["-c", PADDLE_SCRIPT, &path_str])
            .output()
            .map_err(|e| format!("PaddleOCR subprocess failed: {}", e))?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            return Err(format!("PaddleOCR exited with error: {}", stderr));
        }

        let stdout = String::from_utf8_lossy(&output.stdout);
        let parsed: serde_json::Value =
            serde_json::from_str(&stdout).map_err(|e| format!("PaddleOCR JSON parse: {}", e))?;

        let results = parsed["results"]
            .as_array()
            .unwrap_or(&vec![])
            .iter()
            .filter_map(|item| {
                Some(OcrResult {
                    text: item["text"].as_str()?.to_string(),
                    confidence: item["confidence"].as_f64()? as f32,
                    x: item["x"].as_f64()? as f32,
                    y: item["y"].as_f64()? as f32,
                    w: item["w"].as_f64()? as f32,
                    h: item["h"].as_f64()? as f32,
                })
            })
            .collect();

        let full_text = parsed["full_text"]
            .as_str()
            .unwrap_or("")
            .to_string();

        Ok(OcrResponse {
            engine: "paddleocr".into(),
            results,
            full_text,
            duration_ms: start.elapsed().as_millis() as u64,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_paddleocr_engine_new() {
        let engine = PaddleOcrEngine::new();
        assert_eq!(engine.name(), "paddleocr");
    }

    #[test]
    fn test_paddleocr_version_matches_availability() {
        let engine = PaddleOcrEngine::new();
        if engine.is_available() {
            assert!(engine.version().is_some());
        }
    }

    #[test]
    fn test_paddleocr_recognize_when_unavailable_returns_error() {
        let engine = PaddleOcrEngine { available: false, version: None };
        let result = engine.recognize(&[], None);
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("not available"));
    }
}
