//! Stealth and Anti-Detection Module
//!
//! This module provides comprehensive browser fingerprint management and anti-detection
//! capabilities for the ki-browser-standalone project. It enables browsers to appear
//! as legitimate user browsers rather than automated instances.
//!
//! # Modules
//!
//! - `fingerprint` - Browser fingerprint generation and management
//! - `webgl` - WebGL fingerprint spoofing and canvas noise injection
//! - `navigator` - Navigator property overrides and webdriver detection prevention
//! - `webrtc` - WebRTC leak prevention to protect real IP addresses
//! - `canvas` - Canvas fingerprint protection with noise injection
//! - `audio` - AudioContext fingerprint spoofing
//!
//! # Security Considerations
//!
//! The most critical aspect of anti-detection is ensuring `navigator.webdriver` is NEVER
//! exposed as `true`. This module provides multiple layers of protection against detection.
//!
//! # Example
//!
//! ```rust,no_run
//! use ki_browser_standalone::stealth::{
//!     fingerprint::{FingerprintGenerator, FingerprintProfile},
//!     webgl::WebGLConfig,
//!     navigator::NavigatorOverrides,
//!     webrtc::WebRtcConfig,
//!     canvas::CanvasConfig,
//!     audio::AudioConfig,
//! };
//!
//! // Generate a consistent fingerprint
//! let generator = FingerprintGenerator::new();
//! let fingerprint = generator.generate_consistent("my-session-seed");
//!
//! // Get WebGL configuration
//! let webgl = WebGLConfig::nvidia_gtx_1080();
//!
//! // Get navigator overrides (webdriver is ALWAYS false)
//! let navigator = NavigatorOverrides::from_fingerprint(&fingerprint);
//!
//! // WebRTC leak prevention
//! let webrtc = WebRtcConfig::default();
//!
//! // Canvas fingerprint protection
//! let canvas = CanvasConfig::default();
//!
//! // AudioContext spoofing
//! let audio = AudioConfig::default();
//! ```

pub mod audio;
pub mod canvas;
pub mod fingerprint;
pub mod navigator;
pub mod webgl;
pub mod webrtc;

// Re-export commonly used types for convenience
pub use audio::AudioConfig;
pub use canvas::CanvasConfig;
pub use fingerprint::{BrowserFingerprint, FingerprintGenerator, FingerprintProfile};
pub use navigator::{MimeTypeInfo, NavigatorOverrides, PluginInfo};
pub use webgl::{WebGLConfig, WebGLProfile};
pub use webrtc::{WebRtcConfig, WebRtcIpPolicy};

/// Combined stealth configuration for easy setup
#[derive(Debug, Clone)]
pub struct StealthConfig {
    /// Browser fingerprint configuration
    pub fingerprint: BrowserFingerprint,
    /// WebGL spoofing configuration
    pub webgl: WebGLConfig,
    /// Navigator property overrides
    pub navigator: NavigatorOverrides,
    /// WebRTC leak prevention configuration
    pub webrtc: WebRtcConfig,
    /// Canvas fingerprint protection configuration
    pub canvas: CanvasConfig,
    /// AudioContext fingerprint spoofing configuration
    pub audio: AudioConfig,
}

impl StealthConfig {
    /// Create a new stealth configuration from a fingerprint profile
    pub fn from_profile(profile: FingerprintProfile) -> Self {
        let fingerprint = FingerprintGenerator::new().generate_from_profile(profile.clone());
        let webgl = WebGLConfig::for_profile(&profile);
        let navigator = NavigatorOverrides::from_fingerprint(&fingerprint);
        let webrtc = WebRtcConfig::default();
        let canvas = CanvasConfig::default();
        let audio = AudioConfig::default();

        Self {
            fingerprint,
            webgl,
            navigator,
            webrtc,
            canvas,
            audio,
        }
    }

    /// Create a randomized stealth configuration
    pub fn random() -> Self {
        let fingerprint = FingerprintGenerator::new().generate_random();
        let webgl = WebGLConfig::random();
        let navigator = NavigatorOverrides::from_fingerprint(&fingerprint);
        let webrtc = WebRtcConfig::default();
        let canvas = CanvasConfig::default();
        let audio = AudioConfig::default();

        Self {
            fingerprint,
            webgl,
            navigator,
            webrtc,
            canvas,
            audio,
        }
    }

    /// Create a randomized stealth configuration restricted to Chrome-compatible profiles.
    ///
    /// Use this for Chromium-based engines where Safari/Firefox profiles would cause
    /// detectable mismatches (e.g. Safari UA on a Chrome browser).
    pub fn random_chrome() -> Self {
        use std::time::{SystemTime, UNIX_EPOCH};
        let seed = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos() as u64;

        let chrome_profiles = [FingerprintProfile::WindowsChrome,
            FingerprintProfile::MacChrome,
            FingerprintProfile::LinuxChrome,
            FingerprintProfile::WindowsEdge];
        let profile = chrome_profiles[seed as usize % chrome_profiles.len()].clone();

        Self::from_profile(profile)
    }

