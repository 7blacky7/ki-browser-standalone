//! Automatic consent dialog handler.
//!
//! Detects and accepts cookie consent dialogs from common CMP providers
//! (Sourcepoint, Cookiebot, OneTrust, Quantcast, TCF API, generic buttons).
//! Solves the problem of consent iFrames where visual button positions
//! don't match clickable DOM coordinates.

use axum::{
    extract::State,
    http::StatusCode,
    response::IntoResponse,
    routing::post,
    Json, Router,
};
use serde::{Deserialize, Serialize};
use tracing::info;

use crate::api::debug_routes::types::{evaluate_in_tab, resolve_tab_id};
use crate::api::ipc::{IpcCommand, IpcMessage};
use crate::api::routes::ApiResponse;
use crate::api::server::AppState;

// ============================================================================
// Types
// ============================================================================

#[derive(Debug, Deserialize)]
pub struct ConsentRequest {
    #[serde(default)]
    pub tab_id: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct ConsentResult {
    /// Whether a consent dialog was detected.
    pub detected: bool,
    /// Which CMP provider was found (sourcepoint, cookiebot, onetrust, tcf, generic, none).
    pub provider: String,
    /// What action was taken.
    pub action: String,
    /// Whether the consent was successfully accepted.
    pub accepted: bool,
}

// ============================================================================
// Consent detection and acceptance script
// ============================================================================

/// Comprehensive JS script that detects and accepts consent dialogs.
///
/// Strategy:
/// 1. Check for CMP iFrames (Sourcepoint, Cookiebot) — click iFrame center
/// 2. Try TCF API (__tcfapi) consent acceptance
/// 3. Try CMP-specific JS APIs (_sp_, Cookiebot, __cmp)
/// 4. Search for accept/consent buttons in the main document
/// 5. Return what was found and what action was taken
const CONSENT_DETECT_SCRIPT: &str = r#"(function() {
    var result = {detected: false, provider: 'none', action: 'none', iframeClick: null};

    // --- Strategy 1: Find CMP iFrames ---
    var iframes = document.querySelectorAll('iframe');
    for (var i = 0; i < iframes.length; i++) {
        var f = iframes[i];
        var id = f.id || '';
        var src = f.src || '';

        // Sourcepoint
        if (id.match(/sp_message_iframe/) || src.includes('cmp-cdn') || src.includes('sourcepoint')) {
            var r = f.getBoundingClientRect();
            if (r.width > 10 && r.height > 10) {
                result.detected = true;
                result.provider = 'sourcepoint';
                result.action = 'iframe_click';
                result.iframeClick = {x: Math.round(r.x + r.width/2), y: Math.round(r.y + r.height/2)};
                return JSON.stringify(result);
            }
        }

        // Cookiebot
        if (id === 'CybotCookiebotDialog' || src.includes('cookiebot') || src.includes('consentmanager')) {
            var r = f.getBoundingClientRect();
            if (r.width > 10 && r.height > 10) {
                result.detected = true;
                result.provider = 'cookiebot';
                result.action = 'iframe_click';
                result.iframeClick = {x: Math.round(r.x + r.width/2), y: Math.round(r.y + r.height/2)};
                return JSON.stringify(result);
            }
        }
    }

    // --- Strategy 2: TCF API ---
    if (typeof window.__tcfapi === 'function') {
        result.detected = true;
        result.provider = 'tcf';
    }

    // --- Strategy 3: CMP-specific JS APIs ---
    if (window._sp_ && window._sp_.destroyMessages) {
        result.detected = true;
        result.provider = 'sourcepoint_api';
        try { window._sp_.destroyMessages(); result.action = 'sp_destroy'; } catch(e) {}
    }

    if (window.Cookiebot && window.Cookiebot.submitCustomConsent) {
        result.detected = true;
        result.provider = 'cookiebot_api';
        try { window.Cookiebot.submitCustomConsent(true, true, true); result.action = 'cookiebot_accept'; } catch(e) {}
    }

    if (window.__cmp && typeof window.__cmp === 'function') {
        result.detected = true;
        result.provider = 'cmp_api';
        try { window.__cmp('consentAll'); result.action = 'cmp_accept'; } catch(e) {}
    }

    if (window.UC_UI && window.UC_UI.acceptAllConsents) {
        result.detected = true;
        result.provider = 'usercentrics';
        try { window.UC_UI.acceptAllConsents(); result.action = 'uc_accept'; } catch(e) {}
    }

    // --- Strategy 4: Generic button search ---
    if (!result.detected || result.action === 'none') {
        var patterns = [
            /^(alle\s+)?akzeptieren$/i,
            /^zustimmen(\s+und\s+weiter)?$/i,
            /^(alle\s+)?cookies?\s+(akzeptieren|annehmen|zulassen)$/i,
            /^accept(\s+all)?(\s+cookies)?$/i,
            /^agree(\s+and\s+continue)?$/i,
            /^consent$/i,
            /^(ich\s+)?stimme?\s+zu$/i,
            /^einverstanden$/i,
            /^okay?$/i,
            /^got\s+it$/i
        ];

        var candidates = Array.from(document.querySelectorAll(
            'button, a.btn, [role="button"], input[type="submit"], ' +
            '.cmp-accept-all, .sp_choice_type_11, ' +
            '[data-testid*="accept"], [data-testid*="consent"], ' +
            '[id*="accept"], [id*="consent"], [class*="accept"], [class*="consent"]'
        ));

        for (var j = 0; j < candidates.length; j++) {
            var btn = candidates[j];
            var text = btn.textContent.trim();
            var r = btn.getBoundingClientRect();

            if (r.width < 30 || r.height < 15 || r.width > 800) continue;

            for (var k = 0; k < patterns.length; k++) {
                if (patterns[k].test(text)) {
                    result.detected = true;
                    result.provider = result.provider === 'none' ? 'generic' : result.provider;
                    btn.click();
                    result.action = 'button_click:' + text;
                    return JSON.stringify(result);
                }
            }
        }
    }

