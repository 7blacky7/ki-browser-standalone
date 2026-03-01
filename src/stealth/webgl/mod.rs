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

mod builder;
mod config;
mod profiles;
mod scripts;

pub use builder::WebGLConfigBuilder;
pub use config::WebGLConfig;
pub use profiles::WebGLProfile;
pub use scripts::generate_canvas_noise_script;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_webgl_profiles() {
        for profile in WebGLProfile::all() {
            let config = WebGLConfig::from_profile(profile.clone());
            assert!(!config.vendor.is_empty(), "vendor empty for {:?}", profile);
            assert!(!config.renderer.is_empty(), "renderer empty for {:?}", profile);
            assert!(!config.vendor_short.is_empty(), "vendor_short empty for {:?}", profile);
            assert!(!config.architecture.is_empty(), "architecture empty for {:?}", profile);
            assert!(config.max_texture_size >= 4096);
        }
    }

    #[test]
    fn test_webgl_profile_vendor_short() {
        assert_eq!(WebGLProfile::NvidiaRtx3060.vendor_short(), "nvidia");
        assert_eq!(WebGLProfile::AmdRx6700Xt.vendor_short(), "amd");
        assert_eq!(WebGLProfile::IntelIrisXe.vendor_short(), "intel");
        assert_eq!(WebGLProfile::AppleM1.vendor_short(), "apple");
        assert_eq!(WebGLProfile::SwiftShader.vendor_short(), "google");
    }

    #[test]
    fn test_webgl_profile_architecture() {
        assert_eq!(WebGLProfile::NvidiaRtx3060.architecture(), "ampere");
        assert_eq!(WebGLProfile::NvidiaRtx4070.architecture(), "ada-lovelace");
        assert_eq!(WebGLProfile::AmdRx7900Xt.architecture(), "rdna-3");
        assert_eq!(WebGLProfile::IntelIrisXe.architecture(), "gen-12");
        assert_eq!(WebGLProfile::AppleM2.architecture(), "apple-8");
    }

    #[test]
    fn test_random_config() {
        let config1 = WebGLConfig::random();
        let config2 = WebGLConfig::random();

        // Both should be valid
        assert!(!config1.vendor.is_empty());
        assert!(!config2.vendor.is_empty());
        assert!(!config1.vendor_short.is_empty());
        assert!(!config2.architecture.is_empty());
    }

    #[test]
    fn test_consistent_config() {
        let seed = "test-seed";
        let config1 = WebGLConfig::consistent(seed);
        let config2 = WebGLConfig::consistent(seed);

        assert_eq!(config1.vendor, config2.vendor);
        assert_eq!(config1.renderer, config2.renderer);
        assert_eq!(config1.vendor_short, config2.vendor_short);
        assert_eq!(config1.architecture, config2.architecture);
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
    fn test_js_override_has_hex_constants() {
        let config = WebGLConfig::nvidia_rtx_3060();
        let js = config.get_js_override_script();

        // Verify hex constants are used
        assert!(js.contains("0x9245"), "Missing UNMASKED_VENDOR_WEBGL hex constant");
        assert!(js.contains("0x9246"), "Missing UNMASKED_RENDERER_WEBGL hex constant");
        assert!(js.contains("0x1F00"), "Missing GL_VENDOR hex constant");
        assert!(js.contains("0x1F01"), "Missing GL_RENDERER hex constant");
    }

    #[test]
    fn test_js_override_has_gl_vendor_renderer() {
        let config = WebGLConfig::nvidia_rtx_3060();
        let js = config.get_js_override_script();

        // GL_VENDOR and GL_RENDERER should return generic WebKit values
        assert!(js.contains("'WebKit'"), "Missing GL_VENDOR 'WebKit' override");
        assert!(js.contains("'WebKit WebGL'"), "Missing GL_RENDERER 'WebKit WebGL' override");
    }

    #[test]
    fn test_js_override_has_offscreen_canvas_patching() {
        let config = WebGLConfig::nvidia_rtx_3060();
        let js = config.get_js_override_script();

        // OffscreenCanvas must have per-instance getParameter patching
        assert!(js.contains("OffscreenCanvas"), "Missing OffscreenCanvas override");
        assert!(js.contains("origOffscreenGetContext"), "Missing OffscreenCanvas getContext intercept");
        assert!(js.contains("origParam"), "Missing OffscreenCanvas per-instance getParameter patch");
    }

    #[test]
    fn test_js_override_has_webgpu_spoofing() {
        let config = WebGLConfig::nvidia_rtx_3060();
        let js = config.get_js_override_script();

        // WebGPU adapter info must be spoofed
        assert!(js.contains("navigator.gpu"), "Missing WebGPU check");
        assert!(js.contains("requestAdapter"), "Missing WebGPU requestAdapter override");
        assert!(js.contains("requestAdapterInfo"), "Missing WebGPU requestAdapterInfo override");
        assert!(js.contains("VENDOR_SHORT"), "Missing VENDOR_SHORT constant for WebGPU");
        assert!(js.contains("ARCHITECTURE"), "Missing ARCHITECTURE constant for WebGPU");
    }

    #[test]
    fn test_js_override_webgpu_values() {
        let config = WebGLConfig::nvidia_rtx_3060();
        let js = config.get_js_override_script();

        assert!(js.contains(r#"VENDOR_SHORT = "nvidia""#), "WebGPU vendor_short should be 'nvidia' for RTX 3060");
        assert!(js.contains(r#"ARCHITECTURE = "ampere""#), "WebGPU architecture should be 'ampere' for RTX 3060");
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
            .vendor_short("custom")
            .architecture("custom-arch")
            .max_texture_size(8192)
            .canvas_noise(true, 0.0005)
            .build();

        assert_eq!(config.vendor, "Custom Vendor");
        assert_eq!(config.renderer, "Custom Renderer");
        assert_eq!(config.vendor_short, "custom");
        assert_eq!(config.architecture, "custom-arch");
        assert_eq!(config.max_texture_size, 8192);
        assert!(config.enable_canvas_noise);
    }
}