    /// Create a consistent stealth configuration based on a seed
    pub fn consistent(seed: &str) -> Self {
        let fingerprint = FingerprintGenerator::new().generate_consistent(seed);
        let webgl = WebGLConfig::consistent(seed);
        let navigator = NavigatorOverrides::from_fingerprint(&fingerprint);
        let webrtc = WebRtcConfig::default();
        let canvas = CanvasConfig::default();
        let audio = AudioConfig::default();

        Self {
            fingerprint,
            webgl,
            navigator,
            webrtc,
            canvas,
            audio,
        }
    }

    /// Synchronize the fingerprint's screen resolution to match the actual browser viewport.
    ///
    /// This ensures consistency between screen dimensions, outerWidth/Height, innerWidth/Height,
    /// and screen orientation. Must be called after construction with the actual viewport size
    /// (from BrowserConfig::window_size).
    ///
    /// Guarantees: screen.width >= outerWidth >= viewport_width (innerWidth)
    ///             screen.height >= outerHeight >= viewport_height (innerHeight)
    pub fn sync_screen_to_viewport(&mut self, viewport_width: u32, viewport_height: u32) {
        self.fingerprint
            .sync_screen_to_viewport(viewport_width, viewport_height);
    }

    /// Generate the complete JavaScript override script
    ///
    /// This script should be injected before any page scripts run.
    /// It provides comprehensive protection against fingerprinting and detection.
    pub fn get_complete_override_script(&self) -> String {
        let mut script = String::new();

        // Add IIFE wrapper for isolation
        script.push_str("(function() {\n'use strict';\n\n");

        // Navigator overrides (MOST CRITICAL - must run first)
        // Not wrapped in try/catch because a failure here is fatal
        script.push_str("// === NAVIGATOR OVERRIDES (CRITICAL) ===\n");
        script.push_str(&self.navigator.get_override_script());
        script.push_str("\n\n");

        // Each subsequent section is wrapped in try/catch so that a failure
        // in one section (e.g. WebGL not available at document-creation time)
        // does not prevent the remaining sections from executing.

        // WebGL overrides
        script.push_str("// === WEBGL OVERRIDES ===\n");
        script.push_str("try {\n");
        script.push_str(&self.webgl.get_js_override_script());
        script.push_str("\n} catch(e) {}\n\n");

        // Fingerprint overrides
        script.push_str("// === FINGERPRINT OVERRIDES ===\n");
        script.push_str("try {\n");
        script.push_str(&self.fingerprint.to_js_overrides());
        script.push_str("\n} catch(e) {}\n\n");

        // WebRTC leak prevention
        script.push_str("// === WEBRTC LEAK PREVENTION ===\n");
        script.push_str("try {\n");
        script.push_str(&self.webrtc.get_override_script());
        script.push_str("\n} catch(e) {}\n\n");

        // Canvas fingerprint protection
        script.push_str("// === CANVAS FINGERPRINT PROTECTION ===\n");
        script.push_str("try {\n");
        script.push_str(&self.canvas.get_override_script());
        script.push_str("\n} catch(e) {}\n\n");

        // AudioContext fingerprint spoofing
        script.push_str("// === AUDIO FINGERPRINT SPOOFING ===\n");
        script.push_str("try {\n");
        script.push_str(&self.audio.get_override_script());
        script.push_str("\n} catch(e) {}\n\n");

        // Close IIFE
        script.push_str("})();\n");

        script
    }

    /// Returns each stealth section as a separate script string.
    ///
    /// Each script is self-contained (wrapped in an IIFE with try/catch) so it
    /// can be injected independently via `evaluate_on_new_document`.  This
    /// prevents a failure in one section (e.g. WebGL not available at document
    /// creation time) from blocking other sections.
    pub fn get_section_scripts(&self) -> Vec<String> {
        let mut sections = Vec::new();

        // Navigator overrides (MOST CRITICAL - must run first)
        sections.push(format!(
            "(function() {{ 'use strict';\ntry {{\n{}\n}} catch(e) {{}}\n}})();",
            self.navigator.get_override_script()
        ));

        // WebGL overrides
        sections.push(format!(
            "(function() {{ 'use strict';\ntry {{\n{}\n}} catch(e) {{}}\n}})();",
            self.webgl.get_js_override_script()
        ));

        // Fingerprint overrides
        sections.push(format!(
            "(function() {{ 'use strict';\ntry {{\n{}\n}} catch(e) {{}}\n}})();",
            self.fingerprint.to_js_overrides()
        ));

        // WebRTC leak prevention
        sections.push(format!(
            "(function() {{ 'use strict';\ntry {{\n{}\n}} catch(e) {{}}\n}})();",
            self.webrtc.get_override_script()
        ));

        // Canvas fingerprint protection
        sections.push(format!(
            "(function() {{ 'use strict';\ntry {{\n{}\n}} catch(e) {{}}\n}})();",
            self.canvas.get_override_script()
        ));

        // AudioContext fingerprint spoofing
        sections.push(format!(
            "(function() {{ 'use strict';\ntry {{\n{}\n}} catch(e) {{}}\n}})();",
            self.audio.get_override_script()
        ));

        sections
    }

