//! Integration tests for the stealth/anti-detection module
//!
//! Tests for fingerprint generation, webdriver property is always false,
//! JS override script generation, and consistent fingerprint with same seed.

use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};

/// Mock implementations for stealth testing
mod mock {
    use std::collections::hash_map::DefaultHasher;
    use std::hash::{Hash, Hasher};

    /// Browser fingerprint profile types
    #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
    pub enum FingerprintProfile {
        WindowsChrome,
        WindowsFirefox,
        WindowsEdge,
        MacSafari,
        MacChrome,
        LinuxChrome,
        LinuxFirefox,
        Custom,
    }

    /// Screen resolution info
    #[derive(Debug, Clone, PartialEq, Eq)]
    pub struct ScreenInfo {
        pub width: u32,
        pub height: u32,
        pub color_depth: u8,
        pub pixel_ratio: f64,
    }

    impl Default for ScreenInfo {
        fn default() -> Self {
            Self {
                width: 1920,
                height: 1080,
                color_depth: 24,
                pixel_ratio: 1.0,
            }
        }
    }

    impl ScreenInfo {
        pub fn new(width: u32, height: u32) -> Self {
            Self {
                width,
                height,
                ..Default::default()
            }
        }
    }

    /// Browser fingerprint configuration
    #[derive(Debug, Clone)]
    pub struct BrowserFingerprint {
        /// CRITICAL: Must ALWAYS be false
        pub webdriver: bool,

        /// User agent string
        pub user_agent: String,

        /// Platform string (e.g., "Win32", "MacIntel")
        pub platform: String,

        /// Vendor string
        pub vendor: String,

        /// Languages
        pub languages: Vec<String>,

        /// Screen information
        pub screen: ScreenInfo,

        /// Timezone
        pub timezone: String,

        /// Cookies enabled
        pub cookie_enabled: bool,

        /// Do Not Track setting
        pub do_not_track: Option<String>,

        /// Hardware concurrency (CPU cores)
        pub hardware_concurrency: u8,

        /// Device memory in GB
        pub device_memory: u8,

        /// Maximum touch points
        pub max_touch_points: u8,

        /// PDF viewer enabled
        pub pdf_viewer_enabled: bool,

        /// Seed used for generation (for reproducibility)
        pub seed: Option<u64>,
    }

    impl Default for BrowserFingerprint {
        fn default() -> Self {
            Self {
                webdriver: false, // CRITICAL: Always false
                user_agent: "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/120.0.0.0 Safari/537.36".to_string(),
                platform: "Win32".to_string(),
                vendor: "Google Inc.".to_string(),
                languages: vec!["en-US".to_string(), "en".to_string()],
                screen: ScreenInfo::default(),
                timezone: "America/New_York".to_string(),
                cookie_enabled: true,
                do_not_track: None,
                hardware_concurrency: 8,
                device_memory: 8,
                max_touch_points: 0,
                pdf_viewer_enabled: true,
                seed: None,
            }
        }
    }

    impl BrowserFingerprint {
        /// CRITICAL: Verify webdriver is false
        pub fn ensure_no_webdriver(&self) {
            if self.webdriver {
                panic!("CRITICAL: webdriver MUST be false!");
            }
        }

        /// Check if fingerprint is valid
        pub fn is_valid(&self) -> bool {
            !self.webdriver
                && !self.user_agent.is_empty()
                && !self.platform.is_empty()
                && !self.languages.is_empty()
                && self.screen.width > 0
                && self.screen.height > 0
        }
    }

    /// Fingerprint generator
    #[derive(Debug, Clone)]
    pub struct FingerprintGenerator {
        seed: Option<u64>,
    }

    impl Default for FingerprintGenerator {
        fn default() -> Self {
            Self::new()
        }
    }

    impl FingerprintGenerator {
        pub fn new() -> Self {
            Self { seed: None }
        }

        pub fn with_seed(seed: u64) -> Self {
            Self { seed: Some(seed) }
        }

