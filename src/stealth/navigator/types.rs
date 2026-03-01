//! Core types for navigator property anti-detection overrides.
//!
//! Contains `PluginInfo`, `MimeTypeInfo`, and `NavigatorOverrides` structs
//! used to spoof browser navigator properties and prevent automation detection.

use crate::stealth::fingerprint::BrowserFingerprint;

use super::helpers::{default_chrome_plugins, extract_app_version};

/// Information about a browser plugin for navigator.plugins spoofing
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PluginInfo {
    /// Plugin name
    pub name: String,
    /// Plugin description
    pub description: String,
    /// Plugin filename
    pub filename: String,
    /// Plugin version (if available)
    pub version: Option<String>,
    /// MIME types supported by this plugin
    pub mime_types: Vec<MimeTypeInfo>,
}

impl PluginInfo {
    /// Create a new plugin info
    pub fn new(
        name: impl Into<String>,
        description: impl Into<String>,
        filename: impl Into<String>,
    ) -> Self {
        Self {
            name: name.into(),
            description: description.into(),
            filename: filename.into(),
            version: None,
            mime_types: Vec::new(),
        }
    }

    /// Add a MIME type
    pub fn with_mime_type(mut self, mime_type: MimeTypeInfo) -> Self {
        self.mime_types.push(mime_type);
        self
    }

    /// Set version
    pub fn with_version(mut self, version: impl Into<String>) -> Self {
        self.version = Some(version.into());
        self
    }

    /// Chrome PDF Viewer plugin preset
    pub fn chrome_pdf_viewer() -> Self {
        Self::new(
            "Chrome PDF Viewer",
            "Portable Document Format",
            "internal-pdf-viewer",
        )
        .with_mime_type(MimeTypeInfo::pdf())
    }

    /// Chromium PDF Viewer plugin preset
    pub fn chromium_pdf_viewer() -> Self {
        Self::new(
            "Chromium PDF Viewer",
            "Portable Document Format",
            "internal-pdf-viewer",
        )
        .with_mime_type(MimeTypeInfo::pdf())
    }

    /// Native Client plugin preset
    pub fn native_client() -> Self {
        Self::new("Native Client", "", "internal-nacl-plugin")
    }
}

/// Information about a MIME type for navigator.mimeTypes spoofing
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MimeTypeInfo {
    /// MIME type string (e.g., "application/pdf")
    pub mime_type: String,
    /// Description of the MIME type
    pub description: String,
    /// File extensions (e.g., "pdf")
    pub suffixes: String,
}

impl MimeTypeInfo {
    /// Create a new MIME type info
    pub fn new(
        mime_type: impl Into<String>,
        description: impl Into<String>,
        suffixes: impl Into<String>,
    ) -> Self {
        Self {
            mime_type: mime_type.into(),
            description: description.into(),
            suffixes: suffixes.into(),
        }
    }

    /// PDF MIME type preset
    pub fn pdf() -> Self {
        Self::new("application/pdf", "Portable Document Format", "pdf")
    }

    /// PDF (x-pdf variant) MIME type preset
    pub fn x_pdf() -> Self {
        Self::new("application/x-pdf", "Portable Document Format", "pdf")
    }

    /// Text PDF MIME type preset
    pub fn text_pdf() -> Self {
        Self::new("text/pdf", "Portable Document Format", "pdf")
    }
}

/// Navigator property overrides for anti-detection fingerprint spoofing.
///
/// Contains all navigator properties that should be overridden
/// to prevent bot detection. The `webdriver` field MUST always be `false`.
#[derive(Debug, Clone)]
pub struct NavigatorOverrides {
    /// CRITICAL: Must ALWAYS be false to avoid detection.
    /// This is the primary method websites use to detect automation.
    pub webdriver: bool,

    /// Accepted languages (e.g., ["en-US", "en"])
    pub languages: Vec<String>,

    /// Platform string (e.g., "Win32", "MacIntel", "Linux x86_64")
    pub platform: String,

    /// Number of logical CPU cores
    pub hardware_concurrency: u8,

    /// Device memory in GB (must be power of 2: 2, 4, 8, 16, 32)
    pub device_memory: u8,

    /// Maximum touch points (0 for non-touch devices)
    pub max_touch_points: u8,

    /// Vendor string (e.g., "Google Inc.")
    pub vendor: String,

    /// Vendor sub (usually empty string)
    pub vendor_sub: String,

    /// Product (usually "Gecko")
    pub product: String,