    /// Verify that the configuration is safe for use
    ///
    /// Returns an error if any critical anti-detection measures are misconfigured.
    pub fn validate(&self) -> Result<(), String> {
        // CRITICAL: webdriver must NEVER be true
        if self.navigator.webdriver {
            return Err(
                "CRITICAL: navigator.webdriver is set to true! This MUST be false.".to_string(),
            );
        }

        // Validate fingerprint consistency
        if self.fingerprint.user_agent.is_empty() {
            return Err("User agent cannot be empty".to_string());
        }

        if self.fingerprint.platform.is_empty() {
            return Err("Platform cannot be empty".to_string());
        }

        Ok(())
    }
}

impl Default for StealthConfig {
    fn default() -> Self {
        Self::from_profile(FingerprintProfile::WindowsChrome)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_stealth_config_validation() {
        let config = StealthConfig::default();
        assert!(config.validate().is_ok());
        assert!(!config.navigator.webdriver, "webdriver must NEVER be true");
    }

    #[test]
    fn test_random_config_is_valid() {
        for _ in 0..10 {
            let config = StealthConfig::random();
            assert!(config.validate().is_ok());
            assert!(!config.navigator.webdriver);
        }
    }

    #[test]
    fn test_consistent_config_is_deterministic() {
        let seed = "test-seed-123";
        let config1 = StealthConfig::consistent(seed);
        let config2 = StealthConfig::consistent(seed);

        assert_eq!(config1.fingerprint.user_agent, config2.fingerprint.user_agent);
        assert_eq!(config1.fingerprint.platform, config2.fingerprint.platform);
    }

    #[test]
    fn test_default_config_has_webrtc_protection() {
        let config = StealthConfig::default();
        assert!(!config.webrtc.disabled);
        assert_eq!(
            config.webrtc.ip_handling_policy,
            WebRtcIpPolicy::DisableNonProxiedUdp
        );
    }

    #[test]
    fn test_default_config_has_canvas_protection() {
        let config = StealthConfig::default();
        assert!(config.canvas.noise_enabled);
        assert!(config.canvas.protect_to_data_url);
        assert!(config.canvas.protect_to_blob);
        assert!(config.canvas.protect_get_image_data);
    }

    #[test]
    fn test_default_config_has_audio_protection() {
        let config = StealthConfig::default();
        assert!(config.audio.enabled);
        assert!(config.audio.noise_level > 0.0);
    }

    #[test]
    fn test_complete_script_contains_all_overrides() {
        let config = StealthConfig::default();
        let script = config.get_complete_override_script();

        // Navigator overrides (critical)
        assert!(script.contains("NAVIGATOR OVERRIDES"));
        assert!(script.contains("webdriver"));

        // WebGL overrides
        assert!(script.contains("WEBGL OVERRIDES"));

        // Fingerprint overrides
        assert!(script.contains("FINGERPRINT OVERRIDES"));

        // WebRTC leak prevention
        assert!(script.contains("WEBRTC LEAK PREVENTION"));
        assert!(script.contains("RTCPeerConnection"));

        // Canvas fingerprint protection
        assert!(script.contains("CANVAS FINGERPRINT PROTECTION"));
        assert!(script.contains("toDataURL"));

        // Audio fingerprint spoofing
        assert!(script.contains("AUDIO FINGERPRINT SPOOFING"));
        assert!(script.contains("AudioContext") || script.contains("AudioBuffer"));
    }

    #[test]
    fn test_complete_script_is_wrapped_in_iife() {
        let config = StealthConfig::default();
        let script = config.get_complete_override_script();

        assert!(script.starts_with("(function() {\n'use strict';\n"));
        assert!(script.trim_end().ends_with("})();"));
    }

    #[test]
    fn test_random_config_has_all_modules() {
        let config = StealthConfig::random();

        // All modules should be present with safe defaults
        assert!(!config.webrtc.disabled);
        assert!(config.canvas.noise_enabled);
        assert!(config.audio.enabled);
        assert!(!config.navigator.webdriver);
    }

    #[test]
    fn test_consistent_config_has_all_modules() {
        let config = StealthConfig::consistent("test-seed");

        assert!(!config.webrtc.disabled);
        assert!(config.canvas.noise_enabled);
        assert!(config.audio.enabled);
        assert!(!config.navigator.webdriver);
    }
}