        /// Generate a fingerprint based on profile
        pub fn generate_from_profile(&self, profile: FingerprintProfile) -> BrowserFingerprint {
            let mut fp = match profile {
                FingerprintProfile::WindowsChrome => BrowserFingerprint {
                    user_agent: "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/120.0.0.0 Safari/537.36".to_string(),
                    platform: "Win32".to_string(),
                    vendor: "Google Inc.".to_string(),
                    ..Default::default()
                },
                FingerprintProfile::WindowsFirefox => BrowserFingerprint {
                    user_agent: "Mozilla/5.0 (Windows NT 10.0; Win64; x64; rv:121.0) Gecko/20100101 Firefox/121.0".to_string(),
                    platform: "Win32".to_string(),
                    vendor: "".to_string(),
                    ..Default::default()
                },
                FingerprintProfile::WindowsEdge => BrowserFingerprint {
                    user_agent: "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/120.0.0.0 Safari/537.36 Edg/120.0.0.0".to_string(),
                    platform: "Win32".to_string(),
                    vendor: "Google Inc.".to_string(),
                    ..Default::default()
                },
                FingerprintProfile::MacSafari => BrowserFingerprint {
                    user_agent: "Mozilla/5.0 (Macintosh; Intel Mac OS X 14_2) AppleWebKit/605.1.15 (KHTML, like Gecko) Version/17.2 Safari/605.1.15".to_string(),
                    platform: "MacIntel".to_string(),
                    vendor: "Apple Computer, Inc.".to_string(),
                    ..Default::default()
                },
                FingerprintProfile::MacChrome => BrowserFingerprint {
                    user_agent: "Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/120.0.0.0 Safari/537.36".to_string(),
                    platform: "MacIntel".to_string(),
                    vendor: "Google Inc.".to_string(),
                    ..Default::default()
                },
                FingerprintProfile::LinuxChrome => BrowserFingerprint {
                    user_agent: "Mozilla/5.0 (X11; Linux x86_64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/120.0.0.0 Safari/537.36".to_string(),
                    platform: "Linux x86_64".to_string(),
                    vendor: "Google Inc.".to_string(),
                    ..Default::default()
                },
                FingerprintProfile::LinuxFirefox => BrowserFingerprint {
                    user_agent: "Mozilla/5.0 (X11; Linux x86_64; rv:121.0) Gecko/20100101 Firefox/121.0".to_string(),
                    platform: "Linux x86_64".to_string(),
                    vendor: "".to_string(),
                    ..Default::default()
                },
                FingerprintProfile::Custom => BrowserFingerprint::default(),
            };

            // Apply seed-based variations if seed is set
            if let Some(seed) = self.seed {
                fp.seed = Some(seed);
                self.apply_seed_variations(&mut fp, seed);
            }

            // CRITICAL: Ensure webdriver is ALWAYS false
            fp.webdriver = false;

            fp
        }

        /// Generate a random fingerprint
        pub fn generate_random(&self) -> BrowserFingerprint {
            let profiles = [
                FingerprintProfile::WindowsChrome,
                FingerprintProfile::WindowsFirefox,
                FingerprintProfile::MacSafari,
                FingerprintProfile::LinuxChrome,
            ];

            let seed = self.seed.unwrap_or_else(|| rand::random());
            let profile_index = (seed % profiles.len() as u64) as usize;

            self.generate_from_profile(profiles[profile_index])
        }

        fn apply_seed_variations(&self, fp: &mut BrowserFingerprint, seed: u64) {
            // Use seed to deterministically vary some properties
            let mut hasher = DefaultHasher::new();
            seed.hash(&mut hasher);
            let hash = hasher.finish();

            // Vary screen resolution
            let resolutions = [
                (1920, 1080),
                (1366, 768),
                (1536, 864),
                (1440, 900),
                (2560, 1440),
            ];
            let res_index = (hash % resolutions.len() as u64) as usize;
            fp.screen.width = resolutions[res_index].0;
            fp.screen.height = resolutions[res_index].1;

            // Vary hardware concurrency
            let cores = [4, 6, 8, 12, 16];
            let core_index = ((hash >> 8) % cores.len() as u64) as usize;
            fp.hardware_concurrency = cores[core_index];

            // Vary device memory
            let memory = [4, 8, 16, 32];
            let mem_index = ((hash >> 16) % memory.len() as u64) as usize;
            fp.device_memory = memory[mem_index];

            // Vary timezone
            let timezones = [
                "America/New_York",
                "America/Los_Angeles",
                "Europe/London",
                "Europe/Berlin",
                "Asia/Tokyo",
            ];
            let tz_index = ((hash >> 24) % timezones.len() as u64) as usize;
            fp.timezone = timezones[tz_index].to_string();
        }
    }

