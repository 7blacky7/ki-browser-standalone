//! Per-tab stealth identity: API-facing types, resolution and introspection.
//!
//! A tab identity describes the complete fingerprint a tab presents to the web:
//! user agent, platform, languages, hardware, WebGL strings, screen and timezone.
//! Identities are specified via the optional `identity` parameter of `POST /tabs/new`:
//!
//! - `"random"` (default) — a fresh, internally consistent Chrome profile
//!   ([`StealthConfig::random_chrome`]), NOT per-field randomness.
//! - `"consistent:<seed>"` — deterministic profile derived from the seed.
//! - explicit object — any subset of fields; missing fields are filled
//!   consistently from the default profile.
//!
//! The resolved [`StealthConfig`] is the single source of truth for a tab:
//! it drives the JS overrides (CDP init script + CEF load handler) AND the
//! HTTP layer (CDP `Emulation.setUserAgentOverride` with `acceptLanguage`),
//! so `Accept-Language` header == `navigator.languages` and
//! HTTP `User-Agent` == `navigator.userAgent`.

use serde::{Deserialize, Serialize};

use crate::stealth::fingerprint::ScreenResolution;
use crate::stealth::StealthConfig;

// ============================================================================
// API types
// ============================================================================

/// Identity specification accepted by `POST /tabs/new`.
///
/// Deserializes from either a string (`"random"`, `"consistent:<seed>"`)
/// or an explicit object with per-field overrides.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
pub enum IdentitySpec {
    /// Named profile: `"random"` or `"consistent:<seed>"`.
    Named(String),
    /// Explicit per-field overrides; missing fields are filled from the default profile.
    Explicit(IdentityOverrides),
}

/// Screen dimensions for an explicit identity.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ScreenSpec {
    pub width: u32,
    pub height: u32,
}

/// Explicit identity field overrides. All fields optional; anything omitted
/// is filled consistently from the base profile (`random_chrome()` or
/// `consistent:<seed>` when `seed` is given).
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct IdentityOverrides {
    /// Optional seed for the base profile (deterministic fill of missing fields).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub seed: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub user_agent: Option<String>,
    /// e.g. "Win32", "MacIntel", "Linux x86_64"
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub platform: Option<String>,
    /// e.g. ["de-DE", "de", "en"]
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub languages: Option<Vec<String>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub hardware_concurrency: Option<u8>,
    /// Device memory in GB (realistic values: 2, 4, 8, 16, 32)
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub device_memory: Option<u8>,
    /// WEBGL_debug_renderer_info vendor string (e.g. "Google Inc. (NVIDIA)")
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub webgl_vendor: Option<String>,
    /// WEBGL_debug_renderer_info renderer string
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub webgl_renderer: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub screen: Option<ScreenSpec>,
    /// IANA timezone name (e.g. "Europe/Berlin")
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub timezone: Option<String>,
}

// ============================================================================
// Resolution
// ============================================================================

/// Resolves an optional identity specification into a complete, internally
/// consistent [`StealthConfig`].
///
/// `viewport` is the actual browser viewport (window size); the screen
/// fingerprint is synced to it unless an explicit `screen` override is given.
///
/// Returns a human-readable error for unknown named profiles or invalid values.
pub fn resolve_identity(
    spec: Option<&IdentitySpec>,
    viewport: (u32, u32),
) -> Result<StealthConfig, String> {
    let mut config = match spec {
        None => StealthConfig::random_chrome(),
        Some(IdentitySpec::Named(name)) => {
            // Reuse the established profile string parser (agent_routes).
            match crate::api::agent_routes::parse_stealth_profile(name) {
                Some(c) if name == "random" => {
                    // "random" must be Chrome-consistent on a Chromium engine:
                    // discard the cross-browser random profile in favor of random_chrome().
                    let _ = c;
                    StealthConfig::random_chrome()
                }
                Some(c) => c,
                None => {
                    return Err(format!(
                        "Unknown identity '{}'. Expected \"random\", \"consistent:<seed>\" \
                         or an identity object ({{user_agent, platform, languages, ...}})",
                        name
                    ));
                }
            }
        }
        Some(IdentitySpec::Explicit(o)) => {
            let mut base = match &o.seed {
                Some(seed) => StealthConfig::consistent(seed),
                None => StealthConfig::random_chrome(),
            };
            base.sync_screen_to_viewport(viewport.0, viewport.1);
            apply_overrides(&mut base, o)?;
            base.validate()?;
            return Ok(base);
        }
    };

    config.sync_screen_to_viewport(viewport.0, viewport.1);
    config.validate()?;
    Ok(config)
}

