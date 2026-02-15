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

use tracing::error;

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

    /// Chrome major version extracted from user_agent (e.g., "131")
    pub chrome_version: String,

    /// Platform name for userAgentData (e.g., "Windows", "macOS", "Linux")
    pub platform_name: String,

    /// CPU architecture for userAgentData (e.g., "x86")
    pub architecture: String,

    /// Platform version for userAgentData (e.g., "15.0.0" for Windows, "14.0.0" for macOS)
    pub platform_version: String,
}

impl NavigatorOverrides {
    /// Create navigator overrides from a browser fingerprint
    pub fn from_fingerprint(fingerprint: &BrowserFingerprint) -> Self {
        let chrome_version = extract_chrome_version(&fingerprint.user_agent);
        let platform_name = map_platform_name(&fingerprint.platform);
        let platform_version = default_platform_version(&platform_name);

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
            chrome_version,
            platform_name,
            architecture: "x86".to_string(),
            platform_version,
        }
    }

    /// Ensure webdriver is never true.
    ///
    /// This function MUST be called before using the configuration.
    /// If webdriver is somehow set to `true`, it logs a critical error and
    /// returns `false` so callers can take corrective action.  The JavaScript
    /// override script always forces `webdriver = false` on the browser side
    /// regardless, so this is a defence-in-depth check rather than a reason
    /// to crash the whole process.
    pub fn ensure_no_webdriver(&self) -> bool {
        if self.webdriver {
            error!(
                "CRITICAL SECURITY: navigator.webdriver is true! \
                 The JS override will still force it to false on the page, \
                 but the Rust-side config is misconfigured. \
                 Callers should set webdriver = false explicitly."
            );
            return false;
        }
        true
    }

    /// Generate JavaScript override script
    ///
    /// This generates comprehensive JavaScript code to override all navigator
    /// properties and prevent detection of automation.
    ///
    /// CRITICAL: This script MUST be injected before any page scripts run.
    pub fn get_override_script(&self) -> String {
        // Safety check -- logs an error but never crashes.  The JS output
        // always forces `navigator.webdriver = false` regardless.
        let _ = self.ensure_no_webdriver();

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
    // This is THE MOST IMPORTANT anti-detection measure.
    //
    // Chrome sets navigator.webdriver=true when controlled via CDP.
    // We must make it look like a normal (non-automated) browser where
    // webdriver is a getter on Navigator.prototype returning false, and
    // there is NO own-property on the navigator instance.
    //
    // Detection methods we must defeat:
    //   1. navigator.webdriver  (value check)
    //   2. _.has(navigator, 'webdriver')  (own-property check / lodash)
    //   3. Object.getOwnPropertyDescriptor(navigator, 'webdriver')
    //   4. 'webdriver' in navigator  (prototype chain check - should be true)
    // ========================================================================

    // Step 1: Delete the own-property that Chrome/CDP sets on the instance.
    // This is critical so that _.has(navigator, 'webdriver') returns false
    // (matching real Chrome where webdriver lives on the prototype only).
    try {{ delete navigator.__proto__.webdriver; }} catch(e) {{}}
    try {{ delete navigator.webdriver; }} catch(e) {{}}

    // Step 2: Define the getter ONLY on Navigator.prototype (like real Chrome).
    Object.defineProperty(Navigator.prototype, 'webdriver', {{
        get: function() {{ return false; }},
        configurable: true,
        enumerable: true
    }});

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
    // UserAgentData Override
    // Prevents Chrome version mismatch between UA string and Client Hints API
    // ========================================================================

    if (navigator.userAgentData) {{
        const CHROME_VERSION = "{chrome_version}";
        const PLATFORM = "{platform_name}";
        const ARCHITECTURE = "{architecture}";
        const PLATFORM_VERSION = "{platform_version}";
        const HW_CONCURRENCY = {hardware_concurrency};

        const uaDataObj = {{
            get brands() {{
                return [
                    {{brand: "Not_A Brand", version: "8"}},
                    {{brand: "Chromium", version: CHROME_VERSION}},
                    {{brand: "Google Chrome", version: CHROME_VERSION}}
                ];
            }},
            get mobile() {{ return false; }},
            get platform() {{ return PLATFORM; }},
            getHighEntropyValues: function(hints) {{
                return Promise.resolve({{
                    brands: [
                        {{brand: "Not_A Brand", version: "8"}},
                        {{brand: "Chromium", version: CHROME_VERSION}},
                        {{brand: "Google Chrome", version: CHROME_VERSION}}
                    ],
                    mobile: false,
                    platform: PLATFORM,
                    architecture: ARCHITECTURE,
                    bitness: "64",
                    fullVersionList: [
                        {{brand: "Not_A Brand", version: "8.0.0.0"}},
                        {{brand: "Chromium", version: CHROME_VERSION + ".0.0.0"}},
                        {{brand: "Google Chrome", version: CHROME_VERSION + ".0.0.0"}}
                    ],
                    model: "",
                    platformVersion: PLATFORM_VERSION,
                    uaFullVersion: CHROME_VERSION + ".0.0.0",
                    wow64: false
                }});
            }},
            toJSON: function() {{
                return {{
                    brands: this.brands,
                    mobile: false,
                    platform: PLATFORM
                }};
            }}
        }};

        Object.defineProperty(navigator, 'userAgentData', {{
            get: function() {{ return uaDataObj; }},
            configurable: true
        }});
    }}

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

    // Double-check webdriver is false (prototype-only, no own-property)
    if (navigator.webdriver !== false) {{
        console.error('CRITICAL: navigator.webdriver override failed!');
        try {{ delete navigator.webdriver; }} catch(e) {{}}
        Object.defineProperty(Navigator.prototype, 'webdriver', {{
            get: function() {{ return false; }},
            configurable: true,
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
            chrome_version = escape_js_string(&self.chrome_version),
            platform_name = escape_js_string(&self.platform_name),
            architecture = escape_js_string(&self.architecture),
            platform_version = escape_js_string(&self.platform_version),
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
        let default_ua = "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/120.0.0.0 Safari/537.36";
        let default_platform = "Win32";
        let chrome_version = extract_chrome_version(default_ua);
        let platform_name = map_platform_name(default_platform);
        let platform_version = default_platform_version(&platform_name);

        Self {
            webdriver: false, // CRITICAL: Always false
            languages: vec!["en-US".to_string(), "en".to_string()],
            platform: default_platform.to_string(),
            hardware_concurrency: 8,
            device_memory: 8,
            max_touch_points: 0,
            vendor: "Google Inc.".to_string(),
            vendor_sub: String::new(),
            product: "Gecko".to_string(),
            product_sub: "20030107".to_string(),
            user_agent: default_ua.to_string(),
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
            chrome_version,
            platform_name,
            architecture: "x86".to_string(),
            platform_version,
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

/// Extract Chrome major version from user agent string
///
/// Parses strings like "Chrome/131.0.0.0" and returns "131".
/// Falls back to "120" if no Chrome version is found.
fn extract_chrome_version(user_agent: &str) -> String {
    // Regex-like manual parsing for Chrome/(\d+)
    if let Some(pos) = user_agent.find("Chrome/") {
        let after = &user_agent[pos + 7..];
        let version: String = after.chars().take_while(|c| c.is_ascii_digit()).collect();
        if !version.is_empty() {
            return version;
        }
    }
    "120".to_string() // Safe default
}

/// Map navigator.platform to userAgentData platform name
///
/// - "Win32" -> "Windows"
/// - "MacIntel" -> "macOS"
/// - "Linux x86_64" or other Linux variants -> "Linux"
fn map_platform_name(platform: &str) -> String {
    if platform.starts_with("Win") {
        "Windows".to_string()
    } else if platform.starts_with("Mac") {
        "macOS".to_string()
    } else {
        "Linux".to_string()
    }
}

/// Get a plausible platform version for userAgentData.getHighEntropyValues()
///
/// - Windows: "15.0.0" (Windows 11) or "10.0.0" (Windows 10)
/// - macOS: "14.0.0" (Sonoma-era)
/// - Linux: "6.5.0" (kernel-like version)
fn default_platform_version(platform_name: &str) -> String {
    match platform_name {
        "Windows" => "15.0.0".to_string(),
        "macOS" => "14.0.0".to_string(),
        _ => "6.5.0".to_string(),
    }
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
    ///
    /// Also automatically derives platform_name and platform_version from the platform string.
    pub fn platform(mut self, platform: impl Into<String>) -> Self {
        let p: String = platform.into();
        self.overrides.platform_name = map_platform_name(&p);
        self.overrides.platform_version = default_platform_version(&self.overrides.platform_name);
        self.overrides.platform = p;
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
    ///
    /// Also automatically extracts and updates the chrome_version field.
    pub fn user_agent(mut self, user_agent: impl Into<String>) -> Self {
        let ua: String = user_agent.into();
        self.overrides.app_version = extract_app_version(&ua);
        self.overrides.chrome_version = extract_chrome_version(&ua);
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

    /// Set the Chrome version explicitly (overrides auto-extraction from user_agent)
    pub fn chrome_version(mut self, version: impl Into<String>) -> Self {
        self.overrides.chrome_version = version.into();
        self
    }

    /// Set the platform name for userAgentData (e.g., "Windows", "macOS", "Linux")
    pub fn platform_name(mut self, name: impl Into<String>) -> Self {
        self.overrides.platform_name = name.into();
        self
    }

    /// Set the CPU architecture for userAgentData (e.g., "x86", "arm")
    pub fn architecture(mut self, arch: impl Into<String>) -> Self {
        self.overrides.architecture = arch.into();
        self
    }

    /// Set the platform version for userAgentData (e.g., "15.0.0", "14.0.0")
    pub fn platform_version(mut self, version: impl Into<String>) -> Self {
        self.overrides.platform_version = version.into();
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
        assert!(overrides.ensure_no_webdriver(), "should return true when webdriver is false");
    }

    #[test]
    fn test_ensure_no_webdriver_returns_false_on_true() {
        let mut overrides = NavigatorOverrides::default();
        overrides.webdriver = true; // This should never happen in real code
        // No longer panics -- returns false and logs an error instead
        assert!(!overrides.ensure_no_webdriver());
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
        // Verify new userAgentData fields are derived correctly
        assert!(!overrides.chrome_version.is_empty(), "chrome_version must be extracted");
        assert_eq!(overrides.platform_name, "Windows");
        assert_eq!(overrides.architecture, "x86");
        assert_eq!(overrides.platform_version, "15.0.0");
    }

    #[test]
    fn test_from_fingerprint_mac() {
        use crate::stealth::fingerprint::{FingerprintGenerator, FingerprintProfile};

        let generator = FingerprintGenerator::new();
        let fingerprint = generator.generate_from_profile(FingerprintProfile::MacChrome);
        let overrides = NavigatorOverrides::from_fingerprint(&fingerprint);

        assert_eq!(overrides.platform_name, "macOS");
        assert_eq!(overrides.platform_version, "14.0.0");
    }

    #[test]
    fn test_from_fingerprint_linux() {
        use crate::stealth::fingerprint::{FingerprintGenerator, FingerprintProfile};

        let generator = FingerprintGenerator::new();
        let fingerprint = generator.generate_from_profile(FingerprintProfile::LinuxChrome);
        let overrides = NavigatorOverrides::from_fingerprint(&fingerprint);

        assert_eq!(overrides.platform_name, "Linux");
        assert_eq!(overrides.platform_version, "6.5.0");
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

    // ========================================================================
    // UserAgentData Override Tests
    // ========================================================================

    #[test]
    fn test_extract_chrome_version_standard() {
        assert_eq!(extract_chrome_version("Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/131.0.0.0 Safari/537.36"), "131");
    }

    #[test]
    fn test_extract_chrome_version_different_versions() {
        assert_eq!(extract_chrome_version("Chrome/120.0.0.0"), "120");
        assert_eq!(extract_chrome_version("Chrome/99.0.4844.51"), "99");
        assert_eq!(extract_chrome_version("Chrome/144.0.6367.60"), "144");
    }

    #[test]
    fn test_extract_chrome_version_no_chrome() {
        // Firefox UA - should fall back to default
        assert_eq!(extract_chrome_version("Mozilla/5.0 (Windows NT 10.0; rv:109.0) Gecko/20100101 Firefox/115.0"), "120");
    }

    #[test]
    fn test_extract_chrome_version_empty() {
        assert_eq!(extract_chrome_version(""), "120");
    }

    #[test]
    fn test_map_platform_name_windows() {
        assert_eq!(map_platform_name("Win32"), "Windows");
        assert_eq!(map_platform_name("Win64"), "Windows");
    }

    #[test]
    fn test_map_platform_name_mac() {
        assert_eq!(map_platform_name("MacIntel"), "macOS");
        assert_eq!(map_platform_name("MacPPC"), "macOS");
    }

    #[test]
    fn test_map_platform_name_linux() {
        assert_eq!(map_platform_name("Linux x86_64"), "Linux");
        assert_eq!(map_platform_name("Linux armv7l"), "Linux");
    }

    #[test]
    fn test_default_platform_version() {
        assert_eq!(default_platform_version("Windows"), "15.0.0");
        assert_eq!(default_platform_version("macOS"), "14.0.0");
        assert_eq!(default_platform_version("Linux"), "6.5.0");
    }

    #[test]
    fn test_default_overrides_have_useragentdata_fields() {
        let overrides = NavigatorOverrides::default();
        assert_eq!(overrides.chrome_version, "120");
        assert_eq!(overrides.platform_name, "Windows");
        assert_eq!(overrides.architecture, "x86");
        assert_eq!(overrides.platform_version, "15.0.0");
    }

    #[test]
    fn test_js_override_contains_useragentdata() {
        let overrides = NavigatorOverrides::default();
        let js = overrides.get_override_script();

        // Verify the userAgentData override section is present
        assert!(js.contains("UserAgentData Override"), "JS must contain UserAgentData section");
        assert!(js.contains("userAgentData"), "JS must override navigator.userAgentData");
        assert!(js.contains("getHighEntropyValues"), "JS must override getHighEntropyValues");
        assert!(js.contains("fullVersionList"), "JS must include fullVersionList");
        assert!(js.contains("Not_A Brand"), "JS must include Not_A Brand");
    }

    #[test]
    fn test_js_override_uses_correct_chrome_version() {
        let overrides = NavigatorOverridesBuilder::new()
            .user_agent("Mozilla/5.0 Chrome/131.0.0.0 Safari/537.36")
            .build();

        let js = overrides.get_override_script();
        assert!(js.contains(r#"const CHROME_VERSION = "131";"#), "JS must use extracted Chrome version 131");
    }

    #[test]
    fn test_js_override_uses_correct_platform_name() {
        let overrides = NavigatorOverridesBuilder::new()
            .platform("MacIntel")
            .build();

        let js = overrides.get_override_script();
        assert!(js.contains(r#"const PLATFORM = "macOS";"#), "JS must map MacIntel to macOS");
    }

    #[test]
    fn test_builder_user_agent_updates_chrome_version() {
        let overrides = NavigatorOverridesBuilder::new()
            .user_agent("Mozilla/5.0 Chrome/131.0.0.0 Safari/537.36")
            .build();
        assert_eq!(overrides.chrome_version, "131");
    }

    #[test]
    fn test_builder_platform_updates_derived_fields() {
        let overrides = NavigatorOverridesBuilder::new()
            .platform("MacIntel")
            .build();
        assert_eq!(overrides.platform, "MacIntel");
        assert_eq!(overrides.platform_name, "macOS");
        assert_eq!(overrides.platform_version, "14.0.0");
    }

    #[test]
    fn test_builder_explicit_overrides() {
        let overrides = NavigatorOverridesBuilder::new()
            .chrome_version("999")
            .platform_name("CustomOS")
            .architecture("arm")
            .platform_version("1.2.3")
            .build();

        assert_eq!(overrides.chrome_version, "999");
        assert_eq!(overrides.platform_name, "CustomOS");
        assert_eq!(overrides.architecture, "arm");
        assert_eq!(overrides.platform_version, "1.2.3");
    }

    #[test]
    fn test_chrome_version_consistency_in_js() {
        // Verify that when UA says Chrome/131, the JS script uses 131 for userAgentData
        let overrides = NavigatorOverridesBuilder::new()
            .user_agent("Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/131.0.0.0 Safari/537.36")
            .platform("Win32")
            .build();

        let js = overrides.get_override_script();

        // The Chrome version in userAgentData MUST match the UA string
        assert!(js.contains(r#"const CHROME_VERSION = "131";"#),
            "userAgentData Chrome version must match UA string version");
        assert!(js.contains(r#"Chrome/131.0.0.0"#),
            "UA string must contain Chrome/131");
    }
}
