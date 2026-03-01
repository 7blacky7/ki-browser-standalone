//! Fingerprint type definitions for browser/OS profile identification.
//!
//! Contains the core types used by fingerprint generation: profile enums for
//! common browser/OS combinations, screen resolution configuration with
//! orientation support, and plugin/font data structures.

/// Predefined fingerprint profiles for common browser/OS combinations
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum FingerprintProfile {
    /// Windows 10/11 with Chrome (most common)
    WindowsChrome,
    /// Windows 10/11 with Firefox
    WindowsFirefox,
    /// Windows 10/11 with Edge
    WindowsEdge,
    /// macOS with Chrome
    MacChrome,
    /// macOS with Safari
    MacSafari,
    /// macOS with Firefox
    MacFirefox,
    /// Linux with Chrome
    LinuxChrome,
    /// Linux with Firefox
    LinuxFirefox,
    /// Custom profile with user-defined values
    Custom,
}

impl FingerprintProfile {
    /// Get all standard profiles (excluding Custom)
    pub fn all_standard() -> Vec<FingerprintProfile> {
        vec![
            FingerprintProfile::WindowsChrome,
            FingerprintProfile::WindowsFirefox,
            FingerprintProfile::WindowsEdge,
            FingerprintProfile::MacChrome,
            FingerprintProfile::MacSafari,
            FingerprintProfile::MacFirefox,
            FingerprintProfile::LinuxChrome,
            FingerprintProfile::LinuxFirefox,
        ]
    }

    /// Get the platform string for this profile
    pub fn platform(&self) -> &'static str {
        match self {
            FingerprintProfile::WindowsChrome
            | FingerprintProfile::WindowsFirefox
            | FingerprintProfile::WindowsEdge => "Win32",
            FingerprintProfile::MacChrome
            | FingerprintProfile::MacSafari
            | FingerprintProfile::MacFirefox => "MacIntel",
            FingerprintProfile::LinuxChrome | FingerprintProfile::LinuxFirefox => "Linux x86_64",
            FingerprintProfile::Custom => "Win32",
        }
    }

    /// Get the vendor string for this profile
    pub fn vendor(&self) -> &'static str {
        match self {
            FingerprintProfile::WindowsChrome
            | FingerprintProfile::MacChrome
            | FingerprintProfile::LinuxChrome
            | FingerprintProfile::WindowsEdge => "Google Inc.",
            FingerprintProfile::MacSafari => "Apple Computer, Inc.",
            FingerprintProfile::WindowsFirefox
            | FingerprintProfile::MacFirefox
            | FingerprintProfile::LinuxFirefox => "",
            FingerprintProfile::Custom => "Google Inc.",
        }
    }
}

/// Screen resolution configuration
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ScreenResolution {
    pub width: u32,
    pub height: u32,
    pub avail_width: u32,
    pub avail_height: u32,
    /// window.outerWidth (viewport + browser chrome ~16px)
    pub outer_width: u32,
    /// window.outerHeight (viewport + browser chrome ~85px for toolbar/tabs)
    pub outer_height: u32,
    /// screen.orientation.type (e.g., "landscape-primary", "portrait-primary")
    pub orientation_type: String,
    /// screen.orientation.angle (0 for landscape-primary, 90 for portrait-primary)
    pub orientation_angle: u32,
}

impl ScreenResolution {
    pub fn new(width: u32, height: u32) -> Self {
        let orientation_type = if width >= height {
            "landscape-primary".to_string()
        } else {
            "portrait-primary".to_string()
        };
        let orientation_angle = if width >= height { 0 } else { 90 };

        Self {
            width,
            height,
            // Account for taskbar (Windows ~40px, macOS ~25px)
            avail_width: width,
            avail_height: height.saturating_sub(40),
            // Default outer dimensions (will be synced via sync_to_viewport)
            outer_width: width,
            outer_height: height,
            orientation_type,
            orientation_angle,
        }
    }

    /// Common screen resolutions
    pub fn common_resolutions() -> Vec<ScreenResolution> {
        vec![
            ScreenResolution::new(1920, 1080), // Full HD (most common)
            ScreenResolution::new(2560, 1440), // QHD
            ScreenResolution::new(3840, 2160), // 4K
            ScreenResolution::new(1366, 768),  // Laptop HD
            ScreenResolution::new(1536, 864),  // Laptop
            ScreenResolution::new(1440, 900),  // MacBook
            ScreenResolution::new(1680, 1050), // WSXGA+
            ScreenResolution::new(2560, 1600), // MacBook Pro 13"
            ScreenResolution::new(2880, 1800), // MacBook Pro 15"
        ]
    }
}

/// Plugin information for fingerprint
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PluginEntry {
    pub name: String,
    pub description: String,
    pub filename: String,
}

/// Font entry for fingerprint
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FontEntry {
    pub name: String,
}
