//! Canvas Fingerprint Protection
//!
//! This module provides canvas fingerprint protection by injecting controlled
//! noise into canvas operations. Canvas fingerprinting is a highly effective
//! tracking technique that exploits subtle differences in how browsers render
//! graphics on different hardware and software configurations.
//!
//! This module complements the WebGL spoofing in `webgl.rs` by protecting
//! the 2D canvas context specifically. While `webgl.rs` handles WebGL
//! parameter spoofing and basic canvas noise, this module provides
//! fine-grained control over canvas fingerprint protection.
//!
//! # Components
//!
//! - `CanvasConfig` - Configuration for canvas fingerprint protection
//! - Noise injection for `getImageData`, `toDataURL`, and `toBlob`
//! - Seed-based deterministic noise for consistency
//!
//! # Example
//!
//! ```rust,no_run
//! use ki_browser_standalone::stealth::canvas::CanvasConfig;
//!
//! // Use safe defaults
//! let config = CanvasConfig::default();
//!
//! // Or customize noise level
//! let config = CanvasConfig::new(0.02);
//!
//! // Get the JavaScript override script
//! let js = config.get_override_script();
//! ```

/// Canvas fingerprint protection configuration
///
/// Controls how noise is injected into canvas operations to prevent
/// fingerprinting while maintaining visual appearance.
#[derive(Debug, Clone)]
pub struct CanvasConfig {
    /// Enable canvas noise injection
    ///
    /// When false, no canvas modifications are applied.
    pub noise_enabled: bool,
    /// Noise intensity (0.0 - 1.0, recommended: 0.01 - 0.05)
    ///
    /// Higher values provide more protection but may cause visible
    /// artifacts. Values between 0.01 and 0.05 are recommended for
    /// a good balance between protection and visual quality.
    pub noise_level: f64,
    /// Protect `HTMLCanvasElement.toDataURL()`
    ///
    /// When true, noise is injected before toDataURL returns data,
    /// preventing consistent fingerprints from canvas rendering.
    pub protect_to_data_url: bool,
    /// Protect `HTMLCanvasElement.toBlob()`
    ///
    /// When true, noise is injected before toBlob creates the blob,
    /// preventing consistent fingerprints from canvas blob export.
    pub protect_to_blob: bool,
    /// Protect `CanvasRenderingContext2D.getImageData()`
    ///
    /// When true, noise is added to pixel data returned by getImageData,
    /// preventing fingerprinting through direct pixel inspection.
    pub protect_get_image_data: bool,
    /// Use deterministic (seed-based) noise
    ///
    /// When true, noise is generated from a session seed, making it
    /// consistent within a session but different across sessions.
    /// When false, noise is fully random on each call.
    pub deterministic: bool,
}

impl CanvasConfig {
    /// Create a new canvas configuration with the specified noise level
    ///
    /// All protections are enabled by default.
    pub fn new(noise_level: f64) -> Self {
        Self {
            noise_enabled: true,
            noise_level: noise_level.clamp(0.0, 1.0),
            protect_to_data_url: true,
            protect_to_blob: true,
            protect_get_image_data: true,
            deterministic: true,
        }
    }

    /// Create a disabled configuration (no canvas protection)
    pub fn disabled() -> Self {
        Self {
            noise_enabled: false,
            noise_level: 0.0,
            protect_to_data_url: false,
            protect_to_blob: false,
            protect_get_image_data: false,
            deterministic: true,
        }
    }

