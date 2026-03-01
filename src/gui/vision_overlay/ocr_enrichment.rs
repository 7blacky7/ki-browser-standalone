//! OCR text enrichment for vision overlay elements using bounding-box intersection matching.
//!
//! Matches OCR engine results (Tesseract, PaddleOCR, Surya) to loaded `OverlayElement`
//! entries by computing the intersection area between each OCR bounding box and each
//! overlay element bounding box. The element with the largest overlap receives the
//! recognized text. A background thread stub (`trigger_ocr_enrichment`) is provided
//! to wire up screenshot-based OCR once screenshot capture infrastructure is ready.

use super::types::{OverlayState, SharedOverlay};

/// Computes the intersection area between two axis-aligned rectangles.
///
/// Each rectangle is defined as (x, y, w, h) in webpage pixel coordinates.
/// Returns 0.0 when the rectangles do not overlap.
#[allow(clippy::too_many_arguments)]
fn intersection_area(ax: f32, ay: f32, aw: f32, ah: f32, bx: f32, by: f32, bw: f32, bh: f32) -> f32 {
    let x_overlap = (ax + aw).min(bx + bw) - ax.max(bx);
    let y_overlap = (ay + ah).min(by + bh) - ay.max(by);
    if x_overlap > 0.0 && y_overlap > 0.0 {
        x_overlap * y_overlap
    } else {
        0.0
    }
}

/// Enriches overlay elements with OCR results by matching bounding-box overlap.
///
/// For each OCR result, finds the overlay element whose bounding box has the
/// largest intersection area, then sets the corresponding `ocr_*` field on
/// that element based on the engine name ("tesseract", "paddleocr", or "surya").
/// Empty or whitespace-only OCR results are skipped.
///
/// `ocr_results` is a slice of `(engine_name, results)` tuples where
/// `engine_name` is one of "tesseract", "paddleocr", or "surya".
pub fn enrich_with_ocr(
    overlay: &SharedOverlay,
    ocr_results: &[(String, Vec<crate::ocr::OcrResult>)],
) {
    let mut guard = match overlay.lock() {
        Ok(g) => g,
        Err(_) => return,
    };
    let elements = match &mut *guard {
        OverlayState::Loaded(ref mut elems) => elems,
        _ => return,
    };

    for (engine_name, results) in ocr_results {
        for ocr in results {
            // Skip empty OCR results
            if ocr.text.trim().is_empty() {
                continue;
            }

            // Find the overlay element with the largest bounding-box overlap
            let mut best_idx: Option<usize> = None;
            let mut best_area: f32 = 0.0;

            for (i, elem) in elements.iter().enumerate() {
                let area = intersection_area(
                    elem.x, elem.y, elem.w, elem.h,
                    ocr.x, ocr.y, ocr.w, ocr.h,
                );
                if area > best_area {
                    best_area = area;
                    best_idx = Some(i);
                }
            }

            // Assign OCR text to the best-matching element
            if let Some(idx) = best_idx {
                let elem = &mut elements[idx];
                match engine_name.as_str() {
                    "tesseract" => elem.ocr_tesseract = Some(ocr.text.clone()),
                    "paddleocr" => elem.ocr_paddleocr = Some(ocr.text.clone()),
                    "surya" => elem.ocr_surya = Some(ocr.text.clone()),
                    _ => {} // Unknown engine, ignore
                }
            }
        }
    }
}

