//! CAPTCHA detection and solving endpoints.
//!
//! Detects common CAPTCHA types (reCAPTCHA, Cloudflare Turnstile, hCaptcha,
//! text CAPTCHAs) and provides tools for agents to solve them:
//! - Checkbox CAPTCHAs: Solved automatically via coordinate click
//! - Image grid CAPTCHAs: Returns screenshot + grid info for agent analysis
//! - Text CAPTCHAs: Returns image for OCR processing

use axum::{
    extract::State,
    http::StatusCode,
    response::IntoResponse,
    routing::post,
    Json, Router,
};
use serde::{Deserialize, Serialize};

use crate::api::debug_routes::types::{evaluate_in_tab, resolve_tab_id};
use crate::api::ipc::{IpcCommand, IpcMessage};
use crate::api::routes::ApiResponse;
use crate::api::server::AppState;

// ============================================================================
// Types
// ============================================================================

#[derive(Debug, Deserialize)]
pub struct CaptchaRequest {
    #[serde(default)]
    pub tab_id: Option<String>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct CaptchaDetectResult {
    /// Whether a CAPTCHA was detected.
    pub detected: bool,
    /// CAPTCHA type: "recaptcha_checkbox", "recaptcha_image", "cloudflare_turnstile",
    /// "hcaptcha", "text_captcha", "google_sorry", "none"
    pub captcha_type: String,
    /// Position of the CAPTCHA element for clicking.
    pub position: Option<CaptchaPosition>,
    /// URL of the CAPTCHA iframe (if any).
    pub iframe_url: Option<String>,
    /// Hint for the agent on how to proceed.
    pub hint: String,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct CaptchaPosition {
    pub x: i32,
    pub y: i32,
    pub width: i32,
    pub height: i32,
}

#[derive(Debug, Serialize)]
pub struct CaptchaSolveResult {
    /// Whether the solve attempt was made.
    pub attempted: bool,
    /// What was done.
    pub action: String,
    /// Whether the CAPTCHA appears to be solved after the attempt.
    pub solved: bool,
    /// Hint for next steps if not solved.
    pub next_step: Option<String>,
}

// ============================================================================
// Detection Script
// ============================================================================

const CAPTCHA_DETECT_SCRIPT: &str = r#"(function() {
    var result = {detected: false, captcha_type: 'none', position: null, iframe_url: null, hint: 'No CAPTCHA detected'};

    // --- Google Sorry/reCAPTCHA page ---
    if (location.href.includes('google.com/sorry') || document.querySelector('#captcha-form')) {
        var recaptchaIframe = document.querySelector('iframe[src*="recaptcha"], iframe[src*="anchor"]');
        if (recaptchaIframe) {
            var r = recaptchaIframe.getBoundingClientRect();
            result.detected = true;
            result.captcha_type = 'recaptcha_checkbox';
            result.position = {x: Math.round(r.x + 27), y: Math.round(r.y + 27), width: Math.round(r.width), height: Math.round(r.height)};
            result.iframe_url = recaptchaIframe.src;
            result.hint = 'Click the checkbox at position. If image challenge appears, use /debug/captcha/screenshot for grid analysis.';
        } else {
            result.detected = true;
            result.captcha_type = 'google_sorry';
            result.hint = 'Google sorry page without visible reCAPTCHA. Try waiting or changing IP.';
        }
        return JSON.stringify(result);
    }

    // --- reCAPTCHA v2 checkbox ---
    var recaptchaFrame = document.querySelector('iframe[src*="recaptcha/api2/anchor"], iframe[src*="recaptcha/enterprise/anchor"]');
    if (recaptchaFrame) {
        var r = recaptchaFrame.getBoundingClientRect();
        if (r.width > 0 && r.height > 0) {
            result.detected = true;
            result.captcha_type = 'recaptcha_checkbox';
            result.position = {x: Math.round(r.x + 27), y: Math.round(r.y + 27), width: Math.round(r.width), height: Math.round(r.height)};
            result.iframe_url = recaptchaFrame.src;
            result.hint = 'Click checkbox at position. Checkbox is ~27px from top-left of iframe.';
            return JSON.stringify(result);
        }
    }

    // --- reCAPTCHA v2 image challenge (buster frame visible) ---
    var challengeFrame = document.querySelector('iframe[src*="recaptcha/api2/bframe"], iframe[src*="recaptcha/enterprise/bframe"]');
    if (challengeFrame) {
        var r = challengeFrame.getBoundingClientRect();
        if (r.width > 100 && r.height > 100) {
            result.detected = true;
            result.captcha_type = 'recaptcha_image';
            result.position = {x: Math.round(r.x), y: Math.round(r.y), width: Math.round(r.width), height: Math.round(r.height)};
            result.iframe_url = challengeFrame.src;
            result.hint = 'Image grid challenge. Use /screenshot with clip to capture the grid, analyze with vision, click correct tiles.';
            return JSON.stringify(result);
        }
    }

    // --- Cloudflare Turnstile ---
    var turnstile = document.querySelector('iframe[src*="challenges.cloudflare.com"], iframe[src*="turnstile"]');
    if (turnstile) {
        var r = turnstile.getBoundingClientRect();
        if (r.width > 0 && r.height > 0) {
            result.detected = true;
            result.captcha_type = 'cloudflare_turnstile';
            result.position = {x: Math.round(r.x + r.width/2), y: Math.round(r.y + r.height/2), width: Math.round(r.width), height: Math.round(r.height)};
            result.hint = 'Cloudflare Turnstile checkbox. Click center of iframe.';
            return JSON.stringify(result);
        }
    }

    // --- hCaptcha ---
    var hcaptcha = document.querySelector('iframe[src*="hcaptcha.com"]');
    if (hcaptcha) {
        var r = hcaptcha.getBoundingClientRect();
        if (r.width > 0 && r.height > 0) {
            result.detected = true;
            result.captcha_type = 'hcaptcha';
            result.position = {x: Math.round(r.x + 27), y: Math.round(r.y + 27), width: Math.round(r.width), height: Math.round(r.height)};
            result.hint = 'hCaptcha checkbox. Click at position. If image challenge, use screenshot.';
            return JSON.stringify(result);
        }
    }

    // --- Generic text CAPTCHA (image with input field) ---
    var captchaImg = document.querySelector('img[src*="captcha"], img[alt*="captcha"], img[alt*="CAPTCHA"], #captcha-image, .captcha-image');
    var captchaInput = document.querySelector('input[name*="captcha"], input[id*="captcha"], #captchacharacters');
    if (captchaImg && captchaInput) {
        var imgR = captchaImg.getBoundingClientRect();
        result.detected = true;
        result.captcha_type = 'text_captcha';
        result.position = {x: Math.round(imgR.x), y: Math.round(imgR.y), width: Math.round(imgR.width), height: Math.round(imgR.height)};
        result.hint = 'Text CAPTCHA. Screenshot the image area, OCR it, type result into input field.';
        return JSON.stringify(result);
    }

    // --- Check for any generic challenge indicators ---
    var bodyText = document.body ? document.body.innerText.substring(0, 2000) : '';
    if (bodyText.match(/Ich bin kein Roboter|I.m not a robot|verify you.re human|Bestätigen Sie|unusual traffic|automated queries/i)) {
        result.detected = true;
        result.captcha_type = 'generic_challenge';
        result.hint = 'Generic bot challenge detected in page text. Take screenshot to identify type.';
    }

    return JSON.stringify(result);
})()"#;

