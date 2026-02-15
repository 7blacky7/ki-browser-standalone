//! AudioContext Fingerprint Spoofing
//!
//! This module provides AudioContext fingerprint protection by injecting
//! controlled noise into audio processing operations. AudioContext fingerprinting
//! exploits differences in how audio is processed across different hardware
//! and software configurations to create a unique identifier.
//!
//! The attack typically involves creating an oscillator, processing audio through
//! various nodes, and examining the resulting audio data. Subtle differences in
//! floating-point computation and audio pipeline implementation create a unique
//! fingerprint for each browser/hardware combination.
//!
//! # Components
//!
//! - `AudioConfig` - Configuration for AudioContext spoofing
//! - Noise injection for `getChannelData`, `getFloatFrequencyData`, and related methods
//! - Protection for both `AudioContext` and `OfflineAudioContext`
//!
//! # Example
//!
//! ```rust,no_run
//! use ki_browser_standalone::stealth::audio::AudioConfig;
//!
//! // Use safe defaults
//! let config = AudioConfig::default();
//!
//! // Or customize noise level
//! let config = AudioConfig::new(0.001);
//!
//! // Get the JavaScript override script
//! let js = config.get_override_script();
//! ```

/// AudioContext fingerprint spoofing configuration
///
/// Controls how noise is injected into audio processing operations
/// to prevent fingerprinting through the Web Audio API.
#[derive(Debug, Clone)]
pub struct AudioConfig {
    /// Enable AudioContext spoofing
    ///
    /// When false, no audio modifications are applied.
    pub enabled: bool,
    /// Noise level for audio processing (0.0 - 0.1, recommended: 0.0001 - 0.001)
    ///
    /// Audio fingerprinting relies on very precise floating-point values,
    /// so even tiny amounts of noise are effective. Values above 0.01
    /// may cause audible artifacts.
    pub noise_level: f64,
}

impl AudioConfig {
    /// Create a new audio configuration with the specified noise level
    pub fn new(noise_level: f64) -> Self {
        Self {
            enabled: true,
            noise_level: noise_level.clamp(0.0, 0.1),
        }
    }

    /// Create a disabled configuration (no audio protection)
    pub fn disabled() -> Self {
        Self {
            enabled: false,
            noise_level: 0.0,
        }
    }