/// Triggers background OCR enrichment for the current overlay elements.
///
/// Spawns a background thread that checks which OCR engines are available
/// and logs their readiness. Once screenshot capture infrastructure is ready,
/// this function will run each available engine on the viewport screenshot
/// and call [`enrich_with_ocr`] to attach the recognized text to overlay elements.
pub fn trigger_ocr_enrichment(
    overlay: SharedOverlay,
    // screenshot: Vec<u8> -- will be provided when screenshot infrastructure is ready
) {
    std::thread::spawn(move || {
        let engines = crate::ocr::all_engines();
        let mut available_count = 0;
        for engine in &engines {
            if engine.is_available() {
                tracing::info!(
                    "OCR engine '{}' (v{}) available for overlay enrichment",
                    engine.name(),
                    engine.version().unwrap_or_else(|| "unknown".to_string()),
                );
                available_count += 1;
            }
        }

        if available_count == 0 {
            tracing::debug!("No OCR engines available for overlay enrichment");
            return;
        }

        tracing::info!(
            "{} OCR engine(s) ready for enrichment -- waiting for screenshot infrastructure",
            available_count,
        );

        // TODO: Once screenshot capture is available:
        // 1. Capture screenshot of the active tab as PNG bytes
        // 2. Run each available engine: engine.recognize(&screenshot_png, None)
        // 3. Collect results as Vec<(String, Vec<OcrResult>)>
        // 4. Call enrich_with_ocr(&overlay, &results) to attach text to elements
        let _ = overlay; // suppress unused warning until screenshot is wired up
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::{Arc, Mutex};
    use egui::Color32;
    use crate::ocr::OcrResult;
    use crate::gui::vision_overlay::types::{OverlayElement, OverlayState};

    /// Helper to create a test OverlayElement at the given position/size.
    fn make_elem(id: u32, x: f32, y: f32, w: f32, h: f32) -> OverlayElement {
        OverlayElement {
            id,
            x,
            y,
            w,
            h,
            label: format!("elem-{}", id),
            color: Color32::RED,
            element_type: "test".to_string(),
            ocr_tesseract: None,
            ocr_paddleocr: None,
            ocr_surya: None,
        }
    }

    #[test]
    fn test_overlay_element_ocr_fields_default_none() {
        let elem = make_elem(1, 0.0, 0.0, 100.0, 50.0);
        assert!(elem.ocr_tesseract.is_none());
        assert!(elem.ocr_paddleocr.is_none());
        assert!(elem.ocr_surya.is_none());
    }

    #[test]
    fn test_enrich_with_ocr_basic() {
        // Create two overlay elements side by side
        let elem1 = make_elem(1, 0.0, 0.0, 100.0, 50.0);
        let elem2 = make_elem(2, 150.0, 0.0, 100.0, 50.0);

        let overlay: SharedOverlay = Arc::new(Mutex::new(
            OverlayState::Loaded(vec![elem1, elem2]),
        ));

        // OCR result overlapping with elem1 (at position 10,10 size 80x30)
        let ocr_results = vec![
            ("tesseract".to_string(), vec![
                OcrResult {
                    text: "Hello World".to_string(),
                    confidence: 0.95,
                    x: 10.0,
                    y: 10.0,
                    w: 80.0,
                    h: 30.0,
                },
            ]),
            ("paddleocr".to_string(), vec![
                OcrResult {
                    text: "Button Text".to_string(),
                    confidence: 0.88,
                    x: 160.0,
                    y: 5.0,
                    w: 70.0,
                    h: 20.0,
                },
            ]),
        ];

        enrich_with_ocr(&overlay, &ocr_results);

        // Verify: elem1 should have tesseract text, elem2 should have paddleocr text
        let guard = overlay.lock().unwrap();
        if let OverlayState::Loaded(ref elems) = *guard {
            assert_eq!(elems[0].ocr_tesseract, Some("Hello World".to_string()));
            assert!(elems[0].ocr_paddleocr.is_none());
            assert!(elems[0].ocr_surya.is_none());

            assert!(elems[1].ocr_tesseract.is_none());
            assert_eq!(elems[1].ocr_paddleocr, Some("Button Text".to_string()));
            assert!(elems[1].ocr_surya.is_none());
        } else {
            panic!("Expected OverlayState::Loaded");
        }
    }

    #[test]
    fn test_enrich_with_ocr_empty() {
        let elem = make_elem(1, 0.0, 0.0, 100.0, 50.0);
        let overlay: SharedOverlay = Arc::new(Mutex::new(
            OverlayState::Loaded(vec![elem]),
        ));

        // Empty OCR results should not modify anything
        let ocr_results: Vec<(String, Vec<OcrResult>)> = vec![];
        enrich_with_ocr(&overlay, &ocr_results);

        let guard = overlay.lock().unwrap();
        if let OverlayState::Loaded(ref elems) = *guard {
            assert!(elems[0].ocr_tesseract.is_none());
            assert!(elems[0].ocr_paddleocr.is_none());
            assert!(elems[0].ocr_surya.is_none());
        } else {
            panic!("Expected OverlayState::Loaded");
        }
    }

    #[test]
    fn test_enrich_with_ocr_skips_empty_text() {
        let elem = make_elem(1, 0.0, 0.0, 100.0, 50.0);
        let overlay: SharedOverlay = Arc::new(Mutex::new(
            OverlayState::Loaded(vec![elem]),
        ));

        // OCR result with empty/whitespace text should be skipped
        let ocr_results = vec![
            ("tesseract".to_string(), vec![
                OcrResult {
                    text: "   ".to_string(),
                    confidence: 0.5,
                    x: 10.0,
                    y: 10.0,
                    w: 80.0,
                    h: 30.0,
                },
            ]),
        ];
        enrich_with_ocr(&overlay, &ocr_results);

        let guard = overlay.lock().unwrap();
        if let OverlayState::Loaded(ref elems) = *guard {
            assert!(elems[0].ocr_tesseract.is_none());
        } else {
            panic!("Expected OverlayState::Loaded");
        }
    }

    #[test]
    fn test_enrich_with_ocr_no_overlap() {
        let elem = make_elem(1, 0.0, 0.0, 50.0, 50.0);
        let overlay: SharedOverlay = Arc::new(Mutex::new(
            OverlayState::Loaded(vec![elem]),
        ));

        // OCR result far away from the element -- no overlap
        let ocr_results = vec![
            ("surya".to_string(), vec![
                OcrResult {
                    text: "Distant Text".to_string(),
                    confidence: 0.9,
                    x: 500.0,
                    y: 500.0,
                    w: 100.0,
                    h: 30.0,
                },
            ]),
        ];
        enrich_with_ocr(&overlay, &ocr_results);

        let guard = overlay.lock().unwrap();
        if let OverlayState::Loaded(ref elems) = *guard {
            assert!(elems[0].ocr_surya.is_none());
        } else {
            panic!("Expected OverlayState::Loaded");
        }
    }

    #[test]
    fn test_enrich_with_ocr_best_overlap_wins() {
        // Two elements: elem1 partially overlaps, elem2 fully overlaps with OCR bbox
        let elem1 = make_elem(1, 0.0, 0.0, 60.0, 50.0);    // overlaps OCR by small area
        let elem2 = make_elem(2, 40.0, 0.0, 100.0, 50.0);   // overlaps OCR by large area
        let overlay: SharedOverlay = Arc::new(Mutex::new(
            OverlayState::Loaded(vec![elem1, elem2]),
        ));

        // OCR result centered on elem2 (50..130 x 0..30)
        let ocr_results = vec![
            ("tesseract".to_string(), vec![
                OcrResult {
                    text: "Overlapping".to_string(),
                    confidence: 0.9,
                    x: 50.0,
                    y: 0.0,
                    w: 80.0,
                    h: 30.0,
                },
            ]),
        ];
        enrich_with_ocr(&overlay, &ocr_results);

        let guard = overlay.lock().unwrap();
        if let OverlayState::Loaded(ref elems) = *guard {
            // elem1 overlaps by 10*30=300, elem2 overlaps by 80*30=2400
            // So elem2 should get the OCR text
            assert!(elems[0].ocr_tesseract.is_none());
            assert_eq!(elems[1].ocr_tesseract, Some("Overlapping".to_string()));
        } else {
            panic!("Expected OverlayState::Loaded");
        }
    }

    #[test]
    fn test_intersection_area_no_overlap() {
        assert_eq!(intersection_area(0.0, 0.0, 10.0, 10.0, 20.0, 20.0, 10.0, 10.0), 0.0);
    }

    #[test]
    fn test_intersection_area_partial() {
        // Rect A: 0..10 x 0..10, Rect B: 5..15 x 5..15 => overlap 5x5 = 25
        assert_eq!(intersection_area(0.0, 0.0, 10.0, 10.0, 5.0, 5.0, 10.0, 10.0), 25.0);
    }

    #[test]
    fn test_intersection_area_full_containment() {
        // Rect A: 0..100 x 0..100, Rect B: 10..30 x 10..30 => overlap 20x20 = 400
        assert_eq!(intersection_area(0.0, 0.0, 100.0, 100.0, 10.0, 10.0, 20.0, 20.0), 400.0);
    }
}