    /// Generate JavaScript override script for canvas fingerprint protection
    ///
    /// This script must be injected before any page scripts run.
    pub fn get_override_script(&self) -> String {
        if !self.noise_enabled {
            return String::new();
        }

        let noise_level = self.noise_level.clamp(0.0, 1.0);

        format!(
            r#"
// Canvas Fingerprint Protection
(function() {{
    'use strict';

    const NOISE_LEVEL = {noise_level};
    const DETERMINISTIC = {deterministic};

    // Session seed for deterministic noise
    const SESSION_SEED = Math.floor(Math.random() * 2147483647);

    // Deterministic pseudo-random number generator (mulberry32)
    function mulberry32(seed) {{
        return function() {{
            seed |= 0;
            seed = seed + 0x6D2B79F5 | 0;
            let t = Math.imul(seed ^ seed >>> 15, 1 | seed);
            t = t + Math.imul(t ^ t >>> 7, 61 | t) ^ t;
            return ((t ^ t >>> 14) >>> 0) / 4294967296;
        }};
    }}

    // Get noise value for a pixel
    function getNoise(index, seed) {{
        if (DETERMINISTIC) {{
            const rng = mulberry32(seed + index);
            return (rng() - 0.5) * 2 * NOISE_LEVEL * 255;
        }} else {{
            return (Math.random() - 0.5) * 2 * NOISE_LEVEL * 255;
        }}
    }}

    // Add noise to ImageData pixels
    function addNoiseToImageData(imageData, baseSeed) {{
        const data = imageData.data;
        const len = data.length;

        for (let i = 0; i < len; i += 4) {{
            // Skip fully transparent pixels
            if (data[i + 3] === 0) continue;

            const noise = getNoise(i, baseSeed);

            // Apply noise to RGB channels (not alpha)
            data[i]     = Math.max(0, Math.min(255, Math.round(data[i] + noise)));
            data[i + 1] = Math.max(0, Math.min(255, Math.round(data[i + 1] + noise)));
            data[i + 2] = Math.max(0, Math.min(255, Math.round(data[i + 2] + noise)));
        }}

        return imageData;
    }}

    {to_data_url_override}

    {to_blob_override}

    {get_image_data_override}

    // Also handle OffscreenCanvas if available
    if (typeof OffscreenCanvas !== 'undefined') {{
        const originalConvertToBlob = OffscreenCanvas.prototype.convertToBlob;
        if (originalConvertToBlob) {{
            OffscreenCanvas.prototype.convertToBlob = function(options) {{
                try {{
                    const ctx = this.getContext('2d');
                    if (ctx && this.width > 0 && this.height > 0) {{
                        const imageData = ctx.getImageData(0, 0, this.width, this.height);
                        addNoiseToImageData(imageData, SESSION_SEED);
                        ctx.putImageData(imageData, 0, 0);
                    }}
                }} catch (e) {{}}
                return originalConvertToBlob.call(this, options);
            }};
        }}
    }}

}})();
"#,
            noise_level = noise_level,
            deterministic = self.deterministic,
            to_data_url_override = if self.protect_to_data_url {
                Self::get_to_data_url_override()
            } else {
                String::new()
            },
            to_blob_override = if self.protect_to_blob {
                Self::get_to_blob_override()
            } else {
                String::new()
            },
            get_image_data_override = if self.protect_get_image_data {
                Self::get_image_data_override()
            } else {
                String::new()
            },
        )
    }

    /// Generate toDataURL override
    fn get_to_data_url_override() -> String {
        r#"
    // Override HTMLCanvasElement.toDataURL
    const originalToDataURL = HTMLCanvasElement.prototype.toDataURL;
    HTMLCanvasElement.prototype.toDataURL = function(type, quality) {
        try {
            const ctx = this.getContext('2d');
            if (ctx && this.width > 0 && this.height > 0) {
                const imageData = ctx.getImageData(0, 0, this.width, this.height);
                addNoiseToImageData(imageData, SESSION_SEED);
                ctx.putImageData(imageData, 0, 0);
            }
        } catch (e) {
            // Canvas might be tainted or context unavailable
        }
        return originalToDataURL.call(this, type, quality);
    };
"#
        .to_string()
    }