    /// Generate JavaScript override script for AudioContext spoofing
    ///
    /// This script must be injected before any page scripts run.
    pub fn get_override_script(&self) -> String {
        if !self.enabled {
            return String::new();
        }

        let noise_level = self.noise_level.clamp(0.0, 0.1);

        format!(
            r#"
// AudioContext Fingerprint Spoofing
(function() {{
    'use strict';

    const AUDIO_NOISE_LEVEL = {noise_level};

    // Session seed for consistent noise within a session
    const AUDIO_SESSION_SEED = Math.floor(Math.random() * 2147483647);

    // Simple seeded PRNG for deterministic noise
    function audioSeededRandom(seed) {{
        seed = (seed * 16807 + 0) % 2147483647;
        return (seed - 1) / 2147483646;
    }}

    // Add noise to a Float32Array (audio buffer data)
    function addNoiseToAudioBuffer(buffer, seed) {{
        let currentSeed = seed;
        for (let i = 0; i < buffer.length; i++) {{
            currentSeed = (currentSeed * 16807 + 0) % 2147483647;
            const noise = ((currentSeed - 1) / 2147483646 - 0.5) * 2 * AUDIO_NOISE_LEVEL;
            buffer[i] = buffer[i] + noise;
        }}
        return buffer;
    }}

    // Override AudioBuffer.getChannelData
    if (typeof AudioBuffer !== 'undefined') {{
        const originalGetChannelData = AudioBuffer.prototype.getChannelData;
        AudioBuffer.prototype.getChannelData = function(channel) {{
            const data = originalGetChannelData.call(this, channel);
            // Only add noise if buffer has content (not silent)
            let hasContent = false;
            for (let i = 0; i < Math.min(data.length, 100); i++) {{
                if (data[i] !== 0) {{
                    hasContent = true;
                    break;
                }}
            }}
            if (hasContent) {{
                addNoiseToAudioBuffer(data, AUDIO_SESSION_SEED + channel * 1000);
            }}
            return data;
        }};
    }}

    // Override AnalyserNode methods
    if (typeof AnalyserNode !== 'undefined') {{
        // Override getFloatFrequencyData
        const originalGetFloatFrequencyData = AnalyserNode.prototype.getFloatFrequencyData;
        AnalyserNode.prototype.getFloatFrequencyData = function(array) {{
            originalGetFloatFrequencyData.call(this, array);
            addNoiseToAudioBuffer(array, AUDIO_SESSION_SEED + 1);
            return undefined;
        }};

        // Override getByteFrequencyData
        const originalGetByteFrequencyData = AnalyserNode.prototype.getByteFrequencyData;
        AnalyserNode.prototype.getByteFrequencyData = function(array) {{
            originalGetByteFrequencyData.call(this, array);
            let currentSeed = AUDIO_SESSION_SEED + 2;
            for (let i = 0; i < array.length; i++) {{
                currentSeed = (currentSeed * 16807 + 0) % 2147483647;
                const noise = ((currentSeed - 1) / 2147483646 - 0.5) * 2 * AUDIO_NOISE_LEVEL * 255;
                array[i] = Math.max(0, Math.min(255, Math.round(array[i] + noise)));
            }}
            return undefined;
        }};

        // Override getFloatTimeDomainData
        const originalGetFloatTimeDomainData = AnalyserNode.prototype.getFloatTimeDomainData;
        AnalyserNode.prototype.getFloatTimeDomainData = function(array) {{
            originalGetFloatTimeDomainData.call(this, array);
            addNoiseToAudioBuffer(array, AUDIO_SESSION_SEED + 3);
            return undefined;
        }};

        // Override getByteTimeDomainData
        const originalGetByteTimeDomainData = AnalyserNode.prototype.getByteTimeDomainData;
        AnalyserNode.prototype.getByteTimeDomainData = function(array) {{
            originalGetByteTimeDomainData.call(this, array);
            let currentSeed = AUDIO_SESSION_SEED + 4;
            for (let i = 0; i < array.length; i++) {{
                currentSeed = (currentSeed * 16807 + 0) % 2147483647;
                const noise = ((currentSeed - 1) / 2147483646 - 0.5) * 2 * AUDIO_NOISE_LEVEL * 255;
                array[i] = Math.max(0, Math.min(255, Math.round(array[i] + noise)));
            }}
            return undefined;
        }};
    }}

    // Override OfflineAudioContext.startRendering to add noise to result
    if (typeof OfflineAudioContext !== 'undefined') {{
        const originalStartRendering = OfflineAudioContext.prototype.startRendering;
        OfflineAudioContext.prototype.startRendering = function() {{
            return originalStartRendering.call(this).then(function(renderedBuffer) {{
                // Add noise to all channels of the rendered buffer
                for (let ch = 0; ch < renderedBuffer.numberOfChannels; ch++) {{
                    const channelData = renderedBuffer.getChannelData(ch);
                    // Note: getChannelData is already overridden above,
                    // but we add an additional layer here for offline rendering
                    addNoiseToAudioBuffer(channelData, AUDIO_SESSION_SEED + ch * 500 + 100);
                }}
                return renderedBuffer;
            }});
        }};
    }}

    // Override AudioContext properties that can be used for fingerprinting
    if (typeof AudioContext !== 'undefined' || typeof webkitAudioContext !== 'undefined') {{
        const AudioCtx = typeof AudioContext !== 'undefined' ? AudioContext : webkitAudioContext;

        // Override createAnalyser to ensure noise is applied
        const originalCreateAnalyser = AudioCtx.prototype.createAnalyser;
        AudioCtx.prototype.createAnalyser = function() {{
            const analyser = originalCreateAnalyser.call(this);
            // The analyser node methods are already overridden above
            return analyser;
        }};

        // Override createOscillator to add subtle frequency deviation
        const originalCreateOscillator = AudioCtx.prototype.createOscillator;
        AudioCtx.prototype.createOscillator = function() {{
            const oscillator = originalCreateOscillator.call(this);
            // Override the frequency value to add tiny noise
            const originalFrequency = oscillator.frequency;
            try {{
                const originalFreqValue = originalFrequency.value;
                // Add imperceptible frequency deviation
                const deviation = (audioSeededRandom(AUDIO_SESSION_SEED + 42) - 0.5) * AUDIO_NOISE_LEVEL * 0.1;
                originalFrequency.value = originalFreqValue + deviation;
            }} catch (e) {{
                // frequency might be read-only in some contexts
            }}
            return oscillator;
        }};
    }}

}})();
"#,
            noise_level = noise_level,
        )
    }
}

impl Default for AudioConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            noise_level: 0.0001,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_config() {
        let config = AudioConfig::default();
        assert!(config.enabled);
        assert!((config.noise_level - 0.0001).abs() < f64::EPSILON);
    }

    #[test]
    fn test_disabled_config() {
        let config = AudioConfig::disabled();
        assert!(!config.enabled);
        let js = config.get_override_script();
        assert!(js.is_empty());
    }

    #[test]
    fn test_custom_noise_level() {
        let config = AudioConfig::new(0.001);
        assert!((config.noise_level - 0.001).abs() < f64::EPSILON);
    }

    #[test]
    fn test_noise_level_clamping() {
        let config = AudioConfig::new(1.0);
        assert!((config.noise_level - 0.1).abs() < f64::EPSILON);

        let config = AudioConfig::new(-0.5);
        assert!((config.noise_level - 0.0).abs() < f64::EPSILON);
    }

    #[test]
    fn test_override_script_contains_protections() {
        let config = AudioConfig::default();
        let js = config.get_override_script();

        assert!(js.contains("AudioBuffer"));
        assert!(js.contains("getChannelData"));
        assert!(js.contains("AnalyserNode"));
        assert!(js.contains("getFloatFrequencyData"));
        assert!(js.contains("getByteFrequencyData"));
        assert!(js.contains("OfflineAudioContext"));
        assert!(js.contains("startRendering"));
        assert!(js.contains("createAnalyser"));
        assert!(js.contains("createOscillator"));
    }

    #[test]
    fn test_script_is_iife() {
        let config = AudioConfig::default();
        let js = config.get_override_script();

        assert!(js.contains("(function()"));
        assert!(js.contains("'use strict'"));
        assert!(js.contains("})();"));
    }

    #[test]
    fn test_noise_level_in_script() {
        let config = AudioConfig::new(0.005);
        let js = config.get_override_script();

        assert!(js.contains("0.005"));
    }
}
