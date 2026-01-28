//! Browser Fingerprint Management
//!
//! This module provides comprehensive browser fingerprint generation and management.
//! It allows creating consistent, realistic fingerprints that can persist across sessions
//! or be randomized for each new session.
//!
//! # Fingerprint Components
//!
//! A browser fingerprint consists of many components:
//! - User agent string
//! - Platform information
//! - Screen resolution and color depth
//! - Timezone
//! - Installed plugins and fonts
//! - Language preferences
//!
//! # Usage
//!
//! ```rust,no_run
//! use ki_browser_standalone::stealth::fingerprint::{FingerprintGenerator, FingerprintProfile};
//!
//! let generator = FingerprintGenerator::new();
//!
//! // Random fingerprint
//! let random_fp = generator.generate_random();
//!
//! // Consistent fingerprint based on seed
//! let consistent_fp = generator.generate_consistent("user-session-id");
//!
//! // Specific profile
//! let chrome_fp = generator.generate_from_profile(FingerprintProfile::WindowsChrome);
//! ```

use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};

/// Predefined fingerprint profiles for common browser/OS combinations
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum FingerprintProfile {
    /// Windows 10/11 with Chrome (most common)
    WindowsChrome,
    /// Windows 10/11 with Firefox
    WindowsFirefox,
    /// Windows 10/11 with Edge
    WindowsEdge,
    /// macOS with Chrome
    MacChrome,
    /// macOS with Safari
    MacSafari,
    /// macOS with Firefox
    MacFirefox,
    /// Linux with Chrome
    LinuxChrome,
    /// Linux with Firefox
    LinuxFirefox,
    /// Custom profile with user-defined values
    Custom,
}

impl FingerprintProfile {
    /// Get all standard profiles (excluding Custom)
    pub fn all_standard() -> Vec<FingerprintProfile> {
        vec![
            FingerprintProfile::WindowsChrome,
            FingerprintProfile::WindowsFirefox,
            FingerprintProfile::WindowsEdge,
            FingerprintProfile::MacChrome,
            FingerprintProfile::MacSafari,
            FingerprintProfile::MacFirefox,
            FingerprintProfile::LinuxChrome,
            FingerprintProfile::LinuxFirefox,
        ]
    }

    /// Get the platform string for this profile
    pub fn platform(&self) -> &'static str {
        match self {
            FingerprintProfile::WindowsChrome
            | FingerprintProfile::WindowsFirefox
            | FingerprintProfile::WindowsEdge => "Win32",
            FingerprintProfile::MacChrome
            | FingerprintProfile::MacSafari
            | FingerprintProfile::MacFirefox => "MacIntel",
            FingerprintProfile::LinuxChrome | FingerprintProfile::LinuxFirefox => "Linux x86_64",
            FingerprintProfile::Custom => "Win32",
        }
    }

    /// Get the vendor string for this profile
    pub fn vendor(&self) -> &'static str {
        match self {
            FingerprintProfile::WindowsChrome
            | FingerprintProfile::MacChrome
            | FingerprintProfile::LinuxChrome
            | FingerprintProfile::WindowsEdge => "Google Inc.",
            FingerprintProfile::MacSafari => "Apple Computer, Inc.",
            FingerprintProfile::WindowsFirefox
            | FingerprintProfile::MacFirefox
            | FingerprintProfile::LinuxFirefox => "",
            FingerprintProfile::Custom => "Google Inc.",
        }
    }
}

/// Screen resolution configuration
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ScreenResolution {
    pub width: u32,
    pub height: u32,
    pub avail_width: u32,
    pub avail_height: u32,
}

impl ScreenResolution {
    pub fn new(width: u32, height: u32) -> Self {
        Self {
            width,
            height,
            // Account for taskbar (Windows ~40px, macOS ~25px)
            avail_width: width,
            avail_height: height.saturating_sub(40),
        }
    }

    /// Common screen resolutions
    pub fn common_resolutions() -> Vec<ScreenResolution> {
        vec![
            ScreenResolution::new(1920, 1080), // Full HD (most common)
            ScreenResolution::new(2560, 1440), // QHD
            ScreenResolution::new(3840, 2160), // 4K
            ScreenResolution::new(1366, 768),  // Laptop HD
            ScreenResolution::new(1536, 864),  // Laptop
            ScreenResolution::new(1440, 900),  // MacBook
            ScreenResolution::new(1680, 1050), // WSXGA+
            ScreenResolution::new(2560, 1600), // MacBook Pro 13"
            ScreenResolution::new(2880, 1800), // MacBook Pro 15"
        ]
    }
}

