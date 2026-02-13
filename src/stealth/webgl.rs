//! WebGL Fingerprint Spoofing
//!
//! This module provides WebGL fingerprint spoofing capabilities to prevent
//! browser fingerprinting through WebGL renderer and vendor strings.
//!
//! WebGL fingerprinting is one of the most effective techniques for tracking users
//! because the GPU renderer string is highly unique. This module allows spoofing
//! these values to common configurations.
//!
//! # Components
//!
//! - `WebGLConfig` - Configuration for WebGL spoofing
//! - `WebGLProfile` - Predefined GPU profiles
//! - Canvas noise injection for additional protection
//!
//! # Example
//!
//! ```rust,no_run
//! use ki_browser_standalone::stealth::webgl::{WebGLConfig, WebGLProfile};
//!
//! // Use a predefined profile
//! let config = WebGLConfig::nvidia_gtx_1080();
//!
//! // Or create a random configuration
//! let random_config = WebGLConfig::random();
//!
//! // Get the JavaScript override script
//! let js = config.get_js_override_script();
//! ```

use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};

/// Predefined WebGL/GPU profiles
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum WebGLProfile {
    // NVIDIA profiles
    NvidiaGtx1080,
    NvidiaGtx1660,
    NvidiaRtx3060,
    NvidiaRtx3080,
    NvidiaRtx4070,
    NvidiaRtx4090,

    // AMD profiles
    AmdRx580,
    AmdRx6700Xt,
    AmdRx7900Xt,

    // Intel integrated graphics
    IntelUhd620,
    IntelUhd630,
    IntelUhd770,
    IntelIrisXe,
    IntelArcA770,

    // Apple Silicon
    AppleM1,
    AppleM2,
    AppleM3,

    // Generic/Software
    SwiftShader,
    AngleDirect3D11,
}

impl WebGLProfile {
    /// Get all available profiles
    pub fn all() -> Vec<WebGLProfile> {
        vec![
            WebGLProfile::NvidiaGtx1080,
            WebGLProfile::NvidiaGtx1660,
            WebGLProfile::NvidiaRtx3060,
            WebGLProfile::NvidiaRtx3080,
            WebGLProfile::NvidiaRtx4070,
            WebGLProfile::NvidiaRtx4090,
            WebGLProfile::AmdRx580,
            WebGLProfile::AmdRx6700Xt,
            WebGLProfile::AmdRx7900Xt,
            WebGLProfile::IntelUhd620,
            WebGLProfile::IntelUhd630,
            WebGLProfile::IntelUhd770,
            WebGLProfile::IntelIrisXe,
            WebGLProfile::IntelArcA770,
            WebGLProfile::AppleM1,
            WebGLProfile::AppleM2,
            WebGLProfile::AppleM3,
            WebGLProfile::SwiftShader,
            WebGLProfile::AngleDirect3D11,
        ]
    }

    /// Get common desktop profiles (most likely to be seen)
    pub fn common_desktop() -> Vec<WebGLProfile> {
        vec![
            WebGLProfile::NvidiaGtx1660,
            WebGLProfile::NvidiaRtx3060,
            WebGLProfile::NvidiaRtx3080,
            WebGLProfile::AmdRx6700Xt,
            WebGLProfile::IntelUhd630,
            WebGLProfile::IntelIrisXe,
        ]
    }