// ============================================================================
// Handlers
// ============================================================================

/// POST /debug/captcha/detect — Detect CAPTCHA type and position.
async fn detect_captcha(
    State(state): State<AppState>,
    Json(request): Json<CaptchaRequest>,
) -> impl IntoResponse {
    let tab_id = match resolve_tab_id(&state, request.tab_id).await {
        Some(id) => id,
        None => {
            return (
                StatusCode::BAD_REQUEST,
                Json(ApiResponse::<()>::error("No active tab")),
            )
                .into_response()
        }
    };

    match evaluate_in_tab(&state, &tab_id, CAPTCHA_DETECT_SCRIPT).await {
        Ok(json_str) => match serde_json::from_str::<CaptchaDetectResult>(&json_str) {
            Ok(result) => Json(ApiResponse::success(result)).into_response(),
            Err(e) => (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ApiResponse::<()>::error(format!("Parse error: {}", e))),
            )
                .into_response(),
        },
        Err(err) => err.into_response(),
    }
}

/// POST /debug/captcha/solve — Attempt to solve a detected CAPTCHA.
///
/// For checkbox CAPTCHAs (reCAPTCHA, Turnstile, hCaptcha): clicks the checkbox.
/// For image/text CAPTCHAs: returns instructions for the agent.
async fn solve_captcha(
    State(state): State<AppState>,
    Json(request): Json<CaptchaRequest>,
) -> impl IntoResponse {
    let tab_id = match resolve_tab_id(&state, request.tab_id).await {
        Some(id) => id,
        None => {
            return (
                StatusCode::BAD_REQUEST,
                Json(ApiResponse::<()>::error("No active tab")),
            )
                .into_response()
        }
    };

    // Step 1: Detect the CAPTCHA
    let detect_result = match evaluate_in_tab(&state, &tab_id, CAPTCHA_DETECT_SCRIPT).await {
        Ok(json_str) => match serde_json::from_str::<CaptchaDetectResult>(&json_str) {
            Ok(r) => r,
            Err(e) => {
                return (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    Json(ApiResponse::<()>::error(format!("Detect parse error: {}", e))),
                )
                    .into_response()
            }
        },
        Err(err) => return err.into_response(),
    };

    if !detect_result.detected {
        return Json(ApiResponse::success(CaptchaSolveResult {
            attempted: false,
            action: "none".to_string(),
            solved: true,
            next_step: None,
        }))
        .into_response();
    }

    // Step 2: For checkbox types — click automatically
    match detect_result.captcha_type.as_str() {
        "recaptcha_checkbox" | "cloudflare_turnstile" | "hcaptcha" => {
            if let Some(pos) = &detect_result.position {
                let click_cmd = IpcCommand::ClickCoordinates {
                    tab_id: tab_id.clone(),
                    x: pos.x,
                    y: pos.y,
                    button: "left".to_string(),
                    modifiers: None,
                };

                let _ = state
                    .ipc_channel
                    .send_command(IpcMessage::Command(click_cmd))
                    .await;

                // Wait for CAPTCHA to process
                tokio::time::sleep(tokio::time::Duration::from_secs(3)).await;

                // Check if an image challenge appeared (reCAPTCHA may escalate)
                let recheck = evaluate_in_tab(&state, &tab_id, CAPTCHA_DETECT_SCRIPT).await;
                let still_captcha = recheck
                    .ok()
                    .and_then(|s| serde_json::from_str::<CaptchaDetectResult>(&s).ok())
                    .map(|r| r.detected)
                    .unwrap_or(false);

                return Json(ApiResponse::success(CaptchaSolveResult {
                    attempted: true,
                    action: format!("clicked_checkbox({},{})", pos.x, pos.y),
                    solved: !still_captcha,
                    next_step: if still_captcha {
                        Some("Checkbox click triggered image challenge. Use /screenshot to capture grid, analyze with vision, click correct tiles via /click.".to_string())
                    } else {
                        None
                    },
                }))
                .into_response();
            }
        }
        "recaptcha_image" => {
            return Json(ApiResponse::success(CaptchaSolveResult {
                attempted: false,
                action: "image_challenge_detected".to_string(),
                solved: false,
                next_step: Some(format!(
                    "Image grid CAPTCHA at ({},{} {}x{}). Steps: 1) GET /screenshot?clip_x={}&clip_y={}&clip_width={}&clip_height={}&clip_scale=2&format=jpeg&quality=90 2) Analyze image with vision 3) POST /click for each correct tile 4) Click verify button",
                    detect_result.position.as_ref().map(|p| p.x).unwrap_or(0),
                    detect_result.position.as_ref().map(|p| p.y).unwrap_or(0),
                    detect_result.position.as_ref().map(|p| p.width).unwrap_or(0),
                    detect_result.position.as_ref().map(|p| p.height).unwrap_or(0),
                    detect_result.position.as_ref().map(|p| p.x).unwrap_or(0),
                    detect_result.position.as_ref().map(|p| p.y).unwrap_or(0),
                    detect_result.position.as_ref().map(|p| p.width).unwrap_or(0),
                    detect_result.position.as_ref().map(|p| p.height).unwrap_or(0),
                )),
            }))
            .into_response();
        }
        "text_captcha" => {
            return Json(ApiResponse::success(CaptchaSolveResult {
                attempted: false,
                action: "text_captcha_detected".to_string(),
                solved: false,
                next_step: Some(format!(
                    "Text CAPTCHA image at ({},{} {}x{}). Steps: 1) GET /screenshot with clip on image area 2) OCR the text 3) POST /type into captcha input field 4) Submit form",
                    detect_result.position.as_ref().map(|p| p.x).unwrap_or(0),
                    detect_result.position.as_ref().map(|p| p.y).unwrap_or(0),
                    detect_result.position.as_ref().map(|p| p.width).unwrap_or(0),
                    detect_result.position.as_ref().map(|p| p.height).unwrap_or(0),
                )),
            }))
            .into_response();
        }
        _ => {}
    }

    Json(ApiResponse::success(CaptchaSolveResult {
        attempted: false,
        action: format!("unsupported_type:{}", detect_result.captcha_type),
        solved: false,
        next_step: Some("Take screenshot and analyze visually.".to_string()),
    }))
    .into_response()
}

