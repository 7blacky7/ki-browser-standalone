//! Navigator Property Management
//!
//! This module provides comprehensive navigator property overrides for anti-detection.
//! The most critical aspect is ensuring `navigator.webdriver` is NEVER exposed as `true`.
//!
//! # Critical: WebDriver Detection Prevention
//!
//! Automated browsers typically expose `navigator.webdriver = true`, which is the
//! primary method websites use to detect automation. This module provides multiple
//! layers of protection:
//!
//! 1. Direct property override
//! 2. Getter redefinition
//! 3. Prototype chain protection
//! 4. Object.getOwnPropertyDescriptor spoofing
//!
//! # Example
//!
//! ```rust,no_run
//! use ki_browser_standalone::stealth::navigator::NavigatorOverrides;
//!
//! let overrides = NavigatorOverrides::default();
//!
//! // CRITICAL: webdriver is ALWAYS false
//! assert!(!overrides.webdriver);
//!
//! // Get the JavaScript override script
//! let js = overrides.get_override_script();
//! ```

use crate::stealth::fingerprint::BrowserFingerprint;

/// Information about a browser plugin
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
    pub fn new(name: impl Into<String>, description: impl Into<String>, filename: impl Into<String>) -> Self {
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

    /// Chrome PDF Viewer plugin
    pub fn chrome_pdf_viewer() -> Self {
        Self::new(
            "Chrome PDF Viewer",
            "Portable Document Format",
            "internal-pdf-viewer",
        )
        .with_mime_type(MimeTypeInfo::pdf())
    }

    /// Chromium PDF Viewer plugin
    pub fn chromium_pdf_viewer() -> Self {
        Self::new(
            "Chromium PDF Viewer",
            "Portable Document Format",
            "internal-pdf-viewer",
        )
        .with_mime_type(MimeTypeInfo::pdf())
    }

    /// Native Client plugin
    pub fn native_client() -> Self {
        Self::new(
            "Native Client",
            "",
            "internal-nacl-plugin",
        )
    }
}

/// Information about a MIME type
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
    pub fn new(mime_type: impl Into<String>, description: impl Into<String>, suffixes: impl Into<String>) -> Self {
        Self {
            mime_type: mime_type.into(),
            description: description.into(),
            suffixes: suffixes.into(),
        }
    }

    /// PDF MIME type
    pub fn pdf() -> Self {
        Self::new("application/pdf", "Portable Document Format", "pdf")
    }

    /// PDF (x-pdf variant) MIME type
    pub fn x_pdf() -> Self {
        Self::new("application/x-pdf", "Portable Document Format", "pdf")
    }

    /// Text PDF MIME type
    pub fn text_pdf() -> Self {
        Self::new("text/pdf", "Portable Document Format", "pdf")
    }
}

/// Navigator property overrides for anti-detection
///
/// This struct contains all navigator properties that should be overridden
/// to prevent bot detection.
#[derive(Debug, Clone)]
pub struct NavigatorOverrides {
    /// CRITICAL: Must ALWAYS be false to avoid detection
    /// This is the primary method websites use to detect automation
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

    /// CRITICAL FUNCTION: Ensure webdriver is never true
    ///
    /// This function MUST be called before using the configuration.
    /// It is a safety check that will panic if webdriver is true.
    pub fn ensure_no_webdriver(&self) {
        if self.webdriver {
            panic!("CRITICAL SECURITY ERROR: navigator.webdriver MUST be false! Current value is true, which will expose automation detection.");
        }
    }