    /// Get the vendor string for this profile
    pub fn vendor(&self) -> &'static str {
        match self {
            WebGLProfile::NvidiaGtx1080
            | WebGLProfile::NvidiaGtx1660
            | WebGLProfile::NvidiaRtx3060
            | WebGLProfile::NvidiaRtx3080
            | WebGLProfile::NvidiaRtx4070
            | WebGLProfile::NvidiaRtx4090 => "NVIDIA Corporation",

            WebGLProfile::AmdRx580 | WebGLProfile::AmdRx6700Xt | WebGLProfile::AmdRx7900Xt => {
                "AMD"
            }

            WebGLProfile::IntelUhd620
            | WebGLProfile::IntelUhd630
            | WebGLProfile::IntelUhd770
            | WebGLProfile::IntelIrisXe
            | WebGLProfile::IntelArcA770 => "Intel Inc.",

            WebGLProfile::AppleM1 | WebGLProfile::AppleM2 | WebGLProfile::AppleM3 => "Apple Inc.",

            WebGLProfile::SwiftShader => "Google Inc. (Google)",
            WebGLProfile::AngleDirect3D11 => "Google Inc. (NVIDIA)",
        }
    }

    /// Get the renderer string for this profile
    pub fn renderer(&self) -> &'static str {
        match self {
            WebGLProfile::NvidiaGtx1080 => {
                "ANGLE (NVIDIA, NVIDIA GeForce GTX 1080 Direct3D11 vs_5_0 ps_5_0, D3D11)"
            }
            WebGLProfile::NvidiaGtx1660 => {
                "ANGLE (NVIDIA, NVIDIA GeForce GTX 1660 SUPER Direct3D11 vs_5_0 ps_5_0, D3D11)"
            }
            WebGLProfile::NvidiaRtx3060 => {
                "ANGLE (NVIDIA, NVIDIA GeForce RTX 3060 Direct3D11 vs_5_0 ps_5_0, D3D11)"
            }
            WebGLProfile::NvidiaRtx3080 => {
                "ANGLE (NVIDIA, NVIDIA GeForce RTX 3080 Direct3D11 vs_5_0 ps_5_0, D3D11)"
            }
            WebGLProfile::NvidiaRtx4070 => {
                "ANGLE (NVIDIA, NVIDIA GeForce RTX 4070 Direct3D11 vs_5_0 ps_5_0, D3D11)"
            }
            WebGLProfile::NvidiaRtx4090 => {
                "ANGLE (NVIDIA, NVIDIA GeForce RTX 4090 Direct3D11 vs_5_0 ps_5_0, D3D11)"
            }

            WebGLProfile::AmdRx580 => {
                "ANGLE (AMD, AMD Radeon RX 580 Series Direct3D11 vs_5_0 ps_5_0, D3D11)"
            }
            WebGLProfile::AmdRx6700Xt => {
                "ANGLE (AMD, AMD Radeon RX 6700 XT Direct3D11 vs_5_0 ps_5_0, D3D11)"
            }
            WebGLProfile::AmdRx7900Xt => {
                "ANGLE (AMD, AMD Radeon RX 7900 XT Direct3D11 vs_5_0 ps_5_0, D3D11)"
            }

            WebGLProfile::IntelUhd620 => {
                "ANGLE (Intel, Intel(R) UHD Graphics 620 Direct3D11 vs_5_0 ps_5_0, D3D11)"
            }
            WebGLProfile::IntelUhd630 => {
                "ANGLE (Intel, Intel(R) UHD Graphics 630 Direct3D11 vs_5_0 ps_5_0, D3D11)"
            }
            WebGLProfile::IntelUhd770 => {
                "ANGLE (Intel, Intel(R) UHD Graphics 770 Direct3D11 vs_5_0 ps_5_0, D3D11)"
            }
            WebGLProfile::IntelIrisXe => {
                "ANGLE (Intel, Intel(R) Iris(R) Xe Graphics Direct3D11 vs_5_0 ps_5_0, D3D11)"
            }
            WebGLProfile::IntelArcA770 => {
                "ANGLE (Intel, Intel(R) Arc(TM) A770 Graphics Direct3D11 vs_5_0 ps_5_0, D3D11)"
            }

            WebGLProfile::AppleM1 => "Apple M1",
            WebGLProfile::AppleM2 => "Apple M2",
            WebGLProfile::AppleM3 => "Apple M3",

            WebGLProfile::SwiftShader => {
                "ANGLE (Google, Vulkan 1.1.0 (SwiftShader Device (Subzero) (0x0000C0DE)), SwiftShader driver)"
            }
            WebGLProfile::AngleDirect3D11 => {
                "ANGLE (NVIDIA, NVIDIA GeForce GTX 1060 6GB Direct3D11 vs_5_0 ps_5_0, D3D11)"
            }
        }
    }
}