// ============================================================================
// Router
// ============================================================================

pub fn captcha_routes() -> Router<AppState> {
    Router::new()
        .route("/debug/captcha/detect", post(detect_captcha))
        .route("/debug/captcha/solve", post(solve_captcha))
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_captcha_detect_result_serialization() {
        let r = CaptchaDetectResult {
            detected: true,
            captcha_type: "recaptcha_checkbox".to_string(),
            position: Some(CaptchaPosition { x: 100, y: 200, width: 300, height: 80 }),
            iframe_url: Some("https://www.google.com/recaptcha/api2/anchor".to_string()),
            hint: "Click checkbox".to_string(),
        };
        let json = serde_json::to_string(&r).expect("serialize");
        assert!(json.contains("recaptcha_checkbox"));
        assert!(json.contains("\"x\":100"));
    }

    #[test]
    fn test_captcha_solve_result_serialization() {
        let r = CaptchaSolveResult {
            attempted: true,
            action: "clicked_checkbox(100,200)".to_string(),
            solved: true,
            next_step: None,
        };
        let json = serde_json::to_string(&r).expect("serialize");
        assert!(json.contains("\"solved\":true"));
    }

    #[test]
    fn test_captcha_request_empty() {
        let json = "{}";
        let r: CaptchaRequest = serde_json::from_str(json).expect("deserialize");
        assert!(r.tab_id.is_none());
    }

    #[test]
    fn test_captcha_detect_no_captcha() {
        let json = r#"{"detected":false,"captcha_type":"none","position":null,"iframe_url":null,"hint":"No CAPTCHA detected"}"#;
        let r: CaptchaDetectResult = serde_json::from_str(json).expect("deserialize");
        assert!(!r.detected);
        assert_eq!(r.captcha_type, "none");
    }
}
