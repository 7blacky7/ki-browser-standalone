//! Fingerprint generation engine with user agent database.
//!
//! Provides [`FingerprintGenerator`] for creating random, consistent, or
//! profile-based browser fingerprints, and [`UserAgentDatabase`] containing
//! realistic user agent strings for each browser/OS combination.

use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};

use super::types::{
    FingerprintProfile, FontEntry, PluginEntry, ScreenResolution,
};
use super::fingerprint::BrowserFingerprint;

/// Fingerprint generator for creating browser fingerprints
#[derive(Debug, Clone)]
pub struct FingerprintGenerator {
    /// Available user agents by profile
    user_agents: UserAgentDatabase,
}

impl FingerprintGenerator {
    /// Create a new fingerprint generator
    pub fn new() -> Self {
        Self {
            user_agents: UserAgentDatabase::new(),
        }
    }

    /// Generate a completely random fingerprint
    pub fn generate_random(&self) -> BrowserFingerprint {
        use std::time::{SystemTime, UNIX_EPOCH};
        let seed = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos() as u64;

        let profiles = FingerprintProfile::all_standard();
        let profile_index = (seed as usize) % profiles.len();
        let profile = profiles[profile_index].clone();

        self.generate_with_seed(seed, profile)
    }

    /// Generate a consistent fingerprint based on a seed string
    ///
    /// The same seed will always produce the same fingerprint, making it
    /// useful for maintaining identity across sessions.
    pub fn generate_consistent(&self, seed: &str) -> BrowserFingerprint {
        let mut hasher = DefaultHasher::new();
        seed.hash(&mut hasher);
        let hash = hasher.finish();

        let profiles = FingerprintProfile::all_standard();
        let profile_index = (hash as usize) % profiles.len();
        let profile = profiles[profile_index].clone();

        self.generate_with_seed(hash, profile)
    }

    /// Generate a fingerprint for a specific profile
    pub fn generate_from_profile(&self, profile: FingerprintProfile) -> BrowserFingerprint {
        use std::time::{SystemTime, UNIX_EPOCH};
        let seed = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos() as u64;

        self.generate_with_seed(seed, profile)
    }

    /// Generate a fingerprint with a specific seed and profile
    fn generate_with_seed(&self, seed: u64, profile: FingerprintProfile) -> BrowserFingerprint {
        let resolutions = ScreenResolution::common_resolutions();
        let resolution_index = (seed as usize) % resolutions.len();
        let resolution = resolutions[resolution_index].clone();

        let user_agent = self.user_agents.get_user_agent(&profile, seed);
        let (timezone, timezone_offset) = self.get_timezone(seed);
        let languages = self.get_languages(&profile, seed);

        BrowserFingerprint {
            user_agent,
            platform: profile.platform().to_string(),
            vendor: profile.vendor().to_string(),
            language: languages[0].clone(),
            languages,
            screen_resolution: resolution,
            color_depth: 24,
            pixel_depth: 24,
            timezone_offset,
            timezone,
            plugins: self.get_plugins(&profile),
            fonts: self.get_fonts(&profile),
            do_not_track: if seed.is_multiple_of(3) {
                Some("1".to_string())
            } else {
                None
            },
            cookie_enabled: true,
            profile,
        }
    }

    fn get_timezone(&self, seed: u64) -> (String, i32) {
        let timezones = vec![
            ("America/New_York", -300),
            ("America/Chicago", -360),
            ("America/Denver", -420),
            ("America/Los_Angeles", -480),
            ("Europe/London", 0),
            ("Europe/Paris", 60),
            ("Europe/Berlin", 60),
            ("Asia/Tokyo", 540),
            ("Asia/Shanghai", 480),
            ("Australia/Sydney", 600),
        ];
        let index = (seed as usize) % timezones.len();
        let (tz, offset) = timezones[index];
        (tz.to_string(), offset)
    }