/// WebGL configuration for fingerprint spoofing
#[derive(Debug, Clone)]
pub struct WebGLConfig {
    /// WebGL vendor string (WEBGL_debug_renderer_info extension)
    pub vendor: String,
    /// WebGL renderer string (WEBGL_debug_renderer_info extension)
    pub renderer: String,
    /// WebGL version string
    pub version: String,
    /// Shading language version
    pub shading_language_version: String,
    /// Maximum texture size
    pub max_texture_size: u32,
    /// Maximum viewport dimensions
    pub max_viewport_dims: (u32, u32),
    /// Maximum vertex attributes
    pub max_vertex_attribs: u32,
    /// Maximum varying vectors
    pub max_varying_vectors: u32,
    /// Maximum vertex uniform vectors
    pub max_vertex_uniform_vectors: u32,
    /// Maximum fragment uniform vectors
    pub max_fragment_uniform_vectors: u32,
    /// Enable canvas noise injection
    pub enable_canvas_noise: bool,
    /// Noise intensity (0.0 - 1.0, recommended: 0.0001 - 0.001)
    pub canvas_noise_intensity: f64,
    /// The profile used (if any)
    pub profile: Option<WebGLProfile>,
}

impl WebGLConfig {
    /// Create a new WebGL configuration from a profile
    pub fn from_profile(profile: WebGLProfile) -> Self {
        let (max_texture, max_viewport, max_vertex_attribs) = match &profile {
            // High-end NVIDIA
            WebGLProfile::NvidiaRtx3080
            | WebGLProfile::NvidiaRtx4070
            | WebGLProfile::NvidiaRtx4090 => (16384, (32767, 32767), 16),
            // Mid-range NVIDIA
            WebGLProfile::NvidiaGtx1080
            | WebGLProfile::NvidiaGtx1660
            | WebGLProfile::NvidiaRtx3060 => (16384, (32767, 32767), 16),
            // AMD cards
            WebGLProfile::AmdRx580 | WebGLProfile::AmdRx6700Xt | WebGLProfile::AmdRx7900Xt => {
                (16384, (16384, 16384), 16)
            }
            // Intel integrated
            WebGLProfile::IntelUhd620 | WebGLProfile::IntelUhd630 | WebGLProfile::IntelUhd770 => {
                (16384, (16384, 16384), 16)
            }
            WebGLProfile::IntelIrisXe | WebGLProfile::IntelArcA770 => (16384, (16384, 16384), 16),
            // Apple Silicon
            WebGLProfile::AppleM1 | WebGLProfile::AppleM2 | WebGLProfile::AppleM3 => {
                (16384, (16384, 16384), 16)
            }
            // Software renderers
            WebGLProfile::SwiftShader | WebGLProfile::AngleDirect3D11 => {
                (8192, (8192, 8192), 16)
            }
        };

        Self {
            vendor: profile.vendor().to_string(),
            renderer: profile.renderer().to_string(),
            version: "WebGL 1.0 (OpenGL ES 2.0 Chromium)".to_string(),
            shading_language_version: "WebGL GLSL ES 1.0 (OpenGL ES GLSL ES 1.0 Chromium)"
                .to_string(),
            max_texture_size: max_texture,
            max_viewport_dims: max_viewport,
            max_vertex_attribs,
            max_varying_vectors: 30,
            max_vertex_uniform_vectors: 4096,
            max_fragment_uniform_vectors: 1024,
            enable_canvas_noise: true,
            canvas_noise_intensity: 0.0001,
            profile: Some(profile),
        }
    }

    // Predefined configurations for common GPUs

    /// NVIDIA GeForce GTX 1080 configuration
    pub fn nvidia_gtx_1080() -> Self {
        Self::from_profile(WebGLProfile::NvidiaGtx1080)
    }

    /// NVIDIA GeForce GTX 1660 configuration
    pub fn nvidia_gtx_1660() -> Self {
        Self::from_profile(WebGLProfile::NvidiaGtx1660)
    }

    /// NVIDIA GeForce RTX 3060 configuration
    pub fn nvidia_rtx_3060() -> Self {
        Self::from_profile(WebGLProfile::NvidiaRtx3060)
    }

    /// NVIDIA GeForce RTX 3080 configuration
    pub fn nvidia_rtx_3080() -> Self {
        Self::from_profile(WebGLProfile::NvidiaRtx3080)
    }

    /// NVIDIA GeForce RTX 4090 configuration
    pub fn nvidia_rtx_4090() -> Self {
        Self::from_profile(WebGLProfile::NvidiaRtx4090)
    }

    /// AMD Radeon RX 580 configuration
    pub fn amd_rx_580() -> Self {
        Self::from_profile(WebGLProfile::AmdRx580)
    }