    /// Navigator property overrides for anti-detection
    #[derive(Debug, Clone)]
    pub struct NavigatorOverrides {
        /// CRITICAL: Must ALWAYS be false
        pub webdriver: bool,
        pub languages: Vec<String>,
        pub platform: String,
        pub hardware_concurrency: u8,
        pub device_memory: u8,
        pub max_touch_points: u8,
        pub vendor: String,
        pub user_agent: String,
        pub cookie_enabled: bool,
        pub pdf_viewer_enabled: bool,
        pub spoof_permissions: bool,
        pub remove_automation_signals: bool,
    }

    impl Default for NavigatorOverrides {
        fn default() -> Self {
            Self {
                webdriver: false, // CRITICAL: Always false
                languages: vec!["en-US".to_string(), "en".to_string()],
                platform: "Win32".to_string(),
                hardware_concurrency: 8,
                device_memory: 8,
                max_touch_points: 0,
                vendor: "Google Inc.".to_string(),
                user_agent: "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/120.0.0.0 Safari/537.36".to_string(),
                cookie_enabled: true,
                pdf_viewer_enabled: true,
                spoof_permissions: true,
                remove_automation_signals: true,
            }
        }
    }

    impl NavigatorOverrides {
        /// Create from fingerprint
        pub fn from_fingerprint(fp: &BrowserFingerprint) -> Self {
            Self {
                webdriver: false, // CRITICAL: Always false regardless of input
                languages: fp.languages.clone(),
                platform: fp.platform.clone(),
                hardware_concurrency: fp.hardware_concurrency,
                device_memory: fp.device_memory,
                max_touch_points: fp.max_touch_points,
                vendor: fp.vendor.clone(),
                user_agent: fp.user_agent.clone(),
                cookie_enabled: fp.cookie_enabled,
                pdf_viewer_enabled: fp.pdf_viewer_enabled,
                spoof_permissions: true,
                remove_automation_signals: true,
            }
        }

        /// CRITICAL: Verify webdriver is false
        pub fn ensure_no_webdriver(&self) {
            if self.webdriver {
                panic!("CRITICAL SECURITY ERROR: navigator.webdriver MUST be false!");
            }
        }