    fn get_languages(&self, profile: &FingerprintProfile, seed: u64) -> Vec<String> {
        let language_sets = match profile {
            FingerprintProfile::WindowsChrome
            | FingerprintProfile::WindowsFirefox
            | FingerprintProfile::WindowsEdge => vec![
                vec!["en-US", "en"],
                vec!["en-US", "en", "es"],
                vec!["en-GB", "en"],
            ],
            FingerprintProfile::MacChrome
            | FingerprintProfile::MacSafari
            | FingerprintProfile::MacFirefox => vec![
                vec!["en-US", "en"],
                vec!["en-US", "en", "fr"],
                vec!["en-GB", "en"],
            ],
            FingerprintProfile::LinuxChrome | FingerprintProfile::LinuxFirefox => vec![
                vec!["en-US", "en"],
                vec!["en-US", "en", "de"],
                vec!["en-GB", "en"],
            ],
            FingerprintProfile::Custom => vec![vec!["en-US", "en"]],
        };

        let index = (seed as usize) % language_sets.len();
        language_sets[index].iter().map(|s| s.to_string()).collect()
    }

    fn get_plugins(&self, profile: &FingerprintProfile) -> Vec<PluginEntry> {
        match profile {
            FingerprintProfile::WindowsChrome
            | FingerprintProfile::MacChrome
            | FingerprintProfile::LinuxChrome
            | FingerprintProfile::WindowsEdge => vec![
                PluginEntry {
                    name: "PDF Viewer".to_string(),
                    description: "Portable Document Format".to_string(),
                    filename: "internal-pdf-viewer".to_string(),
                },
                PluginEntry {
                    name: "Chrome PDF Viewer".to_string(),
                    description: "Portable Document Format".to_string(),
                    filename: "internal-pdf-viewer".to_string(),
                },
                PluginEntry {
                    name: "Chromium PDF Viewer".to_string(),
                    description: "Portable Document Format".to_string(),
                    filename: "internal-pdf-viewer".to_string(),
                },
                PluginEntry {
                    name: "Microsoft Edge PDF Viewer".to_string(),
                    description: "Portable Document Format".to_string(),
                    filename: "internal-pdf-viewer".to_string(),
                },
                PluginEntry {
                    name: "WebKit built-in PDF".to_string(),
                    description: "Portable Document Format".to_string(),
                    filename: "internal-pdf-viewer".to_string(),
                },
            ],
            FingerprintProfile::WindowsFirefox
            | FingerprintProfile::MacFirefox
            | FingerprintProfile::LinuxFirefox => vec![
                // Firefox typically has fewer plugins visible
            ],
            FingerprintProfile::MacSafari => vec![PluginEntry {
                name: "WebKit built-in PDF".to_string(),
                description: "Portable Document Format".to_string(),
                filename: "WebKitPDFPlugin".to_string(),
            }],
            FingerprintProfile::Custom => vec![],
        }
    }

    fn get_fonts(&self, profile: &FingerprintProfile) -> Vec<FontEntry> {
        let base_fonts = vec![
            "Arial",
            "Arial Black",
            "Comic Sans MS",
            "Courier New",
            "Georgia",
            "Impact",
            "Times New Roman",
            "Trebuchet MS",
            "Verdana",
        ];

        let mut fonts: Vec<FontEntry> = base_fonts
            .iter()
            .map(|name| FontEntry {
                name: name.to_string(),
            })
            .collect();

        // Add platform-specific fonts
        match profile {
            FingerprintProfile::WindowsChrome
            | FingerprintProfile::WindowsFirefox
            | FingerprintProfile::WindowsEdge => {
                fonts.extend(
                    ["Calibri",
                        "Cambria",
                        "Consolas",
                        "Segoe UI",
                        "Tahoma",
                        "Microsoft Sans Serif"]
                    .iter()
                    .map(|name| FontEntry {
                        name: name.to_string(),
                    }),
                );
            }
            FingerprintProfile::MacChrome
            | FingerprintProfile::MacSafari
            | FingerprintProfile::MacFirefox => {
                fonts.extend(
                    ["Helvetica",
                        "Helvetica Neue",
                        "Lucida Grande",
                        "Monaco",
                        "Menlo",
                        "SF Pro"]
                    .iter()
                    .map(|name| FontEntry {
                        name: name.to_string(),
                    }),
                );
            }
            FingerprintProfile::LinuxChrome | FingerprintProfile::LinuxFirefox => {
                fonts.extend(
                    ["DejaVu Sans",
                        "DejaVu Serif",
                        "Liberation Sans",
                        "Liberation Serif",
                        "Ubuntu",
                        "Noto Sans"]
                    .iter()
                    .map(|name| FontEntry {
                        name: name.to_string(),
                    }),
                );
            }
            FingerprintProfile::Custom => {}
        }

        fonts
    }
}