    /// AMD Radeon RX 6700 XT configuration
    pub fn amd_rx_6700_xt() -> Self {
        Self::from_profile(WebGLProfile::AmdRx6700Xt)
    }

    /// Intel UHD 630 configuration
    pub fn intel_uhd_630() -> Self {
        Self::from_profile(WebGLProfile::IntelUhd630)
    }

    /// Intel Iris Xe configuration
    pub fn intel_iris_xe() -> Self {
        Self::from_profile(WebGLProfile::IntelIrisXe)
    }

    /// Apple M1 configuration
    pub fn apple_m1() -> Self {
        Self::from_profile(WebGLProfile::AppleM1)
    }

    /// Apple M2 configuration
    pub fn apple_m2() -> Self {
        Self::from_profile(WebGLProfile::AppleM2)
    }

    /// Generate a random WebGL configuration
    pub fn random() -> Self {
        use std::time::{SystemTime, UNIX_EPOCH};
        let seed = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos() as u64;

        let profiles = WebGLProfile::common_desktop();
        let index = (seed as usize) % profiles.len();
        Self::from_profile(profiles[index].clone())
    }

    /// Generate a consistent WebGL configuration based on a seed
    pub fn consistent(seed: &str) -> Self {
        let mut hasher = DefaultHasher::new();
        seed.hash(&mut hasher);
        let hash = hasher.finish();

        let profiles = WebGLProfile::common_desktop();
        let index = (hash as usize) % profiles.len();
        Self::from_profile(profiles[index].clone())
    }

    /// Get a WebGL configuration appropriate for a fingerprint profile
    pub fn for_profile(fp_profile: &crate::stealth::FingerprintProfile) -> Self {
        use crate::stealth::FingerprintProfile;

        match fp_profile {
            FingerprintProfile::WindowsChrome
            | FingerprintProfile::WindowsFirefox
            | FingerprintProfile::WindowsEdge => {
                // Windows typically has discrete GPUs or Intel integrated
                Self::random()
            }
            FingerprintProfile::MacChrome
            | FingerprintProfile::MacSafari
            | FingerprintProfile::MacFirefox => {
                // Modern Macs use Apple Silicon
                Self::apple_m1()
            }
            FingerprintProfile::LinuxChrome | FingerprintProfile::LinuxFirefox => {
                // Linux often has NVIDIA or AMD
                Self::nvidia_rtx_3060()
            }
            FingerprintProfile::Custom => Self::random(),
        }
    }

