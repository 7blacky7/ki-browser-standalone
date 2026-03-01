//! Builder pattern for constructing custom `NavigatorOverrides`.
//!
//! Provides a fluent API to configure navigator property overrides
//! for anti-detection fingerprint spoofing. The builder guarantees
//! that `webdriver` is always forced to `false` on `build()`.

use super::helpers::extract_app_version;
use super::types::{NavigatorOverrides, PluginInfo};

/// Builder for creating custom NavigatorOverrides with validated fields
#[derive(Debug, Clone)]
pub struct NavigatorOverridesBuilder {
    overrides: NavigatorOverrides,
}

impl NavigatorOverridesBuilder {
    /// Create a new builder with default values
    pub fn new() -> Self {
        Self {
            overrides: NavigatorOverrides::default(),
        }
    }

    /// Set accepted languages (e.g., vec!["en-US", "en"])
    pub fn languages(mut self, languages: Vec<String>) -> Self {
        self.overrides.languages = languages;
        self
    }

    /// Set platform string (e.g., "Win32", "MacIntel", "Linux x86_64")
    pub fn platform(mut self, platform: impl Into<String>) -> Self {
        self.overrides.platform = platform.into();
        self
    }

    /// Set number of logical CPU cores
    pub fn hardware_concurrency(mut self, cores: u8) -> Self {
        self.overrides.hardware_concurrency = cores;
        self
    }

    /// Set device memory in GB, validated to power of 2 (2, 4, 8, 16, 32)
    pub fn device_memory(mut self, memory_gb: u8) -> Self {
        let valid_values = [2, 4, 8, 16, 32];
        self.overrides.device_memory = if valid_values.contains(&memory_gb) {
            memory_gb
        } else {
            8 // Default to 8GB
        };
        self
    }

    /// Set maximum touch points (0 for non-touch desktop devices)
    pub fn max_touch_points(mut self, points: u8) -> Self {
        self.overrides.max_touch_points = points;
        self
    }

    /// Set user agent string (also updates app_version automatically)
    pub fn user_agent(mut self, user_agent: impl Into<String>) -> Self {
        let ua: String = user_agent.into();
        self.overrides.app_version = extract_app_version(&ua);
        self.overrides.user_agent = ua;
        self
    }

    /// Set vendor string (e.g., "Google Inc.")
    pub fn vendor(mut self, vendor: impl Into<String>) -> Self {
        self.overrides.vendor = vendor.into();
        self
    }

    /// Set browser plugins list for navigator.plugins spoofing
    pub fn plugins(mut self, plugins: Vec<PluginInfo>) -> Self {
        self.overrides.plugins = plugins;
        self
    }

    /// Enable or disable Permissions API spoofing
    pub fn spoof_permissions(mut self, enabled: bool) -> Self {
        self.overrides.spoof_permissions = enabled;
        self
    }

    /// Enable or disable automation signal removal (CDP, Selenium, PhantomJS, etc.)
    pub fn remove_automation_signals(mut self, enabled: bool) -> Self {
        self.overrides.remove_automation_signals = enabled;
        self
    }

    /// Build the final NavigatorOverrides.
    ///
    /// CRITICAL: `webdriver` will ALWAYS be forced to `false` regardless
    /// of any other settings to prevent automation detection.
    pub fn build(mut self) -> NavigatorOverrides {
        // CRITICAL: Force webdriver to false
        self.overrides.webdriver = false;
        self.overrides
    }
}

impl Default for NavigatorOverridesBuilder {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_builder_forces_webdriver_false() {
        let overrides = NavigatorOverridesBuilder::new().build();
        assert!(!overrides.webdriver, "webdriver MUST be false after build");
    }

    #[test]
    fn test_device_memory_validation() {
        let overrides = NavigatorOverridesBuilder::new()
            .device_memory(5) // Invalid, should default to 8
            .build();
        assert_eq!(overrides.device_memory, 8);

        let overrides = NavigatorOverridesBuilder::new()
            .device_memory(16) // Valid
            .build();
        assert_eq!(overrides.device_memory, 16);
    }
}