    /// Generate toBlob override
    fn get_to_blob_override() -> String {
        r#"
    // Override HTMLCanvasElement.toBlob
    const originalToBlob = HTMLCanvasElement.prototype.toBlob;
    HTMLCanvasElement.prototype.toBlob = function(callback, type, quality) {
        try {
            const ctx = this.getContext('2d');
            if (ctx && this.width > 0 && this.height > 0) {
                const imageData = ctx.getImageData(0, 0, this.width, this.height);
                addNoiseToImageData(imageData, SESSION_SEED);
                ctx.putImageData(imageData, 0, 0);
            }
        } catch (e) {
            // Canvas might be tainted or context unavailable
        }
        return originalToBlob.call(this, callback, type, quality);
    };
"#
        .to_string()
    }

    /// Generate getImageData override
    fn get_image_data_override() -> String {
        r#"
    // Override CanvasRenderingContext2D.getImageData
    const originalGetImageData = CanvasRenderingContext2D.prototype.getImageData;
    CanvasRenderingContext2D.prototype.getImageData = function(sx, sy, sw, sh, settings) {
        const imageData = originalGetImageData.call(this, sx, sy, sw, sh, settings);
        // Add noise keyed to position for consistency
        return addNoiseToImageData(imageData, SESSION_SEED + sx * 7 + sy * 13);
    };
"#
        .to_string()
    }
}

impl Default for CanvasConfig {
    fn default() -> Self {
        Self {
            noise_enabled: true,
            noise_level: 0.02,
            protect_to_data_url: true,
            protect_to_blob: true,
            protect_get_image_data: true,
            deterministic: true,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_config() {
        let config = CanvasConfig::default();
        assert!(config.noise_enabled);
        assert!((config.noise_level - 0.02).abs() < f64::EPSILON);
        assert!(config.protect_to_data_url);
        assert!(config.protect_to_blob);
        assert!(config.protect_get_image_data);
        assert!(config.deterministic);
    }

    #[test]
    fn test_disabled_config() {
        let config = CanvasConfig::disabled();
        assert!(!config.noise_enabled);
        let js = config.get_override_script();
        assert!(js.is_empty());
    }

    #[test]
    fn test_custom_noise_level() {
        let config = CanvasConfig::new(0.05);
        assert!((config.noise_level - 0.05).abs() < f64::EPSILON);
    }

    #[test]
    fn test_noise_level_clamping() {
        let config = CanvasConfig::new(5.0);
        assert!((config.noise_level - 1.0).abs() < f64::EPSILON);

        let config = CanvasConfig::new(-1.0);
        assert!((config.noise_level - 0.0).abs() < f64::EPSILON);
    }

    #[test]
    fn test_override_script_contains_protections() {
        let config = CanvasConfig::default();
        let js = config.get_override_script();

        assert!(js.contains("toDataURL"));
        assert!(js.contains("toBlob"));
        assert!(js.contains("getImageData"));
        assert!(js.contains("NOISE_LEVEL"));
        assert!(js.contains("addNoiseToImageData"));
        assert!(js.contains("mulberry32"));
    }

    #[test]
    fn test_selective_protections() {
        let mut config = CanvasConfig::default();
        config.protect_to_data_url = false;
        config.protect_to_blob = false;

        let js = config.get_override_script();

        // getImageData should still be protected
        assert!(js.contains("getImageData"));
        // toDataURL and toBlob overrides should not be present
        assert!(!js.contains("originalToDataURL"));
        assert!(!js.contains("originalToBlob"));
    }

    #[test]
    fn test_non_deterministic_mode() {
        let mut config = CanvasConfig::default();
        config.deterministic = false;

        let js = config.get_override_script();

        assert!(js.contains("DETERMINISTIC = false"));
    }

    #[test]
    fn test_script_is_iife() {
        let config = CanvasConfig::default();
        let js = config.get_override_script();

        assert!(js.contains("(function()"));
        assert!(js.contains("'use strict'"));
        assert!(js.contains("})();"));
    }
}