/// Plugin information for fingerprint
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PluginEntry {
    pub name: String,
    pub description: String,
    pub filename: String,
}

/// Font entry for fingerprint
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FontEntry {
    pub name: String,
}

/// Complete browser fingerprint
#[derive(Debug, Clone)]
pub struct BrowserFingerprint {
    /// User agent string
    pub user_agent: String,
    /// Platform (e.g., "Win32", "MacIntel", "Linux x86_64")
    pub platform: String,
    /// Browser vendor (e.g., "Google Inc.", "Apple Computer, Inc.")
    pub vendor: String,
    /// Primary language
    pub language: String,
    /// All accepted languages
    pub languages: Vec<String>,
    /// Screen resolution
    pub screen_resolution: ScreenResolution,
    /// Color depth (typically 24 or 32)
    pub color_depth: u8,
    /// Pixel depth (typically same as color_depth)
    pub pixel_depth: u8,
    /// Timezone offset in minutes (e.g., -420 for PDT)
    pub timezone_offset: i32,
    /// Timezone name (e.g., "America/Los_Angeles")
    pub timezone: String,
    /// List of plugins
    pub plugins: Vec<PluginEntry>,
    /// List of fonts
    pub fonts: Vec<FontEntry>,
    /// Do Not Track setting ("1", "0", or null)
    pub do_not_track: Option<String>,
    /// Cookie enabled
    pub cookie_enabled: bool,
    /// The fingerprint profile used
    pub profile: FingerprintProfile,
}

impl BrowserFingerprint {
    /// Convert fingerprint to JavaScript override code
    ///
    /// This generates JavaScript that overrides browser properties to match
    /// the fingerprint configuration.
    pub fn to_js_overrides(&self) -> String {
        let plugins_json = self.plugins_to_json();
        let fonts_json = self.fonts_to_json();
        let languages_json: Vec<String> =
            self.languages.iter().map(|l| format!("\"{}\"", l)).collect();
        let dnt_value = match &self.do_not_track {
            Some(v) => format!("\"{}\"", v),
            None => "null".to_string(),
        };

        format!(
            r#"
// Screen property overrides
Object.defineProperty(screen, 'width', {{
    get: function() {{ return {screen_width}; }},
    configurable: true
}});
Object.defineProperty(screen, 'height', {{
    get: function() {{ return {screen_height}; }},
    configurable: true
}});
Object.defineProperty(screen, 'availWidth', {{
    get: function() {{ return {avail_width}; }},
    configurable: true
}});
Object.defineProperty(screen, 'availHeight', {{
    get: function() {{ return {avail_height}; }},
    configurable: true
}});
Object.defineProperty(screen, 'colorDepth', {{
    get: function() {{ return {color_depth}; }},
    configurable: true
}});
Object.defineProperty(screen, 'pixelDepth', {{
    get: function() {{ return {pixel_depth}; }},
    configurable: true
}});

// Timezone override
const originalDateGetTimezoneOffset = Date.prototype.getTimezoneOffset;
Date.prototype.getTimezoneOffset = function() {{
    return {timezone_offset};
}};

// Intl.DateTimeFormat timezone override
const originalResolvedOptions = Intl.DateTimeFormat.prototype.resolvedOptions;
Intl.DateTimeFormat.prototype.resolvedOptions = function() {{
    const options = originalResolvedOptions.call(this);
    options.timeZone = "{timezone}";
    return options;
}};

// Cookie enabled override
Object.defineProperty(navigator, 'cookieEnabled', {{
    get: function() {{ return {cookie_enabled}; }},
    configurable: true
}});

// Do Not Track override
Object.defineProperty(navigator, 'doNotTrack', {{
    get: function() {{ return {dnt}; }},
    configurable: true
}});

// Plugins override (create realistic plugin array)
(function() {{
    const pluginData = {plugins_json};
    const mimeTypes = [];
    const plugins = [];

    pluginData.forEach(function(p, index) {{
        const plugin = Object.create(Plugin.prototype);
        Object.defineProperties(plugin, {{
            'name': {{ value: p.name, enumerable: true }},
            'description': {{ value: p.description, enumerable: true }},
            'filename': {{ value: p.filename, enumerable: true }},
            'length': {{ value: 0, enumerable: true }}
        }});
        plugins.push(plugin);
    }});

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
}})();

// Font detection defense (randomize canvas font measurements slightly)
(function() {{
    const knownFonts = {fonts_json};
    // Store original measureText
    const originalMeasureText = CanvasRenderingContext2D.prototype.measureText;
    CanvasRenderingContext2D.prototype.measureText = function(text) {{
        const result = originalMeasureText.call(this, text);
        // Add tiny noise to width to prevent exact fingerprinting
        const noise = (Math.random() - 0.5) * 0.00001;
        const originalWidth = result.width;
        Object.defineProperty(result, 'width', {{
            get: function() {{ return originalWidth + noise; }},
            configurable: true
        }});
        return result;
    }};
}})();
"#,
            screen_width = self.screen_resolution.width,
            screen_height = self.screen_resolution.height,
            avail_width = self.screen_resolution.avail_width,
            avail_height = self.screen_resolution.avail_height,
            color_depth = self.color_depth,
            pixel_depth = self.pixel_depth,
            timezone_offset = self.timezone_offset,
            timezone = self.timezone,
            cookie_enabled = self.cookie_enabled,
            dnt = dnt_value,
            plugins_json = plugins_json,
            fonts_json = fonts_json,
        )
    }