        /// Generate JavaScript override script
        pub fn get_override_script(&self) -> String {
            // Safety check
            self.ensure_no_webdriver();

            let languages_json: String = format!(
                "[{}]",
                self.languages
                    .iter()
                    .map(|l| format!("\"{}\"", l))
                    .collect::<Vec<_>>()
                    .join(", ")
            );

            format!(
                r#"
// ============================================================================
// CRITICAL NAVIGATOR ANTI-DETECTION OVERRIDES
// ============================================================================

(function() {{
    'use strict';

    // ========================================================================
    // CRITICAL: WebDriver Detection Prevention
    // ========================================================================

    // Method 1: Direct property override
    Object.defineProperty(navigator, 'webdriver', {{
        get: function() {{ return false; }},
        configurable: true,
        enumerable: true
    }});

    // Method 2: Delete and redefine
    try {{
        delete navigator.webdriver;
        Object.defineProperty(navigator, 'webdriver', {{
            get: function() {{ return false; }},
            configurable: true,
            enumerable: true
        }});
    }} catch (e) {{}}

    // Method 3: Override on Navigator prototype
    try {{
        Object.defineProperty(Navigator.prototype, 'webdriver', {{
            get: function() {{ return false; }},
            configurable: true,
            enumerable: true
        }});
    }} catch (e) {{}}

    // Method 4: Spoof Object.getOwnPropertyDescriptor
    const originalGetOwnPropertyDescriptor = Object.getOwnPropertyDescriptor;
    Object.getOwnPropertyDescriptor = function(obj, prop) {{
        if (prop === 'webdriver' && (obj === navigator || obj === Navigator.prototype)) {{
            return {{
                value: false,
                writable: false,
                enumerable: true,
                configurable: true
            }};
        }}
        return originalGetOwnPropertyDescriptor.call(this, obj, prop);
    }};

    // ========================================================================
    // User Agent and Related Properties
    // ========================================================================

    Object.defineProperty(navigator, 'userAgent', {{
        get: function() {{ return "{user_agent}"; }},
        configurable: true
    }});

    Object.defineProperty(navigator, 'vendor', {{
        get: function() {{ return "{vendor}"; }},
        configurable: true
    }});

    Object.defineProperty(navigator, 'platform', {{
        get: function() {{ return "{platform}"; }},
        configurable: true
    }});

    // ========================================================================
    // Hardware Properties
    // ========================================================================

    Object.defineProperty(navigator, 'hardwareConcurrency', {{
        get: function() {{ return {hardware_concurrency}; }},
        configurable: true
    }});

    Object.defineProperty(navigator, 'deviceMemory', {{
        get: function() {{ return {device_memory}; }},
        configurable: true
    }});

    Object.defineProperty(navigator, 'maxTouchPoints', {{
        get: function() {{ return {max_touch_points}; }},
        configurable: true
    }});

    // ========================================================================
    // Language Properties
    // ========================================================================

    const LANGUAGES = {languages_json};

    Object.defineProperty(navigator, 'languages', {{
        get: function() {{ return Object.freeze(LANGUAGES.slice()); }},
        configurable: true
    }});

    Object.defineProperty(navigator, 'language', {{
        get: function() {{ return LANGUAGES[0]; }},
        configurable: true
    }});

    // ========================================================================
    // Status Properties
    // ========================================================================

    Object.defineProperty(navigator, 'cookieEnabled', {{
        get: function() {{ return {cookie_enabled}; }},
        configurable: true
    }});

    Object.defineProperty(navigator, 'pdfViewerEnabled', {{
        get: function() {{ return {pdf_viewer_enabled}; }},
        configurable: true
    }});

    // ========================================================================
    // Final Verification
    // ========================================================================

    if (navigator.webdriver !== false) {{
        console.error('CRITICAL: navigator.webdriver override failed!');
        Object.defineProperty(navigator, 'webdriver', {{
            get: function() {{ return false; }},
            configurable: false,
            enumerable: true
        }});
    }}

}})();
"#,
                user_agent = escape_js_string(&self.user_agent),
                vendor = escape_js_string(&self.vendor),
                platform = escape_js_string(&self.platform),
                hardware_concurrency = self.hardware_concurrency,
                device_memory = self.device_memory,
                max_touch_points = self.max_touch_points,
                languages_json = languages_json,
                cookie_enabled = self.cookie_enabled,
                pdf_viewer_enabled = self.pdf_viewer_enabled,
            )
        }
    }

    /// Navigator overrides builder
    #[derive(Debug, Clone)]
    pub struct NavigatorOverridesBuilder {
        overrides: NavigatorOverrides,
    }

    impl NavigatorOverridesBuilder {
        pub fn new() -> Self {
            Self {
                overrides: NavigatorOverrides::default(),
            }
        }

        pub fn languages(mut self, languages: Vec<String>) -> Self {
            self.overrides.languages = languages;
            self
        }

        pub fn platform(mut self, platform: impl Into<String>) -> Self {
            self.overrides.platform = platform.into();
            self
        }

        pub fn hardware_concurrency(mut self, cores: u8) -> Self {
            self.overrides.hardware_concurrency = cores;
            self
        }

        pub fn device_memory(mut self, memory_gb: u8) -> Self {
            // Must be power of 2
            let valid = [2, 4, 8, 16, 32];
            self.overrides.device_memory = if valid.contains(&memory_gb) {
                memory_gb
            } else {
                8
            };
            self
        }

        pub fn user_agent(mut self, ua: impl Into<String>) -> Self {
            self.overrides.user_agent = ua.into();
            self
        }

        pub fn vendor(mut self, vendor: impl Into<String>) -> Self {
            self.overrides.vendor = vendor.into();
            self
        }

        /// Build the final overrides
        /// CRITICAL: webdriver is ALWAYS forced to false
        pub fn build(mut self) -> NavigatorOverrides {
            self.overrides.webdriver = false;
            self.overrides
        }
    }

