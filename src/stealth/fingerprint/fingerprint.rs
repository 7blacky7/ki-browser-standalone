//! BrowserFingerprint struct and JavaScript override generation.
//!
//! Contains the complete browser fingerprint data structure and the logic to
//! convert it into JavaScript property overrides for anti-detection injection.
//! Covers screen, timezone, cookie, DNT, plugin, and font fingerprinting.

use super::types::{
    FontEntry, FingerprintProfile, PluginEntry, ScreenResolution,
};

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
    /// Synchronize screen resolution to match the actual viewport dimensions.
    ///
    /// This ensures consistency between screen, outerWidth/Height, and innerWidth/Height:
    /// - screen.width >= outerWidth >= innerWidth (viewport)
    /// - screen.height >= outerHeight >= innerHeight (viewport)
    /// - orientation matches the screen dimensions
    ///
    /// The screen resolution is chosen from common resolutions that are >= the viewport.
    /// outerWidth/Height are calculated as viewport + typical browser chrome offsets.
    pub fn sync_screen_to_viewport(&mut self, viewport_width: u32, viewport_height: u32) {
        // Browser chrome offsets (typical values):
        // outerWidth = viewport + scrollbar + window border (~16px)
        // outerHeight = viewport + toolbar + tabs + borders (~85px)
        let outer_width = viewport_width + 16;
        let outer_height = viewport_height + 85;

        // Find a common resolution where width >= outer_width AND height >= outer_height
        let resolutions = ScreenResolution::common_resolutions();
        let suitable: Vec<&ScreenResolution> = resolutions
            .iter()
            .filter(|r| r.width >= outer_width && r.height >= outer_height)
            .collect();

        let screen_res = if let Some(res) = suitable.first() {
            // Pick the smallest suitable resolution (most common/realistic)
            let mut best = *res;
            for r in &suitable {
                if r.width * r.height < best.width * best.height {
                    best = r;
                }
            }
            ScreenResolution::new(best.width, best.height)
        } else {
            // Fallback: no common resolution fits, use the largest available
            // or create one that just fits
            resolutions
                .iter()
                .max_by_key(|r| r.width * r.height)
                .cloned()
                .unwrap_or_else(|| ScreenResolution::new(1920, 1080))
        };

        // Determine orientation from the SCREEN dimensions
        let orientation_type = if screen_res.width >= screen_res.height {
            "landscape-primary".to_string()
        } else {
            "portrait-primary".to_string()
        };
        let orientation_angle = if screen_res.width >= screen_res.height {
            0
        } else {
            90
        };

        self.screen_resolution = ScreenResolution {
            width: screen_res.width,
            height: screen_res.height,
            avail_width: screen_res.width,
            avail_height: screen_res.height.saturating_sub(40),
            outer_width,
            outer_height,
            orientation_type,
            orientation_angle,
        };
    }

    /// Convert fingerprint to JavaScript override code
    ///
    /// This generates JavaScript that overrides browser properties to match
    /// the fingerprint configuration.
    pub fn to_js_overrides(&self) -> String {
        let plugins_json = self.plugins_to_json();
        let fonts_json = self.fonts_to_json();
        let _languages_json: Vec<String> =
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

// Screen orientation override
if (screen.orientation) {{
    Object.defineProperty(screen.orientation, 'type', {{
        get: function() {{ return '{orientation_type}'; }},
        configurable: true
    }});
    Object.defineProperty(screen.orientation, 'angle', {{
        get: function() {{ return {orientation_angle}; }},
        configurable: true
    }});
}}

// outerWidth/Height consistent with screen (viewport + browser chrome)
Object.defineProperty(window, 'outerWidth', {{
    get: function() {{ return {outer_width}; }},
    configurable: true
}});
Object.defineProperty(window, 'outerHeight', {{
    get: function() {{ return {outer_height}; }},
    configurable: true
}});
// Mark that fingerprint script has applied outerWidth/Height
// so the chromium_engine fallback does not overwrite these values.
window.__fp_outer_applied = true;

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
            orientation_type = self.screen_resolution.orientation_type,
            orientation_angle = self.screen_resolution.orientation_angle,
            outer_width = self.screen_resolution.outer_width,
            outer_height = self.screen_resolution.outer_height,
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