/// Applies explicit field overrides onto a base config, keeping all derived
/// values (navigator overrides, app_version, primary language) consistent.
fn apply_overrides(config: &mut StealthConfig, o: &IdentityOverrides) -> Result<(), String> {
    if let Some(ua) = &o.user_agent {
        if ua.trim().is_empty() {
            return Err("user_agent must not be empty".to_string());
        }
        config.fingerprint.user_agent = ua.clone();
        config.navigator.user_agent = ua.clone();
        config.navigator.app_version =
            crate::stealth::navigator::helpers::extract_app_version(ua);
    }

    if let Some(platform) = &o.platform {
        if platform.trim().is_empty() {
            return Err("platform must not be empty".to_string());
        }
        config.fingerprint.platform = platform.clone();
        config.navigator.platform = platform.clone();
    }

    if let Some(languages) = &o.languages {
        if languages.is_empty() || languages.iter().any(|l| l.trim().is_empty()) {
            return Err("languages must be a non-empty list of non-empty strings".to_string());
        }
        config.fingerprint.languages = languages.clone();
        config.fingerprint.language = languages[0].clone();
        config.navigator.languages = languages.clone();
    }

    if let Some(hc) = o.hardware_concurrency {
        if hc == 0 {
            return Err("hardware_concurrency must be >= 1".to_string());
        }
        config.navigator.hardware_concurrency = hc;
    }

    if let Some(dm) = o.device_memory {
        if dm == 0 {
            return Err("device_memory must be >= 1 (GB)".to_string());
        }
        config.navigator.device_memory = dm;
    }

    if let Some(vendor) = &o.webgl_vendor {
        config.webgl.vendor = vendor.clone();
    }

    if let Some(renderer) = &o.webgl_renderer {
        config.webgl.renderer = renderer.clone();
    }

    if let Some(screen) = &o.screen {
        if screen.width == 0 || screen.height == 0 {
            return Err("screen.width and screen.height must be > 0".to_string());
        }
        config.fingerprint.screen_resolution = ScreenResolution::new(screen.width, screen.height);
    }

    if let Some(tz) = &o.timezone {
        if tz.trim().is_empty() {
            return Err("timezone must not be empty".to_string());
        }
        config.fingerprint.timezone = tz.clone();
        if let Some(offset) = timezone_offset_minutes(tz) {
            config.fingerprint.timezone_offset = offset;
        }
        // Unknown zones keep the base profile's offset (best effort).
    }

    Ok(())
}

