//! JavaScript override script generation for navigator property spoofing.
//!
//! Generates comprehensive JavaScript code that overrides all navigator properties
//! and prevents detection of browser automation. The generated script MUST be
//! injected before any page scripts run.

use super::helpers::{
    escape_js_string, get_automation_removal_script, get_permissions_spoof_script,
};
use super::types::NavigatorOverrides;

impl NavigatorOverrides {
    /// Generate JavaScript override script for all navigator properties.
    ///
    /// Produces a self-contained IIFE that overrides `navigator.webdriver`,
    /// user agent strings, platform info, language settings, plugins,
    /// and optionally removes automation signals.
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

    // Remove the property entirely (puppeteer-extra-stealth approach).
    // Real non-automated Chrome exposes `navigator.webdriver` as a getter on
    // Navigator.prototype; automation flips it to true. Bot tests (sannysoft
    // "WebDriver (New)") flag the AUTOMATION-typical shapes: an own data
    // property on the navigator INSTANCE, or a faked getOwnPropertyDescriptor.
    // Deleting it from the prototype makes `navigator.webdriver === undefined`
    // and leaves no instance own-property — the cleanest pass. We deliberately
    // do NOT add an instance property and do NOT patch getOwnPropertyDescriptor
    // (that patch previously made the descriptor look like a data property,
    // which is itself the detected anomaly).
    try {{
        delete Navigator.prototype.webdriver;
    }} catch (e) {{}}
    try {{
        // If a stray own-property exists on the instance (e.g. injected by the
        // automation layer), drop it so no own descriptor remains.
        delete navigator.webdriver;
    }} catch (e) {{}}

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

    // Double-check webdriver is not truthy. After the delete above it should be
    // undefined (no own property, no prototype getter) — which passes. Only a
    // truthy value (automation still leaking through) needs a fallback, and we
    // fix it on the prototype as a getter, never as an instance data property.
    if (navigator.webdriver) {{
        console.error('CRITICAL: navigator.webdriver still truthy!');
        try {{
            delete Navigator.prototype.webdriver;
            delete navigator.webdriver;
        }} catch (e) {{}}
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

    /// Serialize languages list to a JSON array string for JavaScript injection
    fn languages_to_json(&self) -> String {
        let entries: Vec<String> = self
            .languages
            .iter()
            .map(|l| format!("\"{}\"", escape_js_string(l)))
            .collect();
        format!("[{}]", entries.join(", "))
    }

    /// Serialize plugins list to a JSON array string for JavaScript injection
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_js_override_contains_webdriver() {
        let overrides = NavigatorOverrides::default();
        let js = overrides.get_override_script();

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
}
