//! Standalone JavaScript generation for canvas fingerprint noise injection.
//!
//! Provides [`generate_canvas_noise_script`] which creates a self-contained IIFE
//! that adds imperceptible pixel noise to HTMLCanvasElement.toDataURL, toBlob,
//! CanvasRenderingContext2D.getImageData, and OffscreenCanvas.convertToBlob to
//! prevent canvas-based browser fingerprinting.

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