    /// Generate JavaScript code to override WebGL properties
    ///
    /// This script must be injected before any page scripts run.
    pub fn get_js_override_script(&self) -> String {
        let canvas_noise_script = if self.enable_canvas_noise {
            generate_canvas_noise_script(self.canvas_noise_intensity)
        } else {
            String::new()
        };

        format!(
            r#"
// WebGL Fingerprint Spoofing
(function() {{
    'use strict';

    const VENDOR = "{vendor}";
    const RENDERER = "{renderer}";
    const VERSION = "{version}";
    const SHADING_LANG_VERSION = "{shading_lang_version}";
    const MAX_TEXTURE_SIZE = {max_texture_size};
    const MAX_VIEWPORT_DIMS = [{max_viewport_0}, {max_viewport_1}];
    const MAX_VERTEX_ATTRIBS = {max_vertex_attribs};
    const MAX_VARYING_VECTORS = {max_varying_vectors};
    const MAX_VERTEX_UNIFORM_VECTORS = {max_vertex_uniform_vectors};
    const MAX_FRAGMENT_UNIFORM_VECTORS = {max_fragment_uniform_vectors};

    // Override getParameter for WebGL contexts
    const overrideGetParameter = function(target) {{
        const originalGetParameter = target.prototype.getParameter;
        target.prototype.getParameter = function(parameter) {{
            // UNMASKED_VENDOR_WEBGL
            if (parameter === 37445) {{
                return VENDOR;
            }}
            // UNMASKED_RENDERER_WEBGL
            if (parameter === 37446) {{
                return RENDERER;
            }}
            // VERSION
            if (parameter === 7938) {{
                return VERSION;
            }}
            // SHADING_LANGUAGE_VERSION
            if (parameter === 35724) {{
                return SHADING_LANG_VERSION;
            }}
            // MAX_TEXTURE_SIZE
            if (parameter === 3379) {{
                return MAX_TEXTURE_SIZE;
            }}
            // MAX_VIEWPORT_DIMS
            if (parameter === 3386) {{
                return new Int32Array(MAX_VIEWPORT_DIMS);
            }}
            // MAX_VERTEX_ATTRIBS
            if (parameter === 34921) {{
                return MAX_VERTEX_ATTRIBS;
            }}
            // MAX_VARYING_VECTORS
            if (parameter === 36348) {{
                return MAX_VARYING_VECTORS;
            }}
            // MAX_VERTEX_UNIFORM_VECTORS
            if (parameter === 36347) {{
                return MAX_VERTEX_UNIFORM_VECTORS;
            }}
            // MAX_FRAGMENT_UNIFORM_VECTORS
            if (parameter === 36349) {{
                return MAX_FRAGMENT_UNIFORM_VECTORS;
            }}
            return originalGetParameter.call(this, parameter);
        }};
    }};

    // Override getExtension to control WEBGL_debug_renderer_info
    const overrideGetExtension = function(target) {{
        const originalGetExtension = target.prototype.getExtension;
        target.prototype.getExtension = function(name) {{
            if (name === 'WEBGL_debug_renderer_info') {{
                // Return a fake extension object
                return {{
                    UNMASKED_VENDOR_WEBGL: 37445,
                    UNMASKED_RENDERER_WEBGL: 37446
                }};
            }}
            return originalGetExtension.call(this, name);
        }};
    }};

    // Override getSupportedExtensions
    const overrideGetSupportedExtensions = function(target) {{
        const originalGetSupportedExtensions = target.prototype.getSupportedExtensions;
        target.prototype.getSupportedExtensions = function() {{
            const extensions = originalGetSupportedExtensions.call(this) || [];
            // Ensure WEBGL_debug_renderer_info is in the list
            if (!extensions.includes('WEBGL_debug_renderer_info')) {{
                extensions.push('WEBGL_debug_renderer_info');
            }}
            return extensions;
        }};
    }};

    // Apply overrides to WebGLRenderingContext
    if (typeof WebGLRenderingContext !== 'undefined') {{
        overrideGetParameter(WebGLRenderingContext);
        overrideGetExtension(WebGLRenderingContext);
        overrideGetSupportedExtensions(WebGLRenderingContext);
    }}

    // Apply overrides to WebGL2RenderingContext
    if (typeof WebGL2RenderingContext !== 'undefined') {{
        overrideGetParameter(WebGL2RenderingContext);
        overrideGetExtension(WebGL2RenderingContext);
        overrideGetSupportedExtensions(WebGL2RenderingContext);
    }}

    // Override getContext to intercept context creation
    const originalGetContext = HTMLCanvasElement.prototype.getContext;
    HTMLCanvasElement.prototype.getContext = function(type, attributes) {{
        const context = originalGetContext.call(this, type, attributes);
        // Context is already patched via prototype
        return context;
    }};

    // Also override OffscreenCanvas if available
    if (typeof OffscreenCanvas !== 'undefined') {{
        const originalOffscreenGetContext = OffscreenCanvas.prototype.getContext;
        OffscreenCanvas.prototype.getContext = function(type, attributes) {{
            const context = originalOffscreenGetContext.call(this, type, attributes);
            return context;
        }};
    }}

}})();

{canvas_noise_script}
"#,
            vendor = escape_js_string(&self.vendor),
            renderer = escape_js_string(&self.renderer),
            version = escape_js_string(&self.version),
            shading_lang_version = escape_js_string(&self.shading_language_version),
            max_texture_size = self.max_texture_size,
            max_viewport_0 = self.max_viewport_dims.0,
            max_viewport_1 = self.max_viewport_dims.1,
            max_vertex_attribs = self.max_vertex_attribs,
            max_varying_vectors = self.max_varying_vectors,
            max_vertex_uniform_vectors = self.max_vertex_uniform_vectors,
            max_fragment_uniform_vectors = self.max_fragment_uniform_vectors,
            canvas_noise_script = canvas_noise_script,
        )
    }

    /// Set canvas noise injection
    pub fn with_canvas_noise(mut self, enabled: bool, intensity: f64) -> Self {
        self.enable_canvas_noise = enabled;
        self.canvas_noise_intensity = intensity.clamp(0.0, 0.01); // Clamp to reasonable range
        self
    }
}