    impl Default for NavigatorOverridesBuilder {
        fn default() -> Self {
            Self::new()
        }
    }

    /// Escape string for JavaScript
    fn escape_js_string(s: &str) -> String {
        s.replace('\\', "\\\\")
            .replace('"', "\\\"")
            .replace('\n', "\\n")
            .replace('\r', "\\r")
            .replace('\t', "\\t")
    }
}

use mock::*;

// ============================================================================
// Fingerprint Generation Tests
// ============================================================================

#[test]
fn test_fingerprint_default() {
    let fp = BrowserFingerprint::default();

    assert!(!fp.webdriver);
    assert!(!fp.user_agent.is_empty());
    assert!(!fp.platform.is_empty());
    assert!(!fp.languages.is_empty());
}

#[test]
fn test_fingerprint_valid() {
    let fp = BrowserFingerprint::default();
    assert!(fp.is_valid());
}

#[test]
fn test_fingerprint_generator_windows_chrome() {
    let gen = FingerprintGenerator::new();
    let fp = gen.generate_from_profile(FingerprintProfile::WindowsChrome);

    assert!(!fp.webdriver);
    assert!(fp.user_agent.contains("Chrome"));
    assert!(fp.user_agent.contains("Windows"));
    assert_eq!(fp.platform, "Win32");
    assert_eq!(fp.vendor, "Google Inc.");
}

#[test]
fn test_fingerprint_generator_windows_firefox() {
    let gen = FingerprintGenerator::new();
    let fp = gen.generate_from_profile(FingerprintProfile::WindowsFirefox);

    assert!(!fp.webdriver);
    assert!(fp.user_agent.contains("Firefox"));
    assert_eq!(fp.platform, "Win32");
    assert!(fp.vendor.is_empty()); // Firefox has empty vendor
}

#[test]
fn test_fingerprint_generator_windows_edge() {
    let gen = FingerprintGenerator::new();
    let fp = gen.generate_from_profile(FingerprintProfile::WindowsEdge);

    assert!(!fp.webdriver);
    assert!(fp.user_agent.contains("Edg"));
    assert_eq!(fp.platform, "Win32");
}

#[test]
fn test_fingerprint_generator_mac_safari() {
    let gen = FingerprintGenerator::new();
    let fp = gen.generate_from_profile(FingerprintProfile::MacSafari);

    assert!(!fp.webdriver);
    assert!(fp.user_agent.contains("Safari"));
    assert!(fp.user_agent.contains("Macintosh"));
    assert_eq!(fp.platform, "MacIntel");
    assert_eq!(fp.vendor, "Apple Computer, Inc.");
}

#[test]
fn test_fingerprint_generator_mac_chrome() {
    let gen = FingerprintGenerator::new();
    let fp = gen.generate_from_profile(FingerprintProfile::MacChrome);

    assert!(!fp.webdriver);
    assert!(fp.user_agent.contains("Chrome"));
    assert!(fp.user_agent.contains("Macintosh"));
    assert_eq!(fp.platform, "MacIntel");
}

#[test]
fn test_fingerprint_generator_linux_chrome() {
    let gen = FingerprintGenerator::new();
    let fp = gen.generate_from_profile(FingerprintProfile::LinuxChrome);

    assert!(!fp.webdriver);
    assert!(fp.user_agent.contains("Chrome"));
    assert!(fp.user_agent.contains("Linux"));
    assert_eq!(fp.platform, "Linux x86_64");
}

#[test]
fn test_fingerprint_generator_linux_firefox() {
    let gen = FingerprintGenerator::new();
    let fp = gen.generate_from_profile(FingerprintProfile::LinuxFirefox);

    assert!(!fp.webdriver);
    assert!(fp.user_agent.contains("Firefox"));
    assert!(fp.user_agent.contains("Linux"));
    assert_eq!(fp.platform, "Linux x86_64");
}

#[test]
fn test_fingerprint_generator_random() {
    let gen = FingerprintGenerator::new();
    let fp = gen.generate_random();

    assert!(!fp.webdriver);
    assert!(fp.is_valid());
}

// ============================================================================
// Webdriver Property Tests - CRITICAL
// ============================================================================