    /// Generate JavaScript override script
    ///
    /// This generates comprehensive JavaScript code to override all navigator
    /// properties and prevent detection of automation.
    ///
    /// CRITICAL: This script MUST be injected before any page scripts run.
    pub fn get_override_script(&self) -> String {
        // Safety check
        self.ensure_no_webdriver();

        let languages_json = self.languages_to_json();
        let plugins_json = self.plugins_to_json();
        let dnt_value = match &self.do_not_track {
            Some(v) => format!("\"{}\"", v),
            None => "null".to_string(),
        };

        format!(
            r#"
// ============================================================================
// CRITICAL NAVIGATOR ANTI-DETECTION OVERRIDES
// This script MUST run before any page scripts to prevent detection
// ============================================================================

(function() {{
    'use strict';

    // ========================================================================
    // CRITICAL: WebDriver Detection Prevention
    // This is THE MOST IMPORTANT anti-detection measure
    // ========================================================================

    // Method 1: Direct property override
    Object.defineProperty(navigator, 'webdriver', {{
        get: function() {{ return false; }},
        configurable: true,
        enumerable: true
    }});

    // Method 2: Delete the property first, then redefine
    try {{
        delete navigator.webdriver;
        Object.defineProperty(navigator, 'webdriver', {{
            get: function() {{ return false; }},
            configurable: true,
            enumerable: true
        }});
    }} catch (e) {{}}

    // Method 3: Override on the Navigator prototype
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

    // Method 5: Override toString to hide our modifications
    const originalNavigatorToString = navigator.toString;
    navigator.toString = function() {{
        return '[object Navigator]';
    }};

    // ========================================================================
    // User Agent and Related Properties
    // ========================================================================

    Object.defineProperty(navigator, 'userAgent', {{
        get: function() {{ return "{user_agent}"; }},
        configurable: true
    }});

    Object.defineProperty(navigator, 'appVersion', {{
        get: function() {{ return "{app_version}"; }},
        configurable: true
    }});

    Object.defineProperty(navigator, 'appName', {{
        get: function() {{ return "{app_name}"; }},
        configurable: true
    }});

    Object.defineProperty(navigator, 'appCodeName', {{
        get: function() {{ return "{app_code_name}"; }},
        configurable: true
    }});

    Object.defineProperty(navigator, 'product', {{
        get: function() {{ return "{product}"; }},
        configurable: true
    }});

    Object.defineProperty(navigator, 'productSub', {{
        get: function() {{ return "{product_sub}"; }},
        configurable: true
    }});

    Object.defineProperty(navigator, 'vendor', {{
        get: function() {{ return "{vendor}"; }},
        configurable: true
    }});

    Object.defineProperty(navigator, 'vendorSub', {{
        get: function() {{ return "{vendor_sub}"; }},
        configurable: true
    }});

    // ========================================================================
    // Platform and Hardware Properties
    // ========================================================================

    Object.defineProperty(navigator, 'platform', {{
        get: function() {{ return "{platform}"; }},
        configurable: true
    }});

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
    // Connection and Status Properties
    // ========================================================================

    Object.defineProperty(navigator, 'onLine', {{
        get: function() {{ return {on_line}; }},
        configurable: true
    }});

    Object.defineProperty(navigator, 'cookieEnabled', {{
        get: function() {{ return {cookie_enabled}; }},
        configurable: true
    }});

    Object.defineProperty(navigator, 'doNotTrack', {{
        get: function() {{ return {dnt}; }},
        configurable: true
    }});

    Object.defineProperty(navigator, 'pdfViewerEnabled', {{
        get: function() {{ return {pdf_viewer_enabled}; }},
        configurable: true
    }});

    // ========================================================================
    // Plugins Override
    // ========================================================================

    (function() {{
        const pluginData = {plugins_json};
        const plugins = [];
        const mimeTypes = [];

        pluginData.forEach(function(p) {{
            const plugin = Object.create(Plugin.prototype);
            const pluginMimeTypes = [];

            (p.mimeTypes || []).forEach(function(mt) {{
                const mimeType = Object.create(MimeType.prototype);
                Object.defineProperties(mimeType, {{
                    'type': {{ value: mt.type, enumerable: true }},
                    'description': {{ value: mt.description, enumerable: true }},
                    'suffixes': {{ value: mt.suffixes, enumerable: true }},
                    'enabledPlugin': {{ value: plugin, enumerable: true }}
                }});
                pluginMimeTypes.push(mimeType);
                mimeTypes.push(mimeType);
            }});

            Object.defineProperties(plugin, {{
                'name': {{ value: p.name, enumerable: true }},
                'description': {{ value: p.description, enumerable: true }},
                'filename': {{ value: p.filename, enumerable: true }},
                'length': {{ value: pluginMimeTypes.length, enumerable: true }}
            }});

            pluginMimeTypes.forEach(function(mt, i) {{
                Object.defineProperty(plugin, i, {{
                    value: mt,
                    enumerable: true
                }});
            }});

            plugin.item = function(index) {{ return pluginMimeTypes[index] || null; }};
            plugin.namedItem = function(name) {{
                return pluginMimeTypes.find(mt => mt.type === name) || null;
            }};

            plugins.push(plugin);
        }});

        // Create PluginArray
        const pluginArray = Object.create(PluginArray.prototype);
        plugins.forEach(function(plugin, i) {{
            Object.defineProperty(pluginArray, i, {{
                value: plugin,
                enumerable: true
            }});
            Object.defineProperty(pluginArray, plugin.name, {{
                value: plugin,
                enumerable: false
            }});
        }});

        Object.defineProperty(pluginArray, 'length', {{
            value: plugins.length,
            enumerable: true
        }});

        pluginArray.item = function(index) {{ return plugins[index] || null; }};
        pluginArray.namedItem = function(name) {{
            return plugins.find(p => p.name === name) || null;
        }};
        pluginArray.refresh = function() {{}};

        Object.defineProperty(navigator, 'plugins', {{
            get: function() {{ return pluginArray; }},
            configurable: true
        }});

        // Create MimeTypeArray
        const mimeTypeArray = Object.create(MimeTypeArray.prototype);
        mimeTypes.forEach(function(mt, i) {{
            Object.defineProperty(mimeTypeArray, i, {{
                value: mt,
                enumerable: true
            }});
            Object.defineProperty(mimeTypeArray, mt.type, {{
                value: mt,
                enumerable: false
            }});
        }});

        Object.defineProperty(mimeTypeArray, 'length', {{
            value: mimeTypes.length,
            enumerable: true
        }});

        mimeTypeArray.item = function(index) {{ return mimeTypes[index] || null; }};
        mimeTypeArray.namedItem = function(name) {{
            return mimeTypes.find(mt => mt.type === name) || null;
        }};

        Object.defineProperty(navigator, 'mimeTypes', {{
            get: function() {{ return mimeTypeArray; }},
            configurable: true
        }});
    }})();

    // ========================================================================
    // Permissions API Spoofing (Optional)
    // ========================================================================

    {permissions_spoof}

    // ========================================================================
    // Automation Signal Removal (Optional)
    // ========================================================================

    {automation_removal}

    // ========================================================================
    // Final Verification
    // ========================================================================

    // Double-check webdriver is false
    if (navigator.webdriver !== false) {{
        console.error('CRITICAL: navigator.webdriver override failed!');
        // Force it again
        Object.defineProperty(navigator, 'webdriver', {{
            get: function() {{ return false; }},
            configurable: false,
            enumerable: true
        }});
    }}

}})();
"#,
            user_agent = escape_js_string(&self.user_agent),
            app_version = escape_js_string(&self.app_version),
            app_name = escape_js_string(&self.app_name),
            app_code_name = escape_js_string(&self.app_code_name),
            product = escape_js_string(&self.product),
            product_sub = escape_js_string(&self.product_sub),
            vendor = escape_js_string(&self.vendor),
            vendor_sub = escape_js_string(&self.vendor_sub),
            platform = escape_js_string(&self.platform),
            hardware_concurrency = self.hardware_concurrency,
            device_memory = self.device_memory,
            max_touch_points = self.max_touch_points,
            languages_json = languages_json,
            on_line = self.on_line,
            cookie_enabled = self.cookie_enabled,
            dnt = dnt_value,
            pdf_viewer_enabled = self.pdf_viewer_enabled,
            plugins_json = plugins_json,
            permissions_spoof = if self.spoof_permissions {
                get_permissions_spoof_script()
            } else {
                String::new()
            },
            automation_removal = if self.remove_automation_signals {
                get_automation_removal_script()
            } else {
                String::new()
            },
        )
    }

