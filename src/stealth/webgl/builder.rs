//! Builder pattern for custom WebGL configurations.
//!
//! Provides [`WebGLConfigBuilder`] for step-by-step construction of a
//! [`WebGLConfig`] with custom vendor, renderer, GPU parameters, and
//! canvas noise settings.

use super::config::WebGLConfig;
use super::profiles::WebGLProfile;

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

    /// Set short vendor name for WebGPU adapter info
    pub fn vendor_short(mut self, vendor_short: impl Into<String>) -> Self {
        self.config.vendor_short = vendor_short.into();
        self
    }

    /// Set GPU architecture for WebGPU adapter info
    pub fn architecture(mut self, architecture: impl Into<String>) -> Self {
        self.config.architecture = architecture.into();
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