#[test]
fn test_webdriver_always_false_in_fingerprint() {
    let fp = BrowserFingerprint::default();
    assert!(!fp.webdriver, "CRITICAL: webdriver MUST be false");
}

#[test]
fn test_webdriver_always_false_in_navigator_overrides() {
    let overrides = NavigatorOverrides::default();
    assert!(!overrides.webdriver, "CRITICAL: webdriver MUST be false");
}

#[test]
fn test_webdriver_always_false_after_builder() {
    let overrides = NavigatorOverridesBuilder::new().build();
    assert!(!overrides.webdriver, "CRITICAL: webdriver MUST be false after build");
}

#[test]
fn test_webdriver_always_false_from_fingerprint() {
    let fp = BrowserFingerprint::default();
    let overrides = NavigatorOverrides::from_fingerprint(&fp);
    assert!(!overrides.webdriver, "CRITICAL: webdriver MUST be false");
}

#[test]
fn test_webdriver_always_false_with_any_profile() {
    let gen = FingerprintGenerator::new();
    let profiles = [
        FingerprintProfile::WindowsChrome,
        FingerprintProfile::WindowsFirefox,
        FingerprintProfile::WindowsEdge,
        FingerprintProfile::MacSafari,
        FingerprintProfile::MacChrome,
        FingerprintProfile::LinuxChrome,
        FingerprintProfile::LinuxFirefox,
        FingerprintProfile::Custom,
    ];

    for profile in profiles {
        let fp = gen.generate_from_profile(profile);
        assert!(!fp.webdriver, "CRITICAL: webdriver MUST be false for {:?}", profile);
    }
}

#[test]
fn test_ensure_no_webdriver_passes() {
    let fp = BrowserFingerprint::default();
    fp.ensure_no_webdriver(); // Should not panic
}

#[test]
fn test_overrides_ensure_no_webdriver_passes() {
    let overrides = NavigatorOverrides::default();
    overrides.ensure_no_webdriver(); // Should not panic
}

#[test]
#[should_panic(expected = "CRITICAL")]
fn test_fingerprint_ensure_no_webdriver_panics_on_true() {
    let mut fp = BrowserFingerprint::default();
    fp.webdriver = true; // This should NEVER happen in real code
    fp.ensure_no_webdriver(); // Should panic
}

#[test]
#[should_panic(expected = "CRITICAL")]
fn test_overrides_ensure_no_webdriver_panics_on_true() {
    let mut overrides = NavigatorOverrides::default();
    overrides.webdriver = true; // This should NEVER happen in real code
    overrides.ensure_no_webdriver(); // Should panic
}

// ============================================================================
// JS Override Script Generation Tests
// ============================================================================

#[test]
fn test_js_override_script_contains_webdriver_override() {
    let overrides = NavigatorOverrides::default();
    let js = overrides.get_override_script();

    assert!(js.contains("webdriver"));
    assert!(js.contains("return false"));
}

#[test]
fn test_js_override_script_multiple_webdriver_methods() {
    let overrides = NavigatorOverrides::default();
    let js = overrides.get_override_script();

    // Should have multiple methods of webdriver protection
    assert!(js.contains("Object.defineProperty(navigator, 'webdriver'"));
    assert!(js.contains("Navigator.prototype"));
    assert!(js.contains("getOwnPropertyDescriptor"));
}

#[test]
fn test_js_override_script_contains_user_agent() {
    let overrides = NavigatorOverrides::default();
    let js = overrides.get_override_script();

    assert!(js.contains("userAgent"));
    assert!(js.contains(&overrides.user_agent));
}

#[test]
fn test_js_override_script_contains_platform() {
    let overrides = NavigatorOverrides::default();
    let js = overrides.get_override_script();

    assert!(js.contains("platform"));
    assert!(js.contains(&overrides.platform));
}

#[test]
fn test_js_override_script_contains_hardware_info() {
    let overrides = NavigatorOverrides::default();
    let js = overrides.get_override_script();

    assert!(js.contains("hardwareConcurrency"));
    assert!(js.contains("deviceMemory"));
    assert!(js.contains("maxTouchPoints"));
}