    fn languages_to_json(&self) -> String {
        let entries: Vec<String> = self
            .languages
            .iter()
            .map(|l| format!("\"{}\"", escape_js_string(l)))
            .collect();
        format!("[{}]", entries.join(", "))
    }

    fn plugins_to_json(&self) -> String {
        let entries: Vec<String> = self
            .plugins
            .iter()
            .map(|p| {
                let mime_types: Vec<String> = p
                    .mime_types
                    .iter()
                    .map(|mt| {
                        format!(
                            r#"{{"type":"{}","description":"{}","suffixes":"{}"}}"#,
                            escape_js_string(&mt.mime_type),
                            escape_js_string(&mt.description),
                            escape_js_string(&mt.suffixes)
                        )
                    })
                    .collect();

                format!(
                    r#"{{"name":"{}","description":"{}","filename":"{}","mimeTypes":[{}]}}"#,
                    escape_js_string(&p.name),
                    escape_js_string(&p.description),
                    escape_js_string(&p.filename),
                    mime_types.join(",")
                )
            })
            .collect();
        format!("[{}]", entries.join(","))
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

/// Get default Chrome plugins
fn default_chrome_plugins() -> Vec<PluginInfo> {
    vec![
        PluginInfo::new("PDF Viewer", "Portable Document Format", "internal-pdf-viewer")
            .with_mime_type(MimeTypeInfo::pdf()),
        PluginInfo::chrome_pdf_viewer(),
        PluginInfo::chromium_pdf_viewer(),
        PluginInfo::new(
            "Microsoft Edge PDF Viewer",
            "Portable Document Format",
            "internal-pdf-viewer",
        )
        .with_mime_type(MimeTypeInfo::pdf()),
        PluginInfo::new(
            "WebKit built-in PDF",
            "Portable Document Format",
            "internal-pdf-viewer",
        )
        .with_mime_type(MimeTypeInfo::pdf()),
    ]
}

/// Extract app version from user agent
fn extract_app_version(user_agent: &str) -> String {
    // App version is typically everything after "Mozilla/"
    if let Some(pos) = user_agent.find("Mozilla/") {
        user_agent[pos + 8..].to_string()
    } else {
        user_agent.to_string()
    }
}

/// Escape string for JavaScript
fn escape_js_string(s: &str) -> String {
    s.replace('\\', "\\\\")
        .replace('"', "\\\"")
        .replace('\n', "\\n")
        .replace('\r', "\\r")
        .replace('\t', "\\t")
        .replace('\'', "\\'")
}

/// Get JavaScript for permissions API spoofing
fn get_permissions_spoof_script() -> String {
    r#"
    // Permissions API spoofing
    if (typeof Permissions !== 'undefined' && Permissions.prototype.query) {
        const originalQuery = Permissions.prototype.query;
        Permissions.prototype.query = function(permissionDesc) {
            return new Promise((resolve, reject) => {
                originalQuery.call(this, permissionDesc)
                    .then(result => {
                        // Don't reveal "prompt" for sensitive permissions
                        // as automation tools often have different defaults
                        resolve(result);
                    })
                    .catch(reject);
            });
        };
    }
    "#.to_string()
}

/// Get JavaScript for removing automation signals
fn get_automation_removal_script() -> String {
    r#"
    // Remove common automation signals

    // Remove CDP (Chrome DevTools Protocol) signals
    try {
        delete window.cdc_adoQpoasnfa76pfcZLmcfl_Array;
        delete window.cdc_adoQpoasnfa76pfcZLmcfl_Promise;
        delete window.cdc_adoQpoasnfa76pfcZLmcfl_Symbol;
    } catch (e) {}

    // Remove Selenium signals
    try {
        delete window._selenium;
        delete window.callSelenium;
        delete window._Selenium_IDE_Recorder;
        delete window.__webdriver_script_fn;
        delete window.__driver_evaluate;
        delete window.__webdriver_evaluate;
        delete window.__selenium_evaluate;
        delete window.__fxdriver_evaluate;
        delete window.__driver_unwrapped;
        delete window.__webdriver_unwrapped;
        delete window.__selenium_unwrapped;
        delete window.__fxdriver_unwrapped;
        delete window.__webdriver_script_func;
        delete window.$chrome_asyncScriptInfo;
        delete window.$cdc_asdjflasutopfhvcZLmcfl_;
    } catch (e) {}

    // Remove PhantomJS signals
    try {
        delete window.callPhantom;
        delete window._phantom;
    } catch (e) {}

    // Remove Nightmare signals
    try {
        delete window.__nightmare;
    } catch (e) {}

    // Remove general automation signals
    try {
        delete window.domAutomation;
        delete window.domAutomationController;
    } catch (e) {}

    // Override console.debug to hide potential automation logs
    const originalDebug = console.debug;
    console.debug = function(...args) {
        // Filter out automation-related debug messages
        const message = args.join(' ');
        if (message.includes('webdriver') || message.includes('automation')) {
            return;
        }
        return originalDebug.apply(console, args);
    };

    // Protect against detection via error stack traces
    const originalError = Error;
    window.Error = function(...args) {
        const error = new originalError(...args);
        // Clean stack trace of automation indicators
        if (error.stack) {
            error.stack = error.stack
                .split('\n')
                .filter(line => !line.includes('webdriver') && !line.includes('puppeteer'))
                .join('\n');
        }
        return error;
    };
    window.Error.prototype = originalError.prototype;

    // Override performance.getEntries to hide automation resources
    if (typeof Performance !== 'undefined' && Performance.prototype.getEntries) {
        const originalGetEntries = Performance.prototype.getEntries;
        Performance.prototype.getEntries = function() {
            return originalGetEntries.call(this).filter(entry => {
                const name = entry.name || '';
                return !name.includes('webdriver') &&
                       !name.includes('puppeteer') &&
                       !name.includes('playwright');
            });
        };
    }
    "#.to_string()
}

/// Builder for creating custom NavigatorOverrides
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

    /// Set languages
    pub fn languages(mut self, languages: Vec<String>) -> Self {
        self.overrides.languages = languages;
        self
    }

    /// Set platform
    pub fn platform(mut self, platform: impl Into<String>) -> Self {
        self.overrides.platform = platform.into();
        self
    }

    /// Set hardware concurrency
    pub fn hardware_concurrency(mut self, cores: u8) -> Self {
        self.overrides.hardware_concurrency = cores;
        self
    }

    /// Set device memory
    pub fn device_memory(mut self, memory_gb: u8) -> Self {
        // Must be power of 2
        let valid_values = [2, 4, 8, 16, 32];
        self.overrides.device_memory = if valid_values.contains(&memory_gb) {
            memory_gb
        } else {
            8 // Default to 8GB
        };
        self
    }

    /// Set max touch points
    pub fn max_touch_points(mut self, points: u8) -> Self {
        self.overrides.max_touch_points = points;
        self
    }

    /// Set user agent
    pub fn user_agent(mut self, user_agent: impl Into<String>) -> Self {
        let ua: String = user_agent.into();
        self.overrides.app_version = extract_app_version(&ua);
        self.overrides.user_agent = ua;
        self
    }

    /// Set vendor
    pub fn vendor(mut self, vendor: impl Into<String>) -> Self {
        self.overrides.vendor = vendor.into();
        self
    }

    /// Set plugins
    pub fn plugins(mut self, plugins: Vec<PluginInfo>) -> Self {
        self.overrides.plugins = plugins;
        self
    }

    /// Enable or disable permissions spoofing
    pub fn spoof_permissions(mut self, enabled: bool) -> Self {
        self.overrides.spoof_permissions = enabled;
        self
    }

    /// Enable or disable automation signal removal
    pub fn remove_automation_signals(mut self, enabled: bool) -> Self {
        self.overrides.remove_automation_signals = enabled;
        self
    }

    /// Build the final NavigatorOverrides
    ///
    /// Note: webdriver will ALWAYS be false regardless of any other settings.
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
    fn test_webdriver_always_false() {
        let overrides = NavigatorOverrides::default();
        assert!(!overrides.webdriver, "webdriver MUST be false");
    }

    #[test]
    fn test_builder_forces_webdriver_false() {
        // Even if somehow webdriver was set to true, build should force it false
        let overrides = NavigatorOverridesBuilder::new().build();
        assert!(!overrides.webdriver, "webdriver MUST be false after build");
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
    fn test_js_override_contains_webdriver() {
        let overrides = NavigatorOverrides::default();
        let js = overrides.get_override_script();

        // Check that webdriver override is present
        assert!(js.contains("webdriver"));
        assert!(js.contains("return false"));
        assert!(js.contains("Navigator.prototype"));
    }

    #[test]
    fn test_js_override_contains_all_properties() {
        let overrides = NavigatorOverrides::default();
        let js = overrides.get_override_script();

        assert!(js.contains("userAgent"));
        assert!(js.contains("platform"));
        assert!(js.contains("hardwareConcurrency"));
        assert!(js.contains("deviceMemory"));
        assert!(js.contains("languages"));
        assert!(js.contains("plugins"));
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
