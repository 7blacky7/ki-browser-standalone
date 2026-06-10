//! Session bundle types — the JSON interface shared between the browser
//! extension (component A) and this backend (component B).
//!
//! A bundle captures everything needed to *inherit* a login session from a
//! real browser into a ki-browser tab: cookies (including httpOnly/secure),
//! per-origin local/sessionStorage, and the originating browser fingerprint.
//! The `fingerprint` object maps 1:1 onto [`crate::api::identity::IdentityOverrides`]
//! so a tab created from a bundle is fingerprint-consistent with the source.

use serde::{Deserialize, Serialize};

use crate::api::identity::{IdentityOverrides, IdentitySpec, ScreenSpec};

/// Current bundle schema version.
pub const BUNDLE_VERSION: u32 = 1;

/// A complete session bundle (cookies + storage + fingerprint).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Bundle {
    /// Schema version (currently always [`BUNDLE_VERSION`]).
    #[serde(default = "default_version")]
    pub version: u32,
    /// ISO-8601 creation timestamp (set by the grabber).
    #[serde(default)]
    pub created_at: Option<String>,
    /// Primary origin this session belongs to, e.g. `https://service.example.com`.
    pub origin: String,
    /// All cookies to restore.
    #[serde(default)]
    pub cookies: Vec<CookieSpec>,
    /// Per-origin local/sessionStorage entries.
    #[serde(default)]
    pub storage: Vec<StorageEntry>,
    /// Browser fingerprint, mappable onto a stealth identity.
    #[serde(default)]
    pub fingerprint: Option<FingerprintSpec>,
}

fn default_version() -> u32 {
    BUNDLE_VERSION
}

/// A single cookie. Field semantics follow CDP `Network.setCookie`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CookieSpec {
    pub name: String,
    pub value: String,
    pub domain: String,
    #[serde(default = "default_path")]
    pub path: String,
    #[serde(default)]
    pub secure: bool,
    #[serde(default, rename = "httpOnly")]
    pub http_only: bool,
    /// `"Strict" | "Lax" | "None"` — optional.
    #[serde(default, rename = "sameSite", skip_serializing_if = "Option::is_none")]
    pub same_site: Option<String>,
    /// Expiry as unix epoch seconds; omitted = session cookie.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub expires: Option<f64>,
}

fn default_path() -> String {
    "/".to_string()
}

/// local/sessionStorage contents for a single origin.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct StorageEntry {
    pub origin: String,
    #[serde(default)]
    pub local: std::collections::BTreeMap<String, String>,
    #[serde(default)]
    pub session: std::collections::BTreeMap<String, String>,
}

/// Browser fingerprint captured from the source browser.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct FingerprintSpec {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub user_agent: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub platform: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub languages: Option<Vec<String>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub hardware_concurrency: Option<u8>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub device_memory: Option<u8>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub screen: Option<ScreenSize>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub webgl_vendor: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub webgl_renderer: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub timezone: Option<String>,
}

/// Screen dimensions in the bundle fingerprint.
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct ScreenSize {
    pub width: u32,
    pub height: u32,
}

impl FingerprintSpec {
    /// Returns `true` if no fingerprint field is set (nothing to apply).
    pub fn is_empty(&self) -> bool {
        self.user_agent.is_none()
            && self.platform.is_none()
            && self.languages.is_none()
            && self.hardware_concurrency.is_none()
            && self.device_memory.is_none()
            && self.screen.is_none()
            && self.webgl_vendor.is_none()
            && self.webgl_renderer.is_none()
            && self.timezone.is_none()
    }

    /// Maps this fingerprint onto an [`IdentitySpec`] understood by
    /// [`crate::api::identity::resolve_identity`]. Returns `None` when the
    /// fingerprint carries no usable fields.
    pub fn to_identity_spec(&self) -> Option<IdentitySpec> {
        if self.is_empty() {
            return None;
        }
        Some(IdentitySpec::Explicit(IdentityOverrides {
            seed: None,
            user_agent: self.user_agent.clone(),
            platform: self.platform.clone(),
            languages: self.languages.clone(),
            hardware_concurrency: self.hardware_concurrency,
            device_memory: self.device_memory,
            webgl_vendor: self.webgl_vendor.clone(),
            webgl_renderer: self.webgl_renderer.clone(),
            screen: self
                .screen
                .map(|s| ScreenSpec { width: s.width, height: s.height }),
            timezone: self.timezone.clone(),
        }))
    }
}

/// Metadata about a stored session (no cookie values — safe to list).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SessionMeta {
    pub id: String,
    pub origin: String,
    #[serde(default)]
    pub created_at: Option<String>,
    /// Number of cookies in the bundle (count only, never values).
    pub cookie_count: usize,
    /// Distinct storage origins captured.
    pub storage_origins: usize,
}