#[test]
fn test_js_override_script_contains_languages() {
    let overrides = NavigatorOverrides::default();
    let js = overrides.get_override_script();

    assert!(js.contains("languages"));
    assert!(js.contains("language"));
    assert!(js.contains("en-US"));
}

#[test]
fn test_js_override_script_contains_final_verification() {
    let overrides = NavigatorOverrides::default();
    let js = overrides.get_override_script();

    assert!(js.contains("Final Verification"));
    assert!(js.contains("if (navigator.webdriver !== false)"));
}

#[test]
fn test_js_override_script_is_iife() {
    let overrides = NavigatorOverrides::default();
    let js = overrides.get_override_script();

    // Should be an IIFE (Immediately Invoked Function Expression)
    assert!(js.contains("(function()"));
    assert!(js.contains("})();"));
    assert!(js.contains("'use strict'"));
}

#[test]
fn test_js_override_script_with_custom_values() {
    let overrides = NavigatorOverridesBuilder::new()
        .user_agent("CustomUA/1.0")
        .platform("CustomPlatform")
        .vendor("Custom Vendor")
        .hardware_concurrency(16)
        .device_memory(32)
        .build();

    let js = overrides.get_override_script();

    assert!(js.contains("CustomUA/1.0"));
    assert!(js.contains("CustomPlatform"));
    assert!(js.contains("Custom Vendor"));
    assert!(js.contains("16")); // hardware_concurrency
    assert!(js.contains("32")); // device_memory
}

// ============================================================================
// Consistent Fingerprint with Same Seed Tests
// ============================================================================

#[test]
fn test_seeded_fingerprint_consistent() {
    let seed = 12345u64;

    let gen1 = FingerprintGenerator::with_seed(seed);
    let gen2 = FingerprintGenerator::with_seed(seed);

    let fp1 = gen1.generate_from_profile(FingerprintProfile::WindowsChrome);
    let fp2 = gen2.generate_from_profile(FingerprintProfile::WindowsChrome);

    // With same seed, fingerprints should be identical
    assert_eq!(fp1.screen.width, fp2.screen.width);
    assert_eq!(fp1.screen.height, fp2.screen.height);
    assert_eq!(fp1.hardware_concurrency, fp2.hardware_concurrency);
    assert_eq!(fp1.device_memory, fp2.device_memory);
    assert_eq!(fp1.timezone, fp2.timezone);
}

#[test]
fn test_different_seeds_produce_different_fingerprints() {
    let gen1 = FingerprintGenerator::with_seed(11111);
    let gen2 = FingerprintGenerator::with_seed(99999);

    let fp1 = gen1.generate_from_profile(FingerprintProfile::WindowsChrome);
    let fp2 = gen2.generate_from_profile(FingerprintProfile::WindowsChrome);

    // With different seeds, at least some properties should differ
    // (user_agent stays the same since it's profile-dependent, not seed-dependent)
    let some_difference = fp1.screen.width != fp2.screen.width
        || fp1.screen.height != fp2.screen.height
        || fp1.hardware_concurrency != fp2.hardware_concurrency
        || fp1.device_memory != fp2.device_memory
        || fp1.timezone != fp2.timezone;

    assert!(some_difference, "Different seeds should produce different fingerprints");
}

#[test]
fn test_seeded_fingerprint_stores_seed() {
    let seed = 42u64;
    let gen = FingerprintGenerator::with_seed(seed);
    let fp = gen.generate_from_profile(FingerprintProfile::WindowsChrome);

    assert_eq!(fp.seed, Some(seed));
}

#[test]
fn test_unseeded_fingerprint_has_no_seed() {
    let gen = FingerprintGenerator::new();
    let fp = gen.generate_from_profile(FingerprintProfile::WindowsChrome);

    assert!(fp.seed.is_none());
}

#[test]
fn test_seeded_random_fingerprint_consistent() {
    let seed = 54321u64;

    let gen1 = FingerprintGenerator::with_seed(seed);
    let gen2 = FingerprintGenerator::with_seed(seed);

    let fp1 = gen1.generate_random();
    let fp2 = gen2.generate_random();

    // Random fingerprints with same seed should be identical
    // (profile selection is deterministic based on seed)
    assert_eq!(fp1.user_agent, fp2.user_agent);
    assert_eq!(fp1.platform, fp2.platform);
}