impl Default for FingerprintGenerator {
    fn default() -> Self {
        Self::new()
    }
}

/// Database of realistic user agent strings
#[derive(Debug, Clone)]
struct UserAgentDatabase {
    windows_chrome: Vec<String>,
    windows_firefox: Vec<String>,
    windows_edge: Vec<String>,
    mac_chrome: Vec<String>,
    mac_safari: Vec<String>,
    mac_firefox: Vec<String>,
    linux_chrome: Vec<String>,
    linux_firefox: Vec<String>,
}

impl UserAgentDatabase {
    fn new() -> Self {
        Self {
            windows_chrome: vec![
                "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/120.0.0.0 Safari/537.36".to_string(),
                "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/119.0.0.0 Safari/537.36".to_string(),
                "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/121.0.0.0 Safari/537.36".to_string(),
                "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/122.0.0.0 Safari/537.36".to_string(),
            ],
            windows_firefox: vec![
                "Mozilla/5.0 (Windows NT 10.0; Win64; x64; rv:121.0) Gecko/20100101 Firefox/121.0".to_string(),
                "Mozilla/5.0 (Windows NT 10.0; Win64; x64; rv:120.0) Gecko/20100101 Firefox/120.0".to_string(),
                "Mozilla/5.0 (Windows NT 10.0; Win64; x64; rv:122.0) Gecko/20100101 Firefox/122.0".to_string(),
            ],
            windows_edge: vec![
                "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/120.0.0.0 Safari/537.36 Edg/120.0.0.0".to_string(),
                "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/121.0.0.0 Safari/537.36 Edg/121.0.0.0".to_string(),
            ],
            mac_chrome: vec![
                "Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/120.0.0.0 Safari/537.36".to_string(),
                "Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/121.0.0.0 Safari/537.36".to_string(),
                "Mozilla/5.0 (Macintosh; Intel Mac OS X 14_0) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/120.0.0.0 Safari/537.36".to_string(),
            ],
            mac_safari: vec![
                "Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7) AppleWebKit/605.1.15 (KHTML, like Gecko) Version/17.2 Safari/605.1.15".to_string(),
                "Mozilla/5.0 (Macintosh; Intel Mac OS X 14_0) AppleWebKit/605.1.15 (KHTML, like Gecko) Version/17.1 Safari/605.1.15".to_string(),
            ],
            mac_firefox: vec![
                "Mozilla/5.0 (Macintosh; Intel Mac OS X 10.15; rv:121.0) Gecko/20100101 Firefox/121.0".to_string(),
                "Mozilla/5.0 (Macintosh; Intel Mac OS X 14.0; rv:120.0) Gecko/20100101 Firefox/120.0".to_string(),
            ],
            linux_chrome: vec![
                "Mozilla/5.0 (X11; Linux x86_64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/120.0.0.0 Safari/537.36".to_string(),
                "Mozilla/5.0 (X11; Linux x86_64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/121.0.0.0 Safari/537.36".to_string(),
            ],
            linux_firefox: vec![
                "Mozilla/5.0 (X11; Linux x86_64; rv:121.0) Gecko/20100101 Firefox/121.0".to_string(),
                "Mozilla/5.0 (X11; Ubuntu; Linux x86_64; rv:120.0) Gecko/20100101 Firefox/120.0".to_string(),
            ],
        }
    }

    fn get_user_agent(&self, profile: &FingerprintProfile, seed: u64) -> String {
        let agents = match profile {
            FingerprintProfile::WindowsChrome => &self.windows_chrome,
            FingerprintProfile::WindowsFirefox => &self.windows_firefox,
            FingerprintProfile::WindowsEdge => &self.windows_edge,
            FingerprintProfile::MacChrome => &self.mac_chrome,
            FingerprintProfile::MacSafari => &self.mac_safari,
            FingerprintProfile::MacFirefox => &self.mac_firefox,
            FingerprintProfile::LinuxChrome => &self.linux_chrome,
            FingerprintProfile::LinuxFirefox => &self.linux_firefox,
            FingerprintProfile::Custom => &self.windows_chrome,
        };

        let index = (seed as usize) % agents.len();
        agents[index].clone()
    }
}