    return JSON.stringify(result);
})()"#;

// ============================================================================
// Handler
// ============================================================================

/// POST /debug/consent/accept — Detect and accept consent dialogs.
///
/// Tries multiple strategies:
/// 1. CMP iFrame detection → coordinate click
/// 2. CMP JS APIs (Sourcepoint, Cookiebot, TCF, Usercentrics)
/// 3. Generic button matching (Akzeptieren, Accept, Zustimmen, etc.)
async fn accept_consent(
    State(state): State<AppState>,
    Json(request): Json<ConsentRequest>,
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

    // Step 1: Run detection script
    let detect_result = match evaluate_in_tab(&state, &tab_id, CONSENT_DETECT_SCRIPT).await {
        Ok(raw) => match serde_json::from_str::<ConsentDetectResult>(&raw) {
            Ok(r) => r,
            Err(e) => {
                return (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    Json(ApiResponse::<()>::error(format!("Parse error: {}", e))),
                )
                    .into_response()
            }
        },
        Err(err) => return err.into_response(),
    };

    // Step 2: If iFrame click needed, do it via the click IPC command
    if let Some(click_pos) = detect_result.iframe_click {
        info!(
            "Consent iFrame detected ({}), clicking at ({}, {})",
            detect_result.provider, click_pos.x, click_pos.y
        );

        let click_command = IpcCommand::ClickCoordinates {
            tab_id: tab_id.clone(),
            x: click_pos.x,
            y: click_pos.y,
            button: "left".to_string(),
            modifiers: None,
        };

        let click_result = state
            .ipc_channel
            .send_command(IpcMessage::Command(click_command))
            .await;

        // Wait for page to potentially reload after consent
        tokio::time::sleep(tokio::time::Duration::from_secs(2)).await;

        // Check if consent was actually accepted (more links = content loaded)
        let check = evaluate_in_tab(
            &state,
            &tab_id,
            "document.querySelectorAll('a').length",
        )
        .await;

        let link_count: i64 = check
            .ok()
            .and_then(|s| s.parse().ok())
            .unwrap_or(0);

        let accepted = link_count > 30;

        return Json(ApiResponse::success(ConsentResult {
            detected: true,
            provider: detect_result.provider,
            action: format!("iframe_click({},{})", click_pos.x, click_pos.y),
            accepted,
        }))
        .into_response();
    }

    // Step 3: JS-based consent was already attempted in the detect script
    let accepted = detect_result.action != "none";

    if accepted {
        // Wait for effect
        tokio::time::sleep(tokio::time::Duration::from_secs(1)).await;
    }

    Json(ApiResponse::success(ConsentResult {
        detected: detect_result.detected,
        provider: detect_result.provider,
        action: detect_result.action,
        accepted,
    }))
    .into_response()
}

// ============================================================================
// Internal types for JS result parsing
// ============================================================================

#[derive(Debug, Deserialize)]
struct ConsentDetectResult {
    detected: bool,
    provider: String,
    action: String,
    #[serde(rename = "iframeClick")]
    iframe_click: Option<ClickPosition>,
}

#[derive(Debug, Deserialize)]
struct ClickPosition {
    x: i32,
    y: i32,
}

// ============================================================================
// Router
// ============================================================================

pub fn consent_routes() -> Router<AppState> {
    Router::new().route("/debug/consent/accept", post(accept_consent))
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_consent_result_serialization() {
        let r = ConsentResult {
            detected: true,
            provider: "sourcepoint".to_string(),
            action: "iframe_click(790,368)".to_string(),
            accepted: true,
        };
        let json = serde_json::to_string(&r).expect("serialize");
        assert!(json.contains("sourcepoint"));
        assert!(json.contains("\"accepted\":true"));
    }

    #[test]
    fn test_consent_detect_result_deserialization() {
        let json = r#"{"detected":true,"provider":"sourcepoint","action":"iframe_click","iframeClick":{"x":790,"y":368}}"#;
        let r: ConsentDetectResult = serde_json::from_str(json).expect("deserialize");
        assert!(r.detected);
        assert_eq!(r.provider, "sourcepoint");
        assert!(r.iframe_click.is_some());
        let pos = r.iframe_click.unwrap();
        assert_eq!(pos.x, 790);
        assert_eq!(pos.y, 368);
    }

    #[test]
    fn test_consent_detect_result_no_iframe() {
        let json = r#"{"detected":true,"provider":"generic","action":"button_click:Akzeptieren","iframeClick":null}"#;
        let r: ConsentDetectResult = serde_json::from_str(json).expect("deserialize");
        assert!(r.iframe_click.is_none());
        assert_eq!(r.action, "button_click:Akzeptieren");
    }

    #[test]
    fn test_consent_detect_result_nothing_found() {
        let json = r#"{"detected":false,"provider":"none","action":"none","iframeClick":null}"#;
        let r: ConsentDetectResult = serde_json::from_str(json).expect("deserialize");
        assert!(!r.detected);
    }

    #[test]
    fn test_consent_request_empty() {
        let json = r#"{}"#;
        let r: ConsentRequest = serde_json::from_str(json).expect("deserialize");
        assert!(r.tab_id.is_none());
    }
}