    /// Product sub (usually "20030107" or "20100101")
    pub product_sub: String,

    /// User agent string
    pub user_agent: String,

    /// App version
    pub app_version: String,

    /// App name
    pub app_name: String,

    /// App code name
    pub app_code_name: String,

    /// Whether cookies are enabled
    pub cookie_enabled: bool,

    /// Whether the browser is online
    pub on_line: bool,

    /// Do Not Track preference
    pub do_not_track: Option<String>,

    /// PDF viewer enabled
    pub pdf_viewer_enabled: bool,

    /// List of plugins
    pub plugins: Vec<PluginInfo>,

    /// Whether permissions should be spoofed
    pub spoof_permissions: bool,

    /// Additional properties to inject as automation signals removal
    pub remove_automation_signals: bool,
}

impl NavigatorOverrides {
    /// Create navigator overrides from a browser fingerprint
    pub fn from_fingerprint(fingerprint: &BrowserFingerprint) -> Self {
        Self {
            webdriver: false, // CRITICAL: Always false
            languages: fingerprint.languages.clone(),
            platform: fingerprint.platform.clone(),
            hardware_concurrency: 8, // Common value
            device_memory: 8,        // Common value
            max_touch_points: 0,     // Desktop
            vendor: fingerprint.vendor.clone(),
            vendor_sub: String::new(),
            product: "Gecko".to_string(),
            product_sub: "20030107".to_string(),
            user_agent: fingerprint.user_agent.clone(),
            app_version: extract_app_version(&fingerprint.user_agent),
            app_name: "Netscape".to_string(),
            app_code_name: "Mozilla".to_string(),
            cookie_enabled: fingerprint.cookie_enabled,
            on_line: true,
            do_not_track: fingerprint.do_not_track.clone(),
            pdf_viewer_enabled: true,
            plugins: default_chrome_plugins(),
            spoof_permissions: true,
            remove_automation_signals: true,
        }
    }

    /// CRITICAL: Ensure webdriver is never true.
    ///
    /// Safety check that will panic if webdriver is true.
    /// Must be called before using the configuration.
    pub fn ensure_no_webdriver(&self) {
        if self.webdriver {
            panic!("CRITICAL SECURITY ERROR: navigator.webdriver MUST be false! Current value is true, which will expose automation detection.");
        }
    }
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
            vendor_sub: String::new(),
            product: "Gecko".to_string(),
            product_sub: "20030107".to_string(),
            user_agent: "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/120.0.0.0 Safari/537.36".to_string(),
            app_version: "5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/120.0.0.0 Safari/537.36".to_string(),
            app_name: "Netscape".to_string(),
            app_code_name: "Mozilla".to_string(),
            cookie_enabled: true,
            on_line: true,
            do_not_track: None,
            pdf_viewer_enabled: true,
            plugins: default_chrome_plugins(),
            spoof_permissions: true,
            remove_automation_signals: true,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_webdriver_always_false() {
        let overrides = NavigatorOverrides::default();
        assert!(!overrides.webdriver, "webdriver MUST be false");
    }

    #[test]
    fn test_ensure_no_webdriver() {
        let overrides = NavigatorOverrides::default();
        // This should not panic
        overrides.ensure_no_webdriver();
    }

    #[test]
    #[should_panic(expected = "CRITICAL SECURITY ERROR")]
    fn test_ensure_no_webdriver_panics_on_true() {
        let mut overrides = NavigatorOverrides::default();
        overrides.webdriver = true; // This should never happen in real code
        overrides.ensure_no_webdriver(); // Should panic
    }

    #[test]
    fn test_from_fingerprint() {
        use crate::stealth::fingerprint::{FingerprintGenerator, FingerprintProfile};

        let generator = FingerprintGenerator::new();
        let fingerprint = generator.generate_from_profile(FingerprintProfile::WindowsChrome);
        let overrides = NavigatorOverrides::from_fingerprint(&fingerprint);

        assert!(!overrides.webdriver);
        assert_eq!(overrides.user_agent, fingerprint.user_agent);
        assert_eq!(overrides.platform, fingerprint.platform);
    }

    #[test]
    fn test_plugin_info() {
        let plugin = PluginInfo::chrome_pdf_viewer();
        assert_eq!(plugin.name, "Chrome PDF Viewer");
        assert!(!plugin.mime_types.is_empty());
        assert_eq!(plugin.mime_types[0].mime_type, "application/pdf");
    }
}
