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
//! ```

pub mod fingerprint;
pub mod navigator;
pub mod webgl;

// Re-export commonly used types for convenience
pub use fingerprint::{BrowserFingerprint, FingerprintGenerator, FingerprintProfile};
pub use navigator::{MimeTypeInfo, NavigatorOverrides, PluginInfo};
pub use webgl::{WebGLConfig, WebGLProfile};

/// Combined stealth configuration for easy setup
#[derive(Debug, Clone)]
pub struct StealthConfig {
    /// Browser fingerprint configuration
    pub fingerprint: BrowserFingerprint,
    /// WebGL spoofing configuration
    pub webgl: WebGLConfig,
    /// Navigator property overrides
    pub navigator: NavigatorOverrides,
}

impl StealthConfig {
    /// Create a new stealth configuration from a fingerprint profile
    pub fn from_profile(profile: FingerprintProfile) -> Self {
        let fingerprint = FingerprintGenerator::new().generate_from_profile(profile.clone());
        let webgl = WebGLConfig::for_profile(&profile);
        let navigator = NavigatorOverrides::from_fingerprint(&fingerprint);

        Self {
            fingerprint,
            webgl,
            navigator,
        }
    }

    /// Create a randomized stealth configuration
    pub fn random() -> Self {
        let fingerprint = FingerprintGenerator::new().generate_random();
        let webgl = WebGLConfig::random();
        let navigator = NavigatorOverrides::from_fingerprint(&fingerprint);

        Self {
            fingerprint,
            webgl,
            navigator,
        }
    }

    /// Create a consistent stealth configuration based on a seed
    pub fn consistent(seed: &str) -> Self {
        let fingerprint = FingerprintGenerator::new().generate_consistent(seed);
        let webgl = WebGLConfig::consistent(seed);
        let navigator = NavigatorOverrides::from_fingerprint(&fingerprint);

        Self {
            fingerprint,
            webgl,
            navigator,
        }
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
        script.push_str("// === NAVIGATOR OVERRIDES (CRITICAL) ===\n");
        script.push_str(&self.navigator.get_override_script());
        script.push_str("\n\n");

        // WebGL overrides
        script.push_str("// === WEBGL OVERRIDES ===\n");
        script.push_str(&self.webgl.get_js_override_script());
        script.push_str("\n\n");

        // Fingerprint overrides
        script.push_str("// === FINGERPRINT OVERRIDES ===\n");
        script.push_str(&self.fingerprint.to_js_overrides());
        script.push_str("\n\n");

        // Close IIFE
        script.push_str("})();\n");

        script
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
}