/// UTC offset in minutes for common IANA timezones (same sign convention as
/// `FingerprintGenerator::get_timezone`: positive = east of UTC).
fn timezone_offset_minutes(tz: &str) -> Option<i32> {
    let offset = match tz {
        "America/New_York" => -300,
        "America/Toronto" => -300,
        "America/Chicago" => -360,
        "America/Denver" => -420,
        "America/Los_Angeles" => -480,
        "America/Sao_Paulo" => -180,
        "UTC" | "Etc/UTC" => 0,
        "Europe/London" | "Europe/Dublin" | "Europe/Lisbon" => 0,
        "Europe/Paris" | "Europe/Berlin" | "Europe/Madrid" | "Europe/Rome"
        | "Europe/Amsterdam" | "Europe/Vienna" | "Europe/Zurich" | "Europe/Brussels"
        | "Europe/Stockholm" | "Europe/Oslo" | "Europe/Copenhagen" | "Europe/Prague"
        | "Europe/Warsaw" | "Europe/Budapest" => 60,
        "Europe/Helsinki" | "Europe/Athens" | "Europe/Bucharest" | "Europe/Kyiv" => 120,
        "Europe/Moscow" | "Europe/Istanbul" => 180,
        "Asia/Dubai" => 240,
        "Asia/Kolkata" => 330,
        "Asia/Bangkok" => 420,
        "Asia/Shanghai" | "Asia/Singapore" | "Asia/Hong_Kong" | "Asia/Taipei" => 480,
        "Asia/Tokyo" | "Asia/Seoul" => 540,
        "Australia/Sydney" | "Australia/Melbourne" => 600,
        "Pacific/Auckland" => 720,
        _ => return None,
    };
    Some(offset)
}

// ============================================================================
// HTTP header + introspection helpers
// ============================================================================

/// Builds an `Accept-Language` header value matching `navigator.languages`.
///
/// Example: `["de-DE", "de", "en"]` -> `"de-DE,de;q=0.9,en;q=0.8"`.
pub fn accept_language_header(languages: &[String]) -> String {
    if languages.is_empty() {
        return "en-US,en;q=0.9".to_string();
    }
    let mut parts = Vec::with_capacity(languages.len());
    for (i, lang) in languages.iter().enumerate() {
        if i == 0 {
            parts.push(lang.clone());
        } else {
            // q decreases by 0.1 per entry, floored at 0.1
            let q = (10_i32 - i as i32).max(1) as f32 / 10.0;
            parts.push(format!("{};q={:.1}", lang, q));
        }
    }
    parts.join(",")
}

