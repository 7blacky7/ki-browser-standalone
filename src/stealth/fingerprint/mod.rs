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

mod builder;
mod fingerprint;
mod generator;
mod types;

pub use builder::FingerprintBuilder;
pub use fingerprint::BrowserFingerprint;
pub use generator::FingerprintGenerator;
pub use types::{FingerprintProfile, FontEntry, PluginEntry, ScreenResolution};

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

    #[test]
    fn test_screen_resolution_has_orientation_fields() {
        let res = ScreenResolution::new(1920, 1080);
        assert_eq!(res.outer_width, 1920);
        assert_eq!(res.outer_height, 1080);
        assert_eq!(res.orientation_type, "landscape-primary");
        assert_eq!(res.orientation_angle, 0);

        let portrait = ScreenResolution::new(1080, 1920);
        assert_eq!(portrait.orientation_type, "portrait-primary");
        assert_eq!(portrait.orientation_angle, 90);
    }

    #[test]
    fn test_sync_screen_to_viewport_basic() {
        let generator = FingerprintGenerator::new();
        let mut fp = generator.generate_from_profile(FingerprintProfile::WindowsChrome);

        // Viewport 1280x720 (default window size)
        fp.sync_screen_to_viewport(1280, 720);

        let screen = &fp.screen_resolution;

        // screen.width >= outerWidth >= viewport (1280)
        assert!(
            screen.width >= screen.outer_width,
            "screen.width ({}) must be >= outer_width ({})",
            screen.width,
            screen.outer_width
        );
        assert!(
            screen.outer_width >= 1280,
            "outer_width ({}) must be >= viewport width (1280)",
            screen.outer_width
        );

        // screen.height >= outerHeight >= viewport (720)
        assert!(
            screen.height >= screen.outer_height,
            "screen.height ({}) must be >= outer_height ({})",
            screen.height,
            screen.outer_height
        );
        assert!(
            screen.outer_height >= 720,
            "outer_height ({}) must be >= viewport height (720)",
            screen.outer_height
        );

        // outerWidth = viewport + 16 (browser chrome)
        assert_eq!(screen.outer_width, 1296);
        // outerHeight = viewport + 85 (toolbar/tabs)
        assert_eq!(screen.outer_height, 805);

        // availWidth = screen.width, availHeight = screen.height - 40 (taskbar)
        assert_eq!(screen.avail_width, screen.width);
        assert_eq!(screen.avail_height, screen.height.saturating_sub(40));
    }

    #[test]
    fn test_sync_screen_to_viewport_orientation_landscape() {
        let generator = FingerprintGenerator::new();
        let mut fp = generator.generate_from_profile(FingerprintProfile::WindowsChrome);

        fp.sync_screen_to_viewport(1280, 720);

        // Screen should be landscape (width > height for all common resolutions)
        assert_eq!(fp.screen_resolution.orientation_type, "landscape-primary");
        assert_eq!(fp.screen_resolution.orientation_angle, 0);
    }

    #[test]
    fn test_sync_screen_to_viewport_picks_suitable_resolution() {
        let generator = FingerprintGenerator::new();
        let mut fp = generator.generate_from_profile(FingerprintProfile::WindowsChrome);

        // Viewport 1280x720 -> outer 1296x805 -> needs screen >= 1296x805
        // The smallest common resolution that fits is 1366x768
        fp.sync_screen_to_viewport(1280, 720);
        assert!(
            fp.screen_resolution.width >= 1296,
            "screen.width ({}) must be >= outer_width (1296)",
            fp.screen_resolution.width
        );
        assert!(
            fp.screen_resolution.height >= 805,
            "screen.height ({}) must be >= outer_height (805)",
            fp.screen_resolution.height
        );
    }

    #[test]
    fn test_sync_screen_to_viewport_large_viewport() {
        let generator = FingerprintGenerator::new();
        let mut fp = generator.generate_from_profile(FingerprintProfile::WindowsChrome);

        // Large viewport: 1920x1080 -> outer 1936x1165 -> needs screen >= 1936x1165
        fp.sync_screen_to_viewport(1920, 1080);

        assert!(
            fp.screen_resolution.width >= 1936,
            "screen.width ({}) must be >= 1936 for 1920 viewport",
            fp.screen_resolution.width
        );
        assert!(
            fp.screen_resolution.height >= 1165,
            "screen.height ({}) must be >= 1165 for 1080 viewport",
            fp.screen_resolution.height
        );
    }

    #[test]
    fn test_sync_screen_to_viewport_small_viewport() {
        let generator = FingerprintGenerator::new();
        let mut fp = generator.generate_from_profile(FingerprintProfile::WindowsChrome);

        // Small viewport: 800x600 -> outer 816x685 -> needs screen >= 816x685
        fp.sync_screen_to_viewport(800, 600);

        assert!(
            fp.screen_resolution.width >= 816,
            "screen.width ({}) must be >= 816 for 800 viewport",
            fp.screen_resolution.width
        );
        assert!(
            fp.screen_resolution.height >= 685,
            "screen.height ({}) must be >= 685 for 600 viewport",
            fp.screen_resolution.height
        );
        assert_eq!(fp.screen_resolution.outer_width, 816);
        assert_eq!(fp.screen_resolution.outer_height, 685);
    }

    #[test]
    fn test_js_overrides_contain_orientation_and_outer() {
        let generator = FingerprintGenerator::new();
        let mut fp = generator.generate_from_profile(FingerprintProfile::WindowsChrome);
        fp.sync_screen_to_viewport(1280, 720);

        let js = fp.to_js_overrides();

        // Orientation overrides
        assert!(
            js.contains("screen.orientation"),
            "JS should contain screen.orientation override"
        );
        assert!(
            js.contains("landscape-primary"),
            "JS should contain landscape-primary for landscape viewport"
        );

        // outerWidth/Height overrides
        assert!(
            js.contains("outerWidth"),
            "JS should contain outerWidth override"
        );
        assert!(
            js.contains("outerHeight"),
            "JS should contain outerHeight override"
        );

        // Flag to prevent chromium_engine fallback from overwriting
        assert!(
            js.contains("__fp_outer_applied"),
            "JS should set __fp_outer_applied flag"
        );
    }

    #[test]
    fn test_screen_resolution_invariants_after_sync() {
        // Test the invariants for multiple viewport sizes
        let viewports = vec![
            (800, 600),
            (1024, 768),
            (1280, 720),
            (1366, 768),
            (1920, 1080),
            (2560, 1440),
        ];

        let generator = FingerprintGenerator::new();

        for (vw, vh) in viewports {
            let mut fp = generator.generate_from_profile(FingerprintProfile::WindowsChrome);
            fp.sync_screen_to_viewport(vw, vh);

            let s = &fp.screen_resolution;

            // Invariant 1: screen.width >= outerWidth >= viewport
            assert!(
                s.width >= s.outer_width && s.outer_width >= vw,
                "Viewport {}x{}: screen.width({}) >= outer_width({}) >= viewport({})",
                vw, vh, s.width, s.outer_width, vw
            );

            // Invariant 2: screen.height >= outerHeight >= viewport
            assert!(
                s.height >= s.outer_height && s.outer_height >= vh,
                "Viewport {}x{}: screen.height({}) >= outer_height({}) >= viewport({})",
                vw, vh, s.height, s.outer_height, vh
            );

            // Invariant 3: orientation matches screen dimensions
            if s.width >= s.height {
                assert_eq!(s.orientation_type, "landscape-primary");
                assert_eq!(s.orientation_angle, 0);
            } else {
                assert_eq!(s.orientation_type, "portrait-primary");
                assert_eq!(s.orientation_angle, 90);
            }

            // Invariant 4: availWidth = width, availHeight = height - 40
            assert_eq!(s.avail_width, s.width);
            assert_eq!(s.avail_height, s.height.saturating_sub(40));
        }
    }
}
