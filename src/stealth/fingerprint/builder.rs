//! Builder pattern for creating custom browser fingerprints.
//!
//! Provides [`FingerprintBuilder`] which allows step-by-step construction
//! of a [`BrowserFingerprint`] with custom user agent, platform, language,
//! screen resolution, timezone, and Do Not Track settings.

use super::fingerprint::BrowserFingerprint;
use super::generator::FingerprintGenerator;
use super::types::{FingerprintProfile, ScreenResolution};

/// Builder for creating custom fingerprints
#[derive(Debug, Clone)]
pub struct FingerprintBuilder {
    fingerprint: BrowserFingerprint,
}

impl FingerprintBuilder {
    /// Create a new builder with default Windows Chrome profile
    pub fn new() -> Self {
        let generator = FingerprintGenerator::new();
        Self {
            fingerprint: generator.generate_from_profile(FingerprintProfile::WindowsChrome),
        }
    }

    /// Start from an existing fingerprint
    pub fn from_fingerprint(fingerprint: BrowserFingerprint) -> Self {
        Self { fingerprint }
    }

    /// Set the user agent
    pub fn user_agent(mut self, user_agent: impl Into<String>) -> Self {
        self.fingerprint.user_agent = user_agent.into();
        self
    }

    /// Set the platform
    pub fn platform(mut self, platform: impl Into<String>) -> Self {
        self.fingerprint.platform = platform.into();
        self
    }

    /// Set the vendor
    pub fn vendor(mut self, vendor: impl Into<String>) -> Self {
        self.fingerprint.vendor = vendor.into();
        self
    }

    /// Set the primary language
    pub fn language(mut self, language: impl Into<String>) -> Self {
        self.fingerprint.language = language.into();
        self
    }

    /// Set all languages
    pub fn languages(mut self, languages: Vec<String>) -> Self {
        if !languages.is_empty() {
            self.fingerprint.language = languages[0].clone();
        }
        self.fingerprint.languages = languages;
        self
    }

    /// Set screen resolution
    pub fn screen_resolution(mut self, width: u32, height: u32) -> Self {
        self.fingerprint.screen_resolution = ScreenResolution::new(width, height);
        self
    }

    /// Set color depth
    pub fn color_depth(mut self, depth: u8) -> Self {
        self.fingerprint.color_depth = depth;
        self.fingerprint.pixel_depth = depth;
        self
    }

    /// Set timezone
    pub fn timezone(mut self, timezone: impl Into<String>, offset: i32) -> Self {
        self.fingerprint.timezone = timezone.into();
        self.fingerprint.timezone_offset = offset;
        self
    }

    /// Set Do Not Track preference
    pub fn do_not_track(mut self, dnt: Option<String>) -> Self {
        self.fingerprint.do_not_track = dnt;
        self
    }

    /// Build the final fingerprint
    pub fn build(self) -> BrowserFingerprint {
        self.fingerprint
    }
}

impl Default for FingerprintBuilder {
    fn default() -> Self {
        Self::new()
    }
}