impl Default for WebGLConfig {
    fn default() -> Self {
        Self::nvidia_gtx_1660() // Common mid-range GPU
    }
}

/// Generate JavaScript code for canvas fingerprint noise injection
///
/// This adds imperceptible noise to canvas operations to prevent fingerprinting
/// while maintaining visual appearance.
pub fn generate_canvas_noise_script(intensity: f64) -> String {
    let intensity = intensity.clamp(0.0, 0.01); // Safety clamp

    format!(
        r#"
// Canvas Fingerprint Noise Injection
(function() {{
    'use strict';

    const NOISE_INTENSITY = {intensity};

    // Deterministic pseudo-random based on pixel position
    // This ensures consistent noise for the same content
    function seededRandom(seed) {{
        const x = Math.sin(seed) * 10000;
        return x - Math.floor(x);
    }}

    // Add noise to ImageData
    function addNoiseToImageData(imageData, seed) {{
        const data = imageData.data;
        const len = data.length;

        for (let i = 0; i < len; i += 4) {{
            // Skip transparent pixels
            if (data[i + 3] === 0) continue;

            // Generate consistent noise for this pixel
            const pixelSeed = seed + i;
            const noise = (seededRandom(pixelSeed) - 0.5) * 2 * NOISE_INTENSITY * 255;

            // Apply noise to RGB channels
            data[i] = Math.max(0, Math.min(255, data[i] + noise));     // R
            data[i + 1] = Math.max(0, Math.min(255, data[i + 1] + noise)); // G
            data[i + 2] = Math.max(0, Math.min(255, data[i + 2] + noise)); // B
            // Alpha channel unchanged
        }}

        return imageData;
    }}

    // Session seed for consistent noise
    const SESSION_SEED = Math.random() * 1000000;

    // Override toDataURL
    const originalToDataURL = HTMLCanvasElement.prototype.toDataURL;
    HTMLCanvasElement.prototype.toDataURL = function(type, quality) {{
        try {{
            const ctx = this.getContext('2d');
            if (ctx && this.width > 0 && this.height > 0) {{
                const imageData = ctx.getImageData(0, 0, this.width, this.height);
                const noisyData = addNoiseToImageData(imageData, SESSION_SEED);
                ctx.putImageData(noisyData, 0, 0);
            }}
        }} catch (e) {{
            // Canvas might be tainted or context unavailable
        }}
        return originalToDataURL.call(this, type, quality);
    }};

    // Override toBlob
    const originalToBlob = HTMLCanvasElement.prototype.toBlob;
    HTMLCanvasElement.prototype.toBlob = function(callback, type, quality) {{
        try {{
            const ctx = this.getContext('2d');
            if (ctx && this.width > 0 && this.height > 0) {{
                const imageData = ctx.getImageData(0, 0, this.width, this.height);
                const noisyData = addNoiseToImageData(imageData, SESSION_SEED);
                ctx.putImageData(noisyData, 0, 0);
            }}
        }} catch (e) {{
            // Canvas might be tainted or context unavailable
        }}
        return originalToBlob.call(this, callback, type, quality);
    }};

    // Override getImageData to add noise when reading
    const originalGetImageData = CanvasRenderingContext2D.prototype.getImageData;
    CanvasRenderingContext2D.prototype.getImageData = function(sx, sy, sw, sh) {{
        const imageData = originalGetImageData.call(this, sx, sy, sw, sh);
        // Add subtle noise to returned data
        return addNoiseToImageData(imageData, SESSION_SEED + sx + sy);
    }};

    // Also handle OffscreenCanvas if available
    if (typeof OffscreenCanvas !== 'undefined') {{
        const originalOffscreenToBlob = OffscreenCanvas.prototype.convertToBlob;
        if (originalOffscreenToBlob) {{
            OffscreenCanvas.prototype.convertToBlob = function(options) {{
                try {{
                    const ctx = this.getContext('2d');
                    if (ctx && this.width > 0 && this.height > 0) {{
                        const imageData = ctx.getImageData(0, 0, this.width, this.height);
                        const noisyData = addNoiseToImageData(imageData, SESSION_SEED);
                        ctx.putImageData(noisyData, 0, 0);
                    }}
                }} catch (e) {{}}
                return originalOffscreenToBlob.call(this, options);
            }};
        }}
    }}

}})();
"#,
        intensity = intensity
    )
}