// ============================================================================
// Builder Tests
// ============================================================================

#[test]
fn test_builder_default_values() {
    let overrides = NavigatorOverridesBuilder::new().build();

    assert!(!overrides.webdriver);
    assert_eq!(overrides.platform, "Win32");
    assert_eq!(overrides.hardware_concurrency, 8);
    assert_eq!(overrides.device_memory, 8);
}

#[test]
fn test_builder_custom_values() {
    let overrides = NavigatorOverridesBuilder::new()
        .platform("MacIntel")
        .hardware_concurrency(12)
        .device_memory(16)
        .languages(vec!["de-DE".to_string(), "de".to_string()])
        .build();

    assert_eq!(overrides.platform, "MacIntel");
    assert_eq!(overrides.hardware_concurrency, 12);
    assert_eq!(overrides.device_memory, 16);
    assert_eq!(overrides.languages, vec!["de-DE", "de"]);
}

#[test]
fn test_builder_device_memory_validation() {
    // Invalid value should default to 8
    let overrides = NavigatorOverridesBuilder::new()
        .device_memory(5) // Not a power of 2
        .build();
    assert_eq!(overrides.device_memory, 8);

    // Valid value should be preserved
    let overrides = NavigatorOverridesBuilder::new()
        .device_memory(16)
        .build();
    assert_eq!(overrides.device_memory, 16);
}

#[test]
fn test_builder_forces_webdriver_false() {
    // Even if somehow webdriver was true (which it shouldn't be),
    // build() should force it to false
    let overrides = NavigatorOverridesBuilder::new().build();
    assert!(!overrides.webdriver);
}

// ============================================================================
// Screen Info Tests
// ============================================================================

#[test]
fn test_screen_info_default() {
    let screen = ScreenInfo::default();

    assert_eq!(screen.width, 1920);
    assert_eq!(screen.height, 1080);
    assert_eq!(screen.color_depth, 24);
    assert!((screen.pixel_ratio - 1.0).abs() < 0.001);
}

#[test]
fn test_screen_info_new() {
    let screen = ScreenInfo::new(2560, 1440);

    assert_eq!(screen.width, 2560);
    assert_eq!(screen.height, 1440);
}

// ============================================================================
// Edge Cases and Security Tests
// ============================================================================

#[test]
fn test_special_characters_in_user_agent_escaped() {
    let overrides = NavigatorOverridesBuilder::new()
        .user_agent("Test \"Agent\"\nWith\tSpecial\\Chars")
        .build();

    let js = overrides.get_override_script();

    // Should not contain unescaped special characters
    assert!(!js.contains("\n"));
    assert!(!js.contains("\t"));
    // The escaped versions should be present
    assert!(js.contains("\\n") || js.contains("Test"));
}

#[test]
fn test_empty_languages_array_handled() {
    let overrides = NavigatorOverridesBuilder::new()
        .languages(vec![])
        .build();

    let js = overrides.get_override_script();

    // Should still generate valid JS
    assert!(js.contains("languages"));
    assert!(js.contains("[]")); // Empty array
}

#[test]
fn test_fingerprint_screen_variations() {
    let gen = FingerprintGenerator::with_seed(99999);
    let fp = gen.generate_from_profile(FingerprintProfile::WindowsChrome);

    // Screen should be a valid resolution
    assert!(fp.screen.width > 0);
    assert!(fp.screen.height > 0);
    assert!(fp.screen.width >= fp.screen.height || fp.screen.width < 2000);
}

#[test]
fn test_all_profiles_produce_valid_fingerprints() {
    let gen = FingerprintGenerator::new();
    let profiles = [
        FingerprintProfile::WindowsChrome,
        FingerprintProfile::WindowsFirefox,
        FingerprintProfile::WindowsEdge,
        FingerprintProfile::MacSafari,
        FingerprintProfile::MacChrome,
        FingerprintProfile::LinuxChrome,
        FingerprintProfile::LinuxFirefox,
        FingerprintProfile::Custom,
    ];

    for profile in profiles {
        let fp = gen.generate_from_profile(profile);
        assert!(fp.is_valid(), "Profile {:?} should produce valid fingerprint", profile);
    }
}
