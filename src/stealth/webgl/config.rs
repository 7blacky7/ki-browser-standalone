//! WebGL configuration for fingerprint spoofing with JavaScript override generation.
//!
//! Contains [`WebGLConfig`] which holds all WebGL/WebGPU parameters for a specific
//! GPU profile and generates comprehensive JavaScript to override WebGL getParameter,
//! getExtension, OffscreenCanvas contexts, and WebGPU adapter info.

use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};

use super::profiles::WebGLProfile;
use super::scripts::generate_canvas_noise_script;

/// WebGL configuration for fingerprint spoofing
#[derive(Debug, Clone)]
pub struct WebGLConfig {
    /// WebGL vendor string (WEBGL_debug_renderer_info extension)
    pub vendor: String,
    /// WebGL renderer string (WEBGL_debug_renderer_info extension)
    pub renderer: String,
    /// Short vendor name for WebGPU adapter info (e.g. "nvidia", "amd", "intel")
    pub vendor_short: String,
    /// GPU architecture for WebGPU adapter info (e.g. "ampere", "rdna-3", "gen-12")
    pub architecture: String,
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
            vendor_short: profile.vendor_short().to_string(),
            architecture: profile.architecture().to_string(),
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
    /// Covers: WebGLRenderingContext, WebGL2RenderingContext, OffscreenCanvas WebGL,
    /// WEBGL_debug_renderer_info extension, and WebGPU adapter info.
    pub fn get_js_override_script(&self) -> String {
        let canvas_noise_script = if self.enable_canvas_noise {
            generate_canvas_noise_script(self.canvas_noise_intensity)
        } else {
            String::new()
        };

        format!(
            r#"
// WebGL Fingerprint Spoofing (comprehensive)
(function() {{
    'use strict';

    const VENDOR = "{vendor}";
    const RENDERER = "{renderer}";
    const VENDOR_SHORT = "{vendor_short}";
    const ARCHITECTURE = "{architecture}";
    const VERSION = "{version}";
    const SHADING_LANG_VERSION = "{shading_lang_version}";
    const MAX_TEXTURE_SIZE = {max_texture_size};
    const MAX_VIEWPORT_DIMS = [{max_viewport_0}, {max_viewport_1}];
    const MAX_VERTEX_ATTRIBS = {max_vertex_attribs};
    const MAX_VARYING_VECTORS = {max_varying_vectors};
    const MAX_VERTEX_UNIFORM_VECTORS = {max_vertex_uniform_vectors};
    const MAX_FRAGMENT_UNIFORM_VECTORS = {max_fragment_uniform_vectors};

    // === WebGL parameter constants (hex for clarity) ===
    const GL_VENDOR = 0x1F00;
    const GL_RENDERER = 0x1F01;
    const GL_VERSION = 0x1F02;
    const UNMASKED_VENDOR_WEBGL = 0x9245;
    const UNMASKED_RENDERER_WEBGL = 0x9246;
    const GL_SHADING_LANGUAGE_VERSION = 0x8B8C;
    const GL_MAX_TEXTURE_SIZE = 0x0D33;
    const GL_MAX_VIEWPORT_DIMS = 0x0D3A;
    const GL_MAX_VERTEX_ATTRIBS = 0x8869;
    const GL_MAX_VARYING_VECTORS = 0x8DFC;
    const GL_MAX_VERTEX_UNIFORM_VECTORS = 0x8DFB;
    const GL_MAX_FRAGMENT_UNIFORM_VECTORS = 0x8DFD;

    // === Helper: Patch getParameter on a WebGL context prototype ===
    const overrideGetParameter = function(target) {{
        const originalGetParameter = target.prototype.getParameter;
        target.prototype.getParameter = function(parameter) {{
            if (parameter === UNMASKED_VENDOR_WEBGL) return VENDOR;
            if (parameter === UNMASKED_RENDERER_WEBGL) return RENDERER;
            if (parameter === GL_VENDOR) return 'WebKit';
            if (parameter === GL_RENDERER) return 'WebKit WebGL';
            if (parameter === GL_VERSION) return VERSION;
            if (parameter === GL_SHADING_LANGUAGE_VERSION) return SHADING_LANG_VERSION;
            if (parameter === GL_MAX_TEXTURE_SIZE) return MAX_TEXTURE_SIZE;
            if (parameter === GL_MAX_VIEWPORT_DIMS) return new Int32Array(MAX_VIEWPORT_DIMS);
            if (parameter === GL_MAX_VERTEX_ATTRIBS) return MAX_VERTEX_ATTRIBS;
            if (parameter === GL_MAX_VARYING_VECTORS) return MAX_VARYING_VECTORS;
            if (parameter === GL_MAX_VERTEX_UNIFORM_VECTORS) return MAX_VERTEX_UNIFORM_VECTORS;
            if (parameter === GL_MAX_FRAGMENT_UNIFORM_VECTORS) return MAX_FRAGMENT_UNIFORM_VECTORS;
            return originalGetParameter.call(this, parameter);
        }};
    }};

    // === Helper: Patch getExtension for WEBGL_debug_renderer_info ===
    const overrideGetExtension = function(target) {{
        const originalGetExtension = target.prototype.getExtension;
        target.prototype.getExtension = function(name) {{
            if (name === 'WEBGL_debug_renderer_info') {{
                return {{
                    UNMASKED_VENDOR_WEBGL: 0x9245,
                    UNMASKED_RENDERER_WEBGL: 0x9246
                }};
            }}
            return originalGetExtension.call(this, name);
        }};
    }};

    // === Helper: Patch getSupportedExtensions ===
    const overrideGetSupportedExtensions = function(target) {{
        const originalGetSupportedExtensions = target.prototype.getSupportedExtensions;
        target.prototype.getSupportedExtensions = function() {{
            const extensions = originalGetSupportedExtensions.call(this) || [];
            if (!extensions.includes('WEBGL_debug_renderer_info')) {{
                extensions.push('WEBGL_debug_renderer_info');
            }}
            return extensions;
        }};
    }};

    // === 1. Apply overrides to WebGLRenderingContext ===
    if (typeof WebGLRenderingContext !== 'undefined') {{
        overrideGetParameter(WebGLRenderingContext);
        overrideGetExtension(WebGLRenderingContext);
        overrideGetSupportedExtensions(WebGLRenderingContext);
    }}

    // === 2. Apply overrides to WebGL2RenderingContext ===
    if (typeof WebGL2RenderingContext !== 'undefined') {{
        overrideGetParameter(WebGL2RenderingContext);
        overrideGetExtension(WebGL2RenderingContext);
        overrideGetSupportedExtensions(WebGL2RenderingContext);
    }}

    // === 3. Override HTMLCanvasElement.getContext ===
    // Guard: HTMLCanvasElement may not exist at document-creation time
    // (evaluate_on_new_document). Also patch each context INSTANCE to be
    // resilient against Chrome re-initialising prototype methods after our
    // script has run.
    if (typeof HTMLCanvasElement !== 'undefined') {{
        const originalGetContext = HTMLCanvasElement.prototype.getContext;
        HTMLCanvasElement.prototype.getContext = function(type, attributes) {{
            const context = originalGetContext.call(this, type, attributes);
            if (context && (type === 'webgl' || type === 'webgl2' || type === 'experimental-webgl')) {{
                const origParam = context.getParameter.bind(context);
                context.getParameter = function(p) {{
                    if (p === UNMASKED_VENDOR_WEBGL) return VENDOR;
                    if (p === UNMASKED_RENDERER_WEBGL) return RENDERER;
                    if (p === GL_VENDOR) return 'WebKit';
                    if (p === GL_RENDERER) return 'WebKit WebGL';
                    if (p === GL_VERSION) return VERSION;
                    if (p === GL_SHADING_LANGUAGE_VERSION) return SHADING_LANG_VERSION;
                    if (p === GL_MAX_TEXTURE_SIZE) return MAX_TEXTURE_SIZE;
                    if (p === GL_MAX_VIEWPORT_DIMS) return new Int32Array(MAX_VIEWPORT_DIMS);
                    if (p === GL_MAX_VERTEX_ATTRIBS) return MAX_VERTEX_ATTRIBS;
                    if (p === GL_MAX_VARYING_VECTORS) return MAX_VARYING_VECTORS;
                    if (p === GL_MAX_VERTEX_UNIFORM_VECTORS) return MAX_VERTEX_UNIFORM_VECTORS;
                    if (p === GL_MAX_FRAGMENT_UNIFORM_VECTORS) return MAX_FRAGMENT_UNIFORM_VECTORS;
                    return origParam(p);
                }};
                const origExt = context.getExtension.bind(context);
                context.getExtension = function(name) {{
                    if (name === 'WEBGL_debug_renderer_info') {{
                        return {{
                            UNMASKED_VENDOR_WEBGL: 0x9245,
                            UNMASKED_RENDERER_WEBGL: 0x9246
                        }};
                    }}
                    return origExt(name);
                }};
            }}
            return context;
        }};
    }}

    // === 4. OffscreenCanvas WebGL override ===
    // OffscreenCanvas creates independent contexts that may bypass prototype patches,
    // so we must patch getParameter on each returned context instance.
    if (typeof OffscreenCanvas !== 'undefined') {{
        const origOffscreenGetContext = OffscreenCanvas.prototype.getContext;
        OffscreenCanvas.prototype.getContext = function(type, attrs) {{
            const ctx = origOffscreenGetContext.call(this, type, attrs);
            if (ctx && (type === 'webgl' || type === 'webgl2' || type === 'experimental-webgl')) {{
                const origParam = ctx.getParameter.bind(ctx);
                ctx.getParameter = function(p) {{
                    if (p === UNMASKED_VENDOR_WEBGL) return VENDOR;
                    if (p === UNMASKED_RENDERER_WEBGL) return RENDERER;
                    if (p === GL_VENDOR) return 'WebKit';
                    if (p === GL_RENDERER) return 'WebKit WebGL';
                    if (p === GL_VERSION) return VERSION;
                    if (p === GL_SHADING_LANGUAGE_VERSION) return SHADING_LANG_VERSION;
                    if (p === GL_MAX_TEXTURE_SIZE) return MAX_TEXTURE_SIZE;
                    if (p === GL_MAX_VIEWPORT_DIMS) return new Int32Array(MAX_VIEWPORT_DIMS);
                    if (p === GL_MAX_VERTEX_ATTRIBS) return MAX_VERTEX_ATTRIBS;
                    if (p === GL_MAX_VARYING_VECTORS) return MAX_VARYING_VECTORS;
                    if (p === GL_MAX_VERTEX_UNIFORM_VECTORS) return MAX_VERTEX_UNIFORM_VECTORS;
                    if (p === GL_MAX_FRAGMENT_UNIFORM_VECTORS) return MAX_FRAGMENT_UNIFORM_VECTORS;
                    return origParam(p);
                }};
                // Also patch getExtension on the instance
                const origExt = ctx.getExtension.bind(ctx);
                ctx.getExtension = function(name) {{
                    if (name === 'WEBGL_debug_renderer_info') {{
                        return {{
                            UNMASKED_VENDOR_WEBGL: 0x9245,
                            UNMASKED_RENDERER_WEBGL: 0x9246
                        }};
                    }}
                    return origExt(name);
                }};
                // Patch getSupportedExtensions on the instance
                const origSupported = ctx.getSupportedExtensions.bind(ctx);
                ctx.getSupportedExtensions = function() {{
                    const extensions = origSupported() || [];
                    if (!extensions.includes('WEBGL_debug_renderer_info')) {{
                        extensions.push('WEBGL_debug_renderer_info');
                    }}
                    return extensions;
                }};
            }}
            return ctx;
        }};
    }}

    // === 5. WebGPU adapter info spoofing ===
    if (typeof navigator !== 'undefined' && navigator.gpu) {{
        const origRequestAdapter = navigator.gpu.requestAdapter.bind(navigator.gpu);
        navigator.gpu.requestAdapter = async function(options) {{
            const adapter = await origRequestAdapter(options);
            if (adapter) {{
                // Override requestAdapterInfo if available
                const origRequestAdapterInfo = adapter.requestAdapterInfo?.bind(adapter);
                if (origRequestAdapterInfo) {{
                    adapter.requestAdapterInfo = async function() {{
                        return {{
                            vendor: VENDOR_SHORT,
                            architecture: ARCHITECTURE,
                            device: '',
                            description: RENDERER
                        }};
                    }};
                }}
                // Override adapterInfo property (direct access without method call)
                try {{
                    Object.defineProperty(adapter, 'info', {{
                        get: function() {{
                            return {{
                                vendor: VENDOR_SHORT,
                                architecture: ARCHITECTURE,
                                device: '',
                                description: RENDERER
                            }};
                        }},
                        configurable: true
                    }});
                }} catch(e) {{}}
            }}
            return adapter;
        }};
    }}

}})();

{canvas_noise_script}
"#,
            vendor = escape_js_string(&self.vendor),
            renderer = escape_js_string(&self.renderer),
            vendor_short = escape_js_string(&self.vendor_short),
            architecture = escape_js_string(&self.architecture),
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

/// Escape string for JavaScript
fn escape_js_string(s: &str) -> String {
    s.replace('\\', "\\\\")
        .replace('"', "\\\"")
        .replace('\n', "\\n")
        .replace('\r', "\\r")
        .replace('\t', "\\t")
}
