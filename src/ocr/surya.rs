//! Surya OCR engine via Python subprocess invocation.
//!
//! Checks availability by running `python3 -c "import surya"`.
//! Recognition writes the PNG to a temporary file, invokes a Python script
//! that runs Surya OCR and outputs JSON to stdout, then parses the result.
//! Gracefully returns `available: false` when Python or surya is missing.

use super::{crop_png, OcrEngine, OcrRegion, OcrResponse, OcrResult};
use std::process::Command;
use std::time::Instant;

/// Surya OCR engine that delegates to the Python surya package via subprocess.
pub struct SuryaEngine {
    available: bool,
    version: Option<String>,
}

impl SuryaEngine {
    /// Probes whether `python3 -c "import surya"` succeeds.
    pub fn new() -> Self {
        let (available, version) = check_surya_available();
        Self { available, version }
    }
}

/// Runs a quick Python import check to determine if surya is installed.
fn check_surya_available() -> (bool, Option<String>) {
    let output = Command::new("python3")
        .args(["-c", "import surya; print(surya.__version__)"])
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

/// Python script that runs Surya OCR on a given image file and outputs JSON.
const SURYA_SCRIPT: &str = r#"
import sys, json
from PIL import Image
from surya.recognition import run_recognition
from surya.detection import run_detection
from surya.model.detection.model import load_model as load_det_model, load_processor as load_det_processor
from surya.model.recognition.model import load_model as load_rec_model
from surya.model.recognition.processor import load_processor as load_rec_processor

image = Image.open(sys.argv[1])
det_model = load_det_model()
det_processor = load_det_processor()
rec_model = load_rec_model()
rec_processor = load_rec_processor()

det_result = run_detection([image], det_model, det_processor)
rec_result = run_recognition([image], rec_model, rec_processor, det_result)

items = []
full_parts = []
if rec_result and len(rec_result) > 0:
    for line in rec_result[0].text_lines:
        bbox = line.bbox
        items.append({
            "text": line.text,
            "confidence": float(line.confidence),
            "x": float(bbox[0]),
            "y": float(bbox[1]),
            "w": float(bbox[2] - bbox[0]),
            "h": float(bbox[3] - bbox[1]),
        })
        full_parts.append(line.text)

output = {"results": items, "full_text": " ".join(full_parts)}
print(json.dumps(output))
"#;

impl OcrEngine for SuryaEngine {
    fn name(&self) -> &str {
        "surya"
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
            return Err("Surya not available (python3 surya not installed)".into());
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

        let output = Command::new("python3")
            .args(["-c", SURYA_SCRIPT, &tmp.path().to_string_lossy()])
            .output()
            .map_err(|e| format!("Surya subprocess failed: {}", e))?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            return Err(format!("Surya exited with error: {}", stderr));
        }

        let stdout = String::from_utf8_lossy(&output.stdout);
        let parsed: serde_json::Value =
            serde_json::from_str(&stdout).map_err(|e| format!("Surya JSON parse: {}", e))?;

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
            engine: "surya".into(),
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
    fn test_surya_engine_new() {
        let engine = SuryaEngine::new();
        assert_eq!(engine.name(), "surya");
    }

    #[test]
    fn test_surya_version_matches_availability() {
        let engine = SuryaEngine::new();
        if engine.is_available() {
            assert!(engine.version().is_some());
        }
    }

    #[test]
    fn test_surya_recognize_when_unavailable_returns_error() {
        let engine = SuryaEngine { available: false, version: None };
        let result = engine.recognize(&[], None);
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("not available"));
    }
}