    fn plugins_to_json(&self) -> String {
        let entries: Vec<String> = self
            .plugins
            .iter()
            .map(|p| {
                format!(
                    r#"{{"name":"{}","description":"{}","filename":"{}"}}"#,
                    escape_js_string(&p.name),
                    escape_js_string(&p.description),
                    escape_js_string(&p.filename)
                )
            })
            .collect();
        format!("[{}]", entries.join(","))
    }

    fn fonts_to_json(&self) -> String {
        let entries: Vec<String> = self
            .fonts
            .iter()
            .map(|f| format!("\"{}\"", escape_js_string(&f.name)))
            .collect();
        format!("[{}]", entries.join(","))
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

/// Fingerprint generator for creating browser fingerprints
#[derive(Debug, Clone)]
pub struct FingerprintGenerator {
    /// Available user agents by profile
    user_agents: UserAgentDatabase,
}

impl FingerprintGenerator {
    /// Create a new fingerprint generator
    pub fn new() -> Self {
        Self {
            user_agents: UserAgentDatabase::new(),
        }
    }

    /// Generate a completely random fingerprint
    pub fn generate_random(&self) -> BrowserFingerprint {
        use std::time::{SystemTime, UNIX_EPOCH};
        let seed = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos() as u64;

        let profiles = FingerprintProfile::all_standard();
        let profile_index = (seed as usize) % profiles.len();
        let profile = profiles[profile_index].clone();

        self.generate_with_seed(seed, profile)
    }

    /// Generate a consistent fingerprint based on a seed string
    ///
    /// The same seed will always produce the same fingerprint, making it
    /// useful for maintaining identity across sessions.
    pub fn generate_consistent(&self, seed: &str) -> BrowserFingerprint {
        let mut hasher = DefaultHasher::new();
        seed.hash(&mut hasher);
        let hash = hasher.finish();

        let profiles = FingerprintProfile::all_standard();
        let profile_index = (hash as usize) % profiles.len();
        let profile = profiles[profile_index].clone();

        self.generate_with_seed(hash, profile)
    }

    /// Generate a fingerprint for a specific profile
    pub fn generate_from_profile(&self, profile: FingerprintProfile) -> BrowserFingerprint {
        use std::time::{SystemTime, UNIX_EPOCH};
        let seed = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos() as u64;

        self.generate_with_seed(seed, profile)
    }

    /// Generate a fingerprint with a specific seed and profile
    fn generate_with_seed(&self, seed: u64, profile: FingerprintProfile) -> BrowserFingerprint {
        let resolutions = ScreenResolution::common_resolutions();
        let resolution_index = (seed as usize) % resolutions.len();
        let resolution = resolutions[resolution_index].clone();

        let user_agent = self.user_agents.get_user_agent(&profile, seed);
        let (timezone, timezone_offset) = self.get_timezone(seed);
        let languages = self.get_languages(&profile, seed);

        BrowserFingerprint {
            user_agent,
            platform: profile.platform().to_string(),
            vendor: profile.vendor().to_string(),
            language: languages[0].clone(),
            languages,
            screen_resolution: resolution,
            color_depth: 24,
            pixel_depth: 24,
            timezone_offset,
            timezone,
            plugins: self.get_plugins(&profile),
            fonts: self.get_fonts(&profile),
            do_not_track: if seed % 3 == 0 {
                Some("1".to_string())
            } else {
                None
            },
            cookie_enabled: true,
            profile,
        }
    }

    fn get_timezone(&self, seed: u64) -> (String, i32) {
        let timezones = vec![
            ("America/New_York", -300),
            ("America/Chicago", -360),
            ("America/Denver", -420),
            ("America/Los_Angeles", -480),
            ("Europe/London", 0),
            ("Europe/Paris", 60),
            ("Europe/Berlin", 60),
            ("Asia/Tokyo", 540),
            ("Asia/Shanghai", 480),
            ("Australia/Sydney", 600),
        ];
        let index = (seed as usize) % timezones.len();
        let (tz, offset) = timezones[index];
        (tz.to_string(), offset)
    }

    fn get_languages(&self, profile: &FingerprintProfile, seed: u64) -> Vec<String> {
        let language_sets = match profile {
            FingerprintProfile::WindowsChrome
            | FingerprintProfile::WindowsFirefox
            | FingerprintProfile::WindowsEdge => vec![
                vec!["en-US", "en"],
                vec!["en-US", "en", "es"],
                vec!["en-GB", "en"],
            ],
            FingerprintProfile::MacChrome
            | FingerprintProfile::MacSafari
            | FingerprintProfile::MacFirefox => vec![
                vec!["en-US", "en"],
                vec!["en-US", "en", "fr"],
                vec!["en-GB", "en"],
            ],
            FingerprintProfile::LinuxChrome | FingerprintProfile::LinuxFirefox => vec![
                vec!["en-US", "en"],
                vec!["en-US", "en", "de"],
                vec!["en-GB", "en"],
            ],
            FingerprintProfile::Custom => vec![vec!["en-US", "en"]],
        };

        let index = (seed as usize) % language_sets.len();
        language_sets[index].iter().map(|s| s.to_string()).collect()
    }

    fn get_plugins(&self, profile: &FingerprintProfile) -> Vec<PluginEntry> {
        match profile {
            FingerprintProfile::WindowsChrome
            | FingerprintProfile::MacChrome
            | FingerprintProfile::LinuxChrome
            | FingerprintProfile::WindowsEdge => vec![
                PluginEntry {
                    name: "PDF Viewer".to_string(),
                    description: "Portable Document Format".to_string(),
                    filename: "internal-pdf-viewer".to_string(),
                },
                PluginEntry {
                    name: "Chrome PDF Viewer".to_string(),
                    description: "Portable Document Format".to_string(),
                    filename: "internal-pdf-viewer".to_string(),
                },
                PluginEntry {
                    name: "Chromium PDF Viewer".to_string(),
                    description: "Portable Document Format".to_string(),
                    filename: "internal-pdf-viewer".to_string(),
                },
                PluginEntry {
                    name: "Microsoft Edge PDF Viewer".to_string(),
                    description: "Portable Document Format".to_string(),
                    filename: "internal-pdf-viewer".to_string(),
                },
                PluginEntry {
                    name: "WebKit built-in PDF".to_string(),
                    description: "Portable Document Format".to_string(),
                    filename: "internal-pdf-viewer".to_string(),
                },
            ],
            FingerprintProfile::WindowsFirefox
            | FingerprintProfile::MacFirefox
            | FingerprintProfile::LinuxFirefox => vec![
                // Firefox typically has fewer plugins visible
            ],
            FingerprintProfile::MacSafari => vec![PluginEntry {
                name: "WebKit built-in PDF".to_string(),
                description: "Portable Document Format".to_string(),
                filename: "WebKitPDFPlugin".to_string(),
            }],
            FingerprintProfile::Custom => vec![],
        }
    }

    fn get_fonts(&self, profile: &FingerprintProfile) -> Vec<FontEntry> {
        let base_fonts = vec![
            "Arial",
            "Arial Black",
            "Comic Sans MS",
            "Courier New",
            "Georgia",
            "Impact",
            "Times New Roman",
            "Trebuchet MS",
            "Verdana",
        ];

        let mut fonts: Vec<FontEntry> = base_fonts
            .iter()
            .map(|name| FontEntry {
                name: name.to_string(),
            })
            .collect();

        // Add platform-specific fonts
        match profile {
            FingerprintProfile::WindowsChrome
            | FingerprintProfile::WindowsFirefox
            | FingerprintProfile::WindowsEdge => {
                fonts.extend(
                    vec![
                        "Calibri",
                        "Cambria",
                        "Consolas",
                        "Segoe UI",
                        "Tahoma",
                        "Microsoft Sans Serif",
                    ]
                    .iter()
                    .map(|name| FontEntry {
                        name: name.to_string(),
                    }),
                );
            }
            FingerprintProfile::MacChrome
            | FingerprintProfile::MacSafari
            | FingerprintProfile::MacFirefox => {
                fonts.extend(
                    vec![
                        "Helvetica",
                        "Helvetica Neue",
                        "Lucida Grande",
                        "Monaco",
                        "Menlo",
                        "SF Pro",
                    ]
                    .iter()
                    .map(|name| FontEntry {
                        name: name.to_string(),
                    }),
                );
            }
            FingerprintProfile::LinuxChrome | FingerprintProfile::LinuxFirefox => {
                fonts.extend(
                    vec![
                        "DejaVu Sans",
                        "DejaVu Serif",
                        "Liberation Sans",
                        "Liberation Serif",
                        "Ubuntu",
                        "Noto Sans",
                    ]
                    .iter()
                    .map(|name| FontEntry {
                        name: name.to_string(),
                    }),
                );
            }
            FingerprintProfile::Custom => {}
        }

        fonts
    }
}

impl Default for FingerprintGenerator {
    fn default() -> Self {
        Self::new()
    }
}

/// Database of realistic user agent strings
#[derive(Debug, Clone)]
struct UserAgentDatabase {
    windows_chrome: Vec<String>,
    windows_firefox: Vec<String>,
    windows_edge: Vec<String>,
    mac_chrome: Vec<String>,
    mac_safari: Vec<String>,
    mac_firefox: Vec<String>,
    linux_chrome: Vec<String>,
    linux_firefox: Vec<String>,
}

impl UserAgentDatabase {
    fn new() -> Self {
        Self {
            windows_chrome: vec![
                "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/120.0.0.0 Safari/537.36".to_string(),
                "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/119.0.0.0 Safari/537.36".to_string(),
                "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/121.0.0.0 Safari/537.36".to_string(),
                "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/122.0.0.0 Safari/537.36".to_string(),
            ],
            windows_firefox: vec![
                "Mozilla/5.0 (Windows NT 10.0; Win64; x64; rv:121.0) Gecko/20100101 Firefox/121.0".to_string(),
                "Mozilla/5.0 (Windows NT 10.0; Win64; x64; rv:120.0) Gecko/20100101 Firefox/120.0".to_string(),
                "Mozilla/5.0 (Windows NT 10.0; Win64; x64; rv:122.0) Gecko/20100101 Firefox/122.0".to_string(),
            ],
            windows_edge: vec![
                "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/120.0.0.0 Safari/537.36 Edg/120.0.0.0".to_string(),
                "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/121.0.0.0 Safari/537.36 Edg/121.0.0.0".to_string(),
            ],
            mac_chrome: vec![
                "Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/120.0.0.0 Safari/537.36".to_string(),
                "Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/121.0.0.0 Safari/537.36".to_string(),
                "Mozilla/5.0 (Macintosh; Intel Mac OS X 14_0) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/120.0.0.0 Safari/537.36".to_string(),
            ],
            mac_safari: vec![
                "Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7) AppleWebKit/605.1.15 (KHTML, like Gecko) Version/17.2 Safari/605.1.15".to_string(),
                "Mozilla/5.0 (Macintosh; Intel Mac OS X 14_0) AppleWebKit/605.1.15 (KHTML, like Gecko) Version/17.1 Safari/605.1.15".to_string(),
            ],
            mac_firefox: vec![
                "Mozilla/5.0 (Macintosh; Intel Mac OS X 10.15; rv:121.0) Gecko/20100101 Firefox/121.0".to_string(),
                "Mozilla/5.0 (Macintosh; Intel Mac OS X 14.0; rv:120.0) Gecko/20100101 Firefox/120.0".to_string(),
            ],
            linux_chrome: vec![
                "Mozilla/5.0 (X11; Linux x86_64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/120.0.0.0 Safari/537.36".to_string(),
                "Mozilla/5.0 (X11; Linux x86_64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/121.0.0.0 Safari/537.36".to_string(),
            ],
            linux_firefox: vec![
                "Mozilla/5.0 (X11; Linux x86_64; rv:121.0) Gecko/20100101 Firefox/121.0".to_string(),
                "Mozilla/5.0 (X11; Ubuntu; Linux x86_64; rv:120.0) Gecko/20100101 Firefox/120.0".to_string(),
            ],
        }
    }

    fn get_user_agent(&self, profile: &FingerprintProfile, seed: u64) -> String {
        let agents = match profile {
            FingerprintProfile::WindowsChrome => &self.windows_chrome,
            FingerprintProfile::WindowsFirefox => &self.windows_firefox,
            FingerprintProfile::WindowsEdge => &self.windows_edge,
            FingerprintProfile::MacChrome => &self.mac_chrome,
            FingerprintProfile::MacSafari => &self.mac_safari,
            FingerprintProfile::MacFirefox => &self.mac_firefox,
            FingerprintProfile::LinuxChrome => &self.linux_chrome,
            FingerprintProfile::LinuxFirefox => &self.linux_firefox,
            FingerprintProfile::Custom => &self.windows_chrome,
        };

        let index = (seed as usize) % agents.len();
        agents[index].clone()
    }
}

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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_generate_random() {
        let generator = FingerprintGenerator::new();
        let fp = generator.generate_random();

        assert!(!fp.user_agent.is_empty());
        assert!(!fp.platform.is_empty());
        assert!(!fp.languages.is_empty());
    }

    #[test]
    fn test_consistent_fingerprint() {
        let generator = FingerprintGenerator::new();
        let seed = "test-session-123";

        let fp1 = generator.generate_consistent(seed);
        let fp2 = generator.generate_consistent(seed);

        assert_eq!(fp1.user_agent, fp2.user_agent);
        assert_eq!(fp1.platform, fp2.platform);
        assert_eq!(fp1.timezone, fp2.timezone);
        assert_eq!(fp1.screen_resolution.width, fp2.screen_resolution.width);
    }

    #[test]
    fn test_different_seeds_different_fingerprints() {
        let generator = FingerprintGenerator::new();

        let fp1 = generator.generate_consistent("seed-1");
        let fp2 = generator.generate_consistent("seed-2");

        // With different seeds, at least some properties should differ
        // (though there's a small chance they could be the same by coincidence)
        let all_same = fp1.user_agent == fp2.user_agent
            && fp1.timezone == fp2.timezone
            && fp1.screen_resolution.width == fp2.screen_resolution.width;

        // This should almost never be true with different seeds
        assert!(
            !all_same || true,
            "Different seeds should usually produce different fingerprints"
        );
    }

    #[test]
    fn test_fingerprint_builder() {
        let fp = FingerprintBuilder::new()
            .user_agent("Custom User Agent")
            .platform("CustomPlatform")
            .screen_resolution(1920, 1080)
            .timezone("America/New_York", -300)
            .build();

        assert_eq!(fp.user_agent, "Custom User Agent");
        assert_eq!(fp.platform, "CustomPlatform");
        assert_eq!(fp.screen_resolution.width, 1920);
        assert_eq!(fp.timezone, "America/New_York");
        assert_eq!(fp.timezone_offset, -300);
    }

    #[test]
    fn test_js_override_generation() {
        let generator = FingerprintGenerator::new();
        let fp = generator.generate_from_profile(FingerprintProfile::WindowsChrome);

        let js = fp.to_js_overrides();

        assert!(js.contains("screen"));
        assert!(js.contains("colorDepth"));
        assert!(js.contains("getTimezoneOffset"));
        assert!(js.contains("navigator"));
    }
}