/// Escape string for JavaScript
fn escape_js_string(s: &str) -> String {
    s.replace('\\', "\\\\")
        .replace('"', "\\\"")
        .replace('\n', "\\n")
        .replace('\r', "\\r")
        .replace('\t', "\\t")
}

/// Builder for custom WebGL configurations
#[derive(Debug, Clone)]
pub struct WebGLConfigBuilder {
    config: WebGLConfig,
}

impl WebGLConfigBuilder {
    /// Create a new builder with default values
    pub fn new() -> Self {
        Self {
            config: WebGLConfig::default(),
        }
    }

    /// Start from a profile
    pub fn from_profile(profile: WebGLProfile) -> Self {
        Self {
            config: WebGLConfig::from_profile(profile),
        }
    }

    /// Set vendor string
    pub fn vendor(mut self, vendor: impl Into<String>) -> Self {
        self.config.vendor = vendor.into();
        self
    }

    /// Set renderer string
    pub fn renderer(mut self, renderer: impl Into<String>) -> Self {
        self.config.renderer = renderer.into();
        self
    }

    /// Set WebGL version string
    pub fn version(mut self, version: impl Into<String>) -> Self {
        self.config.version = version.into();
        self
    }

    /// Set shading language version
    pub fn shading_language_version(mut self, version: impl Into<String>) -> Self {
        self.config.shading_language_version = version.into();
        self
    }

    /// Set maximum texture size
    pub fn max_texture_size(mut self, size: u32) -> Self {
        self.config.max_texture_size = size;
        self
    }

    /// Set maximum viewport dimensions
    pub fn max_viewport_dims(mut self, width: u32, height: u32) -> Self {
        self.config.max_viewport_dims = (width, height);
        self
    }

    /// Enable or disable canvas noise
    pub fn canvas_noise(mut self, enabled: bool, intensity: f64) -> Self {
        self.config.enable_canvas_noise = enabled;
        self.config.canvas_noise_intensity = intensity.clamp(0.0, 0.01);
        self
    }

    /// Build the final configuration
    pub fn build(self) -> WebGLConfig {
        self.config
    }
}

impl Default for WebGLConfigBuilder {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_webgl_profiles() {
        for profile in WebGLProfile::all() {
            let config = WebGLConfig::from_profile(profile);
            assert!(!config.vendor.is_empty());
            assert!(!config.renderer.is_empty());
            assert!(config.max_texture_size >= 4096);
        }
    }

    #[test]
    fn test_random_config() {
        let config1 = WebGLConfig::random();
        let config2 = WebGLConfig::random();

        // Both should be valid
        assert!(!config1.vendor.is_empty());
        assert!(!config2.vendor.is_empty());
    }

    #[test]
    fn test_consistent_config() {
        let seed = "test-seed";
        let config1 = WebGLConfig::consistent(seed);
        let config2 = WebGLConfig::consistent(seed);

        assert_eq!(config1.vendor, config2.vendor);
        assert_eq!(config1.renderer, config2.renderer);
    }

    #[test]
    fn test_js_override_generation() {
        let config = WebGLConfig::nvidia_rtx_3060();
        let js = config.get_js_override_script();

        assert!(js.contains("NVIDIA"));
        assert!(js.contains("RTX 3060"));
        assert!(js.contains("getParameter"));
        assert!(js.contains("WEBGL_debug_renderer_info"));
    }

    #[test]
    fn test_canvas_noise_script() {
        let script = generate_canvas_noise_script(0.0001);
        assert!(script.contains("toDataURL"));
        assert!(script.contains("getImageData"));
        assert!(script.contains("NOISE_INTENSITY"));
    }

    #[test]
    fn test_builder() {
        let config = WebGLConfigBuilder::new()
            .vendor("Custom Vendor")
            .renderer("Custom Renderer")
            .max_texture_size(8192)
            .canvas_noise(true, 0.0005)
            .build();

        assert_eq!(config.vendor, "Custom Vendor");
        assert_eq!(config.renderer, "Custom Renderer");
        assert_eq!(config.max_texture_size, 8192);
        assert!(config.enable_canvas_noise);
    }
}