/// Serializes the externally visible identity of a [`StealthConfig`] as JSON.
///
/// Used by `GET /tabs/{id}/identity` and embedded in the `POST /tabs/new`
/// response, so agents can always inspect the active identity of a tab.
pub fn identity_summary(config: &StealthConfig) -> serde_json::Value {
    serde_json::json!({
        "user_agent": config.fingerprint.user_agent,
        "platform": config.fingerprint.platform,
        "vendor": config.fingerprint.vendor,
        "languages": config.fingerprint.languages,
        "accept_language": accept_language_header(&config.fingerprint.languages),
        "hardware_concurrency": config.navigator.hardware_concurrency,
        "device_memory": config.navigator.device_memory,
        "webgl": {
            "vendor": config.webgl.vendor,
            "renderer": config.webgl.renderer,
        },
        "screen": {
            "width": config.fingerprint.screen_resolution.width,
            "height": config.fingerprint.screen_resolution.height,
            "avail_width": config.fingerprint.screen_resolution.avail_width,
            "avail_height": config.fingerprint.screen_resolution.avail_height,
        },
        "timezone": config.fingerprint.timezone,
        "timezone_offset": config.fingerprint.timezone_offset,
        "webdriver": config.navigator.webdriver,
    })
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    const VIEWPORT: (u32, u32) = (1280, 720);

    #[test]
    fn test_default_identity_is_consistent_chrome() {
        let config = resolve_identity(None, VIEWPORT).expect("default identity");
        assert!(config.validate().is_ok());
        assert!(!config.navigator.webdriver);
        // random_chrome() only picks Chrome/Edge profiles -> UA must contain Chrome
        assert!(config.fingerprint.user_agent.contains("Chrome"));
        // navigator values derive from the same fingerprint (one identity)
        assert_eq!(config.navigator.user_agent, config.fingerprint.user_agent);
        assert_eq!(config.navigator.platform, config.fingerprint.platform);
        assert_eq!(config.navigator.languages, config.fingerprint.languages);
    }

    #[test]
    fn test_named_random() {
        let spec = IdentitySpec::Named("random".to_string());
        let config = resolve_identity(Some(&spec), VIEWPORT).expect("random identity");
        assert!(config.fingerprint.user_agent.contains("Chrome"));
    }

    #[test]
    fn test_named_consistent_is_deterministic() {
        let spec = IdentitySpec::Named("consistent:tab-seed-1".to_string());
        let c1 = resolve_identity(Some(&spec), VIEWPORT).expect("consistent identity");
        let c2 = resolve_identity(Some(&spec), VIEWPORT).expect("consistent identity");
        assert_eq!(c1.fingerprint.user_agent, c2.fingerprint.user_agent);
        assert_eq!(c1.fingerprint.platform, c2.fingerprint.platform);
        assert_eq!(c1.webgl.renderer, c2.webgl.renderer);
    }

    #[test]
    fn test_named_unknown_is_error() {
        let spec = IdentitySpec::Named("bogus".to_string());
        let err = resolve_identity(Some(&spec), VIEWPORT).unwrap_err();
        assert!(err.contains("bogus"));
    }

    #[test]
    fn test_explicit_overrides_applied() {
        let spec = IdentitySpec::Explicit(IdentityOverrides {
            user_agent: Some("Mozilla/5.0 (X11; Linux x86_64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/126.0.0.0 Safari/537.36".to_string()),
            platform: Some("Linux x86_64".to_string()),
            languages: Some(vec!["de-DE".to_string(), "de".to_string(), "en".to_string()]),
            hardware_concurrency: Some(16),
            device_memory: Some(32),
            webgl_vendor: Some("Google Inc. (NVIDIA)".to_string()),
            webgl_renderer: Some("ANGLE (NVIDIA, NVIDIA GeForce RTX 4070)".to_string()),
            screen: Some(ScreenSpec { width: 2560, height: 1440 }),
            timezone: Some("Europe/Berlin".to_string()),
            seed: None,
        });
        let config = resolve_identity(Some(&spec), VIEWPORT).expect("explicit identity");

        assert!(config.fingerprint.user_agent.contains("Chrome/126"));
        assert_eq!(config.navigator.user_agent, config.fingerprint.user_agent);
        assert_eq!(config.fingerprint.platform, "Linux x86_64");
        assert_eq!(config.navigator.platform, "Linux x86_64");
        assert_eq!(config.fingerprint.languages, vec!["de-DE", "de", "en"]);
        assert_eq!(config.fingerprint.language, "de-DE");
        assert_eq!(config.navigator.languages, vec!["de-DE", "de", "en"]);
        assert_eq!(config.navigator.hardware_concurrency, 16);
        assert_eq!(config.navigator.device_memory, 32);
        assert_eq!(config.webgl.vendor, "Google Inc. (NVIDIA)");
        assert_eq!(config.webgl.renderer, "ANGLE (NVIDIA, NVIDIA GeForce RTX 4070)");
        assert_eq!(config.fingerprint.screen_resolution.width, 2560);
        assert_eq!(config.fingerprint.screen_resolution.height, 1440);
        assert_eq!(config.fingerprint.timezone, "Europe/Berlin");
        assert_eq!(config.fingerprint.timezone_offset, 60);
        assert!(!config.navigator.webdriver);
    }

    #[test]
    fn test_partial_overrides_fill_rest_consistently() {
        let spec = IdentitySpec::Explicit(IdentityOverrides {
            languages: Some(vec!["fr-FR".to_string(), "fr".to_string()]),
            ..Default::default()
        });
        let config = resolve_identity(Some(&spec), VIEWPORT).expect("partial identity");

        assert_eq!(config.fingerprint.languages, vec!["fr-FR", "fr"]);
        // Everything else must still be a complete, consistent profile
        assert!(!config.fingerprint.user_agent.is_empty());
        assert!(!config.fingerprint.platform.is_empty());
        assert!(!config.webgl.renderer.is_empty());
        assert!(config.validate().is_ok());
    }

    #[test]
    fn test_explicit_with_seed_is_deterministic_for_unset_fields() {
        let make = || {
            IdentitySpec::Explicit(IdentityOverrides {
                seed: Some("base-seed".to_string()),
                languages: Some(vec!["en-GB".to_string(), "en".to_string()]),
                ..Default::default()
            })
        };
        let c1 = resolve_identity(Some(&make()), VIEWPORT).expect("seeded identity");
        let c2 = resolve_identity(Some(&make()), VIEWPORT).expect("seeded identity");
        assert_eq!(c1.fingerprint.user_agent, c2.fingerprint.user_agent);
        assert_eq!(c1.fingerprint.languages, vec!["en-GB", "en"]);
    }

    #[test]
    fn test_invalid_explicit_values_rejected() {
        let empty_langs = IdentitySpec::Explicit(IdentityOverrides {
            languages: Some(vec![]),
            ..Default::default()
        });
        assert!(resolve_identity(Some(&empty_langs), VIEWPORT).is_err());

        let zero_hc = IdentitySpec::Explicit(IdentityOverrides {
            hardware_concurrency: Some(0),
            ..Default::default()
        });
        assert!(resolve_identity(Some(&zero_hc), VIEWPORT).is_err());

        let empty_ua = IdentitySpec::Explicit(IdentityOverrides {
            user_agent: Some("  ".to_string()),
            ..Default::default()
        });
        assert!(resolve_identity(Some(&empty_ua), VIEWPORT).is_err());
    }

    #[test]
    fn test_accept_language_header_matches_languages() {
        let langs = vec!["de-DE".to_string(), "de".to_string(), "en".to_string()];
        assert_eq!(accept_language_header(&langs), "de-DE,de;q=0.9,en;q=0.8");

        let single = vec!["en-GB".to_string()];
        assert_eq!(accept_language_header(&single), "en-GB");

        assert_eq!(accept_language_header(&[]), "en-US,en;q=0.9");
    }

    #[test]
    fn test_identity_spec_deserializes_string_and_object() {
        let named: IdentitySpec = serde_json::from_str("\"consistent:abc\"").expect("string spec");
        assert!(matches!(named, IdentitySpec::Named(ref s) if s == "consistent:abc"));

        let explicit: IdentitySpec =
            serde_json::from_str(r#"{"languages": ["de-DE", "de"], "hardware_concurrency": 4}"#)
                .expect("object spec");
        match explicit {
            IdentitySpec::Explicit(o) => {
                assert_eq!(o.languages.as_deref(), Some(&["de-DE".to_string(), "de".to_string()][..]));
                assert_eq!(o.hardware_concurrency, Some(4));
            }
            _ => panic!("expected explicit identity"),
        }
    }

    #[test]
    fn test_identity_summary_contains_all_fields() {
        let config = resolve_identity(None, VIEWPORT).expect("default identity");
        let summary = identity_summary(&config);
        for key in [
            "user_agent",
            "platform",
            "languages",
            "accept_language",
            "hardware_concurrency",
            "device_memory",
            "webgl",
            "screen",
            "timezone",
            "timezone_offset",
            "webdriver",
        ] {
            assert!(summary.get(key).is_some(), "missing key: {}", key);
        }
        assert_eq!(summary["webdriver"], serde_json::Value::Bool(false));
    }

    #[test]
    fn test_timezone_offset_lookup() {
        assert_eq!(timezone_offset_minutes("Europe/Berlin"), Some(60));
        assert_eq!(timezone_offset_minutes("America/New_York"), Some(-300));
        assert_eq!(timezone_offset_minutes("Asia/Tokyo"), Some(540));
        assert_eq!(timezone_offset_minutes("Mars/Olympus_Mons"), None);
    }
}
