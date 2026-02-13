//! Browser settings and configuration management.
//!
//! This module provides comprehensive configuration options for the ki-browser-standalone
//! application, supporting multiple configuration sources with proper precedence.

use serde::{Deserialize, Serialize};
use std::env;
use std::fs;
use std::path::{Path, PathBuf};
use thiserror::Error;

/// Errors that can occur during configuration loading or validation.
#[derive(Debug, Error)]
pub enum ConfigError {
    /// Failed to read configuration file.
    #[error("Failed to read configuration file: {0}")]
    IoError(#[from] std::io::Error),

    /// Failed to parse TOML configuration.
    #[error("Failed to parse TOML configuration: {0}")]
    TomlParseError(#[from] toml::de::Error),

    /// Failed to serialize TOML configuration.
    #[error("Failed to serialize TOML configuration: {0}")]
    TomlSerializeError(#[from] toml::ser::Error),

    /// Failed to parse JSON configuration.
    #[error("Failed to parse JSON configuration: {0}")]
    JsonError(#[from] serde_json::Error),

    /// Invalid configuration value.
    #[error("Invalid configuration: {0}")]
    ValidationError(String),

    /// Unsupported file format.
    #[error("Unsupported configuration file format: {0}")]
    UnsupportedFormat(String),
}

/// Proxy type enumeration.
///
/// Defines the type of proxy connection to use.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ProxyType {
    /// HTTP proxy.
    Http,
    /// HTTPS proxy.
    Https,
    /// SOCKS5 proxy.
    Socks5,
}

impl Default for ProxyType {
    fn default() -> Self {
        Self::Http
    }
}

impl std::fmt::Display for ProxyType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ProxyType::Http => write!(f, "http"),
            ProxyType::Https => write!(f, "https"),
            ProxyType::Socks5 => write!(f, "socks5"),
        }
    }
}

impl std::str::FromStr for ProxyType {
    type Err = ConfigError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_lowercase().as_str() {
            "http" => Ok(ProxyType::Http),
            "https" => Ok(ProxyType::Https),
            "socks5" | "socks" => Ok(ProxyType::Socks5),
            _ => Err(ConfigError::ValidationError(format!(
                "Unknown proxy type: {}. Valid types are: http, https, socks5",
                s
            ))),
        }
    }
}

/// Proxy configuration settings.
///
/// Defines all parameters needed to connect through a proxy server.
///
/// # Example
///
/// ```rust
/// use ki_browser_standalone::config::{ProxyConfig, ProxyType};
///
/// let proxy = ProxyConfig {
///     host: "proxy.example.com".to_string(),
///     port: 8080,
///     username: Some("user".to_string()),
///     password: Some("pass".to_string()),
///     proxy_type: ProxyType::Http,
/// };
/// ```
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProxyConfig {
    /// Proxy server hostname or IP address.
    pub host: String,

    /// Proxy server port.
    pub port: u16,

    /// Optional username for proxy authentication.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub username: Option<String>,

    /// Optional password for proxy authentication.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub password: Option<String>,

    /// Type of proxy (HTTP, HTTPS, or SOCKS5).
    #[serde(default)]
    pub proxy_type: ProxyType,
}

impl ProxyConfig {
    /// Creates a new proxy configuration.
    ///
    /// # Arguments
    ///
    /// * `host` - Proxy server hostname or IP address
    /// * `port` - Proxy server port
    ///
    /// # Example
    ///
    /// ```rust
    /// use ki_browser_standalone::config::ProxyConfig;
    ///
    /// let proxy = ProxyConfig::new("localhost", 8080);
    /// ```
    pub fn new(host: impl Into<String>, port: u16) -> Self {
        Self {
            host: host.into(),
            port,
            username: None,
            password: None,
            proxy_type: ProxyType::default(),
        }
    }

    /// Sets the proxy type.
    pub fn with_type(mut self, proxy_type: ProxyType) -> Self {
        self.proxy_type = proxy_type;
        self
    }

    /// Sets authentication credentials.
    pub fn with_auth(mut self, username: impl Into<String>, password: impl Into<String>) -> Self {
        self.username = Some(username.into());
        self.password = Some(password.into());
        self
    }

    /// Returns the proxy URL string.
    ///
    /// # Example
    ///
    /// ```rust
    /// use ki_browser_standalone::config::{ProxyConfig, ProxyType};
    ///
    /// let proxy = ProxyConfig::new("localhost", 8080).with_type(ProxyType::Socks5);
    /// assert_eq!(proxy.to_url(), "socks5://localhost:8080");
    /// ```
    pub fn to_url(&self) -> String {
        let auth = match (&self.username, &self.password) {
            (Some(user), Some(pass)) => format!("{}:{}@", user, pass),
            (Some(user), None) => format!("{}@", user),
            _ => String::new(),
        };
        format!("{}://{}{}:{}", self.proxy_type, auth, self.host, self.port)
    }

    /// Validates the proxy configuration.
    pub fn validate(&self) -> Result<(), ConfigError> {
        if self.host.is_empty() {
            return Err(ConfigError::ValidationError(
                "Proxy host cannot be empty".to_string(),
            ));
        }
        if self.port == 0 {
            return Err(ConfigError::ValidationError(
                "Proxy port cannot be 0".to_string(),
            ));
        }
        Ok(())
    }
}

/// Main browser settings configuration.
///
/// This struct contains all configurable options for the browser instance.
/// Settings can be loaded from files, environment variables, or CLI arguments.
///
/// # Configuration Precedence
///
/// Settings are applied in the following order (later sources override earlier):
/// 1. Default values
/// 2. Configuration file (TOML or JSON)
/// 3. Environment variables
/// 4. CLI arguments
///
/// # Example
///
/// ```rust
/// use ki_browser_standalone::config::BrowserSettings;
///
/// let settings = BrowserSettings::default()
///     .with_headless(true)
///     .with_window_size(1920, 1080);
/// ```
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BrowserSettings {
    /// Browser window width in pixels.
    #[serde(default = "default_window_width")]
    pub window_width: u32,

    /// Browser window height in pixels.
    #[serde(default = "default_window_height")]
    pub window_height: u32,

    /// Run browser in headless mode (no visible window).
    #[serde(default)]
    pub headless: bool,

    /// Custom user agent string.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub user_agent: Option<String>,

    /// Proxy configuration.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub proxy: Option<ProxyConfig>,

    /// Enable the HTTP API server.
    #[serde(default = "default_api_enabled")]
    pub api_enabled: bool,

    /// Port for the HTTP API server.
    #[serde(default = "default_api_port")]
    pub api_port: u16,

    /// Enable stealth mode to avoid bot detection.
    #[serde(default)]
    pub stealth_mode: bool,

    /// Path to browser profile directory for persistent storage.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub profile_path: Option<PathBuf>,

    /// Maximum number of concurrent tabs allowed.
    #[serde(default = "default_max_tabs")]
    pub max_tabs: usize,

    /// Default timeout for operations in milliseconds.
    #[serde(default = "default_timeout_ms")]
    pub default_timeout_ms: u64,
}

// Default value functions for serde
fn default_window_width() -> u32 {
    1280
}

fn default_window_height() -> u32 {
    720
}

fn default_api_enabled() -> bool {
    true
}

fn default_api_port() -> u16 {
    9222
}

fn default_max_tabs() -> usize {
    10
}

fn default_timeout_ms() -> u64 {
    30000
}

impl Default for BrowserSettings {
    fn default() -> Self {
        Self {
            window_width: default_window_width(),
            window_height: default_window_height(),
            headless: false,
            user_agent: None,
            proxy: None,
            api_enabled: default_api_enabled(),
            api_port: default_api_port(),
            stealth_mode: false,
            profile_path: None,
            max_tabs: default_max_tabs(),
            default_timeout_ms: default_timeout_ms(),
        }
    }
}

impl BrowserSettings {
    /// Creates a new BrowserSettings with default values.
    pub fn new() -> Self {
        Self::default()
    }

    /// Loads settings from a configuration file.
    ///
    /// Supports both TOML and JSON formats, detected by file extension.
    ///
    /// # Arguments
    ///
    /// * `path` - Path to the configuration file
    ///
    /// # Errors
    ///
    /// Returns an error if the file cannot be read or parsed.
    ///
    /// # Example
    ///
    /// ```rust,no_run
    /// use ki_browser_standalone::config::BrowserSettings;
    ///
    /// let settings = BrowserSettings::from_file("config.toml").unwrap();
    /// ```
    pub fn from_file<P: AsRef<Path>>(path: P) -> Result<Self, ConfigError> {
        let path = path.as_ref();
        let content = fs::read_to_string(path)?;

        let extension = path
            .extension()
            .and_then(|e| e.to_str())
            .unwrap_or("")
            .to_lowercase();

        match extension.as_str() {
            "toml" => Ok(toml::from_str(&content)?),
            "json" => Ok(serde_json::from_str(&content)?),
            ext => Err(ConfigError::UnsupportedFormat(ext.to_string())),
        }
    }

    /// Saves settings to a configuration file.
    ///
    /// The format is determined by the file extension.
    ///
    /// # Arguments
    ///
    /// * `path` - Path where the configuration file will be saved
    ///
    /// # Errors
    ///
    /// Returns an error if the file cannot be written.
    ///
    /// # Example
    ///
    /// ```rust,no_run
    /// use ki_browser_standalone::config::BrowserSettings;
    ///
    /// let settings = BrowserSettings::default();
    /// settings.to_file("config.toml").unwrap();
    /// ```
    pub fn to_file<P: AsRef<Path>>(&self, path: P) -> Result<(), ConfigError> {
        let path = path.as_ref();
        let extension = path
            .extension()
            .and_then(|e| e.to_str())
            .unwrap_or("")
            .to_lowercase();

        let content = match extension.as_str() {
            "toml" => toml::to_string_pretty(self)?,
            "json" => serde_json::to_string_pretty(self)?,
            ext => return Err(ConfigError::UnsupportedFormat(ext.to_string())),
        };

        fs::write(path, content)?;
        Ok(())
    }

    /// Loads settings from environment variables.
    ///
    /// Environment variables are prefixed with `KI_BROWSER_` and use uppercase
    /// names with underscores. For example:
    /// - `KI_BROWSER_WINDOW_WIDTH`
    /// - `KI_BROWSER_HEADLESS`
    /// - `KI_BROWSER_API_PORT`
    ///
    /// # Example
    ///
    /// ```rust
    /// use ki_browser_standalone::config::BrowserSettings;
    ///
    /// // With KI_BROWSER_HEADLESS=true set in environment
    /// let settings = BrowserSettings::from_env();
    /// ```
    pub fn from_env() -> Self {
        let mut settings = Self::default();
        settings.apply_env_overrides();
        settings
    }

    /// Applies environment variable overrides to current settings.
    fn apply_env_overrides(&mut self) {
        if let Ok(val) = env::var("KI_BROWSER_WINDOW_WIDTH") {
            if let Ok(width) = val.parse() {
                self.window_width = width;
            }
        }

        if let Ok(val) = env::var("KI_BROWSER_WINDOW_HEIGHT") {
            if let Ok(height) = val.parse() {
                self.window_height = height;
            }
        }

        if let Ok(val) = env::var("KI_BROWSER_HEADLESS") {
            self.headless = val.to_lowercase() == "true" || val == "1";
        }

        if let Ok(val) = env::var("KI_BROWSER_USER_AGENT") {
            self.user_agent = Some(val);
        }

        if let Ok(val) = env::var("KI_BROWSER_API_ENABLED") {
            self.api_enabled = val.to_lowercase() == "true" || val == "1";
        }

        if let Ok(val) = env::var("KI_BROWSER_API_PORT") {
            if let Ok(port) = val.parse() {
                self.api_port = port;
            }
        }

        if let Ok(val) = env::var("KI_BROWSER_STEALTH_MODE") {
            self.stealth_mode = val.to_lowercase() == "true" || val == "1";
        }

        if let Ok(val) = env::var("KI_BROWSER_PROFILE_PATH") {
            self.profile_path = Some(PathBuf::from(val));
        }

        if let Ok(val) = env::var("KI_BROWSER_MAX_TABS") {
            if let Ok(max) = val.parse() {
                self.max_tabs = max;
            }
        }

        if let Ok(val) = env::var("KI_BROWSER_DEFAULT_TIMEOUT_MS") {
            if let Ok(timeout) = val.parse() {
                self.default_timeout_ms = timeout;
            }
        }

        // Proxy configuration from environment
        if let Ok(host) = env::var("KI_BROWSER_PROXY_HOST") {
            let port = env::var("KI_BROWSER_PROXY_PORT")
                .ok()
                .and_then(|p| p.parse().ok())
                .unwrap_or(8080);

            let mut proxy = ProxyConfig::new(host, port);

            if let Ok(proxy_type) = env::var("KI_BROWSER_PROXY_TYPE") {
                if let Ok(pt) = proxy_type.parse() {
                    proxy.proxy_type = pt;
                }
            }

            if let Ok(username) = env::var("KI_BROWSER_PROXY_USERNAME") {
                proxy.username = Some(username);
            }

            if let Ok(password) = env::var("KI_BROWSER_PROXY_PASSWORD") {
                proxy.password = Some(password);
            }

            self.proxy = Some(proxy);
        }
    }

    /// Merges current settings with environment variable overrides.
    ///
    /// Returns a new settings instance with environment overrides applied.
    pub fn merge_with_env(mut self) -> Self {
        self.apply_env_overrides();
        self
    }

    /// Merges settings with CLI arguments.
    ///
    /// This method accepts parsed CLI arguments and applies them as overrides.
    ///
    /// # Arguments
    ///
    /// * `args` - A struct containing parsed CLI arguments
    ///
    /// # Example
    ///
    /// ```rust
    /// use ki_browser_standalone::config::{BrowserSettings, CliArgs};
    ///
    /// let args = CliArgs {
    ///     headless: Some(true),
    ///     width: Some(1920),
    ///     ..Default::default()
    /// };
    ///
    /// let settings = BrowserSettings::default().merge_with_args(&args);
    /// ```
    pub fn merge_with_args(mut self, args: &CliArgs) -> Self {
        if let Some(width) = args.width {
            self.window_width = width;
        }
        if let Some(height) = args.height {
            self.window_height = height;
        }
        if let Some(headless) = args.headless {
            self.headless = headless;
        }
        if let Some(ref user_agent) = args.user_agent {
            self.user_agent = Some(user_agent.clone());
        }
        if let Some(api_enabled) = args.api_enabled {
            self.api_enabled = api_enabled;
        }
        if let Some(api_port) = args.api_port {
            self.api_port = api_port;
        }
        if let Some(stealth) = args.stealth_mode {
            self.stealth_mode = stealth;
        }
        if let Some(ref profile) = args.profile_path {
            self.profile_path = Some(profile.clone());
        }
        if let Some(max_tabs) = args.max_tabs {
            self.max_tabs = max_tabs;
        }
        if let Some(timeout) = args.timeout_ms {
            self.default_timeout_ms = timeout;
        }

        // Handle proxy from CLI
        if let Some(ref proxy_host) = args.proxy_host {
            let port = args.proxy_port.unwrap_or(8080);
            let mut proxy = ProxyConfig::new(proxy_host, port);

            if let Some(ref proxy_type) = args.proxy_type {
                if let Ok(pt) = proxy_type.parse() {
                    proxy.proxy_type = pt;
                }
            }
            if let Some(ref username) = args.proxy_username {
                proxy.username = Some(username.clone());
            }
            if let Some(ref password) = args.proxy_password {
                proxy.password = Some(password.clone());
            }

            self.proxy = Some(proxy);
        }

        self
    }

    /// Validates all settings.
    ///
    /// # Errors
    ///
    /// Returns an error if any setting is invalid.
    ///
    /// # Example
    ///
    /// ```rust
    /// use ki_browser_standalone::config::BrowserSettings;
    ///
    /// let settings = BrowserSettings::default();
    /// assert!(settings.validate().is_ok());
    /// ```
    pub fn validate(&self) -> Result<(), ConfigError> {
        // Validate window dimensions
        if self.window_width < 100 {
            return Err(ConfigError::ValidationError(
                "Window width must be at least 100 pixels".to_string(),
            ));
        }
        if self.window_width > 7680 {
            return Err(ConfigError::ValidationError(
                "Window width cannot exceed 7680 pixels (8K)".to_string(),
            ));
        }
        if self.window_height < 100 {
            return Err(ConfigError::ValidationError(
                "Window height must be at least 100 pixels".to_string(),
            ));
        }
        if self.window_height > 4320 {
            return Err(ConfigError::ValidationError(
                "Window height cannot exceed 4320 pixels (8K)".to_string(),
            ));
        }

        // Validate API port
        if self.api_enabled && self.api_port == 0 {
            return Err(ConfigError::ValidationError(
                "API port cannot be 0 when API is enabled".to_string(),
            ));
        }

        // Validate max tabs
        if self.max_tabs == 0 {
            return Err(ConfigError::ValidationError(
                "Maximum tabs must be at least 1".to_string(),
            ));
        }
        if self.max_tabs > 100 {
            return Err(ConfigError::ValidationError(
                "Maximum tabs cannot exceed 100".to_string(),
            ));
        }

        // Validate timeout
        if self.default_timeout_ms < 1000 {
            return Err(ConfigError::ValidationError(
                "Default timeout must be at least 1000ms".to_string(),
            ));
        }
        if self.default_timeout_ms > 300000 {
            return Err(ConfigError::ValidationError(
                "Default timeout cannot exceed 300000ms (5 minutes)".to_string(),
            ));
        }

        // Validate proxy if present
        if let Some(ref proxy) = self.proxy {
            proxy.validate()?;
        }

        // Validate profile path if present
        if let Some(ref path) = self.profile_path {
            if let Some(parent) = path.parent() {
                if !parent.as_os_str().is_empty() && !parent.exists() {
                    return Err(ConfigError::ValidationError(format!(
                        "Profile path parent directory does not exist: {}",
                        parent.display()
                    )));
                }
            }
        }

        Ok(())
    }

    // Builder-style methods for convenient configuration

    /// Sets the window size.
    pub fn with_window_size(mut self, width: u32, height: u32) -> Self {
        self.window_width = width;
        self.window_height = height;
        self
    }

    /// Sets headless mode.
    pub fn with_headless(mut self, headless: bool) -> Self {
        self.headless = headless;
        self
    }

    /// Sets the user agent string.
    pub fn with_user_agent(mut self, user_agent: impl Into<String>) -> Self {
        self.user_agent = Some(user_agent.into());
        self
    }

    /// Sets the proxy configuration.
    pub fn with_proxy(mut self, proxy: ProxyConfig) -> Self {
        self.proxy = Some(proxy);
        self
    }

    /// Enables or disables the API server.
    pub fn with_api(mut self, enabled: bool, port: u16) -> Self {
        self.api_enabled = enabled;
        self.api_port = port;
        self
    }

    /// Enables or disables stealth mode.
    pub fn with_stealth_mode(mut self, stealth: bool) -> Self {
        self.stealth_mode = stealth;
        self
    }

    /// Sets the profile path.
    pub fn with_profile_path(mut self, path: impl Into<PathBuf>) -> Self {
        self.profile_path = Some(path.into());
        self
    }

    /// Sets the maximum number of tabs.
    pub fn with_max_tabs(mut self, max: usize) -> Self {
        self.max_tabs = max;
        self
    }

    /// Sets the default timeout in milliseconds.
    pub fn with_timeout(mut self, timeout_ms: u64) -> Self {
        self.default_timeout_ms = timeout_ms;
        self
    }
}

/// CLI argument structure for parsing command line options.
///
/// This struct is designed to work with argument parsing libraries like `clap`.
/// All fields are optional to allow partial overrides.
#[derive(Debug, Default, Clone)]
pub struct CliArgs {
    /// Browser window width.
    pub width: Option<u32>,
    /// Browser window height.
    pub height: Option<u32>,
    /// Enable headless mode.
    pub headless: Option<bool>,
    /// Custom user agent string.
    pub user_agent: Option<String>,
    /// Enable API server.
    pub api_enabled: Option<bool>,
    /// API server port.
    pub api_port: Option<u16>,
    /// Enable stealth mode.
    pub stealth_mode: Option<bool>,
    /// Browser profile path.
    pub profile_path: Option<PathBuf>,
    /// Maximum concurrent tabs.
    pub max_tabs: Option<usize>,
    /// Default operation timeout in milliseconds.
    pub timeout_ms: Option<u64>,
    /// Proxy host.
    pub proxy_host: Option<String>,
    /// Proxy port.
    pub proxy_port: Option<u16>,
    /// Proxy type (http, https, socks5).
    pub proxy_type: Option<String>,
    /// Proxy username.
    pub proxy_username: Option<String>,
    /// Proxy password.
    pub proxy_password: Option<String>,
    /// Configuration file path.
    pub config_file: Option<PathBuf>,
}

impl CliArgs {
    /// Creates an empty CliArgs instance.
    pub fn new() -> Self {
        Self::default()
    }

    /// Loads the final settings by applying the full configuration chain.
    ///
    /// This method handles the complete configuration precedence:
    /// 1. Default values
    /// 2. Configuration file (if specified)
    /// 3. Environment variables
    /// 4. CLI arguments (self)
    ///
    /// # Example
    ///
    /// ```rust,no_run
    /// use ki_browser_standalone::config::CliArgs;
    ///
    /// let args = CliArgs {
    ///     config_file: Some("config.toml".into()),
    ///     headless: Some(true),
    ///     ..Default::default()
    /// };
    ///
    /// let settings = args.load_settings().unwrap();
    /// ```
    pub fn load_settings(&self) -> Result<BrowserSettings, ConfigError> {
        // Start with defaults or file
        let mut settings = if let Some(ref config_file) = self.config_file {
            BrowserSettings::from_file(config_file)?
        } else {
            BrowserSettings::default()
        };

        // Apply environment overrides
        settings = settings.merge_with_env();

        // Apply CLI overrides
        settings = settings.merge_with_args(self);

        // Validate final settings
        settings.validate()?;

        Ok(settings)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_settings() {
        let settings = BrowserSettings::default();
        assert_eq!(settings.window_width, 1280);
        assert_eq!(settings.window_height, 720);
        assert!(!settings.headless);
        assert!(settings.api_enabled);
        assert_eq!(settings.api_port, 9222);
        assert_eq!(settings.max_tabs, 10);
        assert_eq!(settings.default_timeout_ms, 30000);
    }

    #[test]
    fn test_builder_methods() {
        let settings = BrowserSettings::default()
            .with_window_size(1920, 1080)
            .with_headless(true)
            .with_user_agent("TestAgent/1.0")
            .with_api(true, 8080)
            .with_stealth_mode(true)
            .with_max_tabs(5)
            .with_timeout(60000);

        assert_eq!(settings.window_width, 1920);
        assert_eq!(settings.window_height, 1080);
        assert!(settings.headless);
        assert_eq!(settings.user_agent, Some("TestAgent/1.0".to_string()));
        assert!(settings.api_enabled);
        assert_eq!(settings.api_port, 8080);
        assert!(settings.stealth_mode);
        assert_eq!(settings.max_tabs, 5);
        assert_eq!(settings.default_timeout_ms, 60000);
    }

    #[test]
    fn test_validation_valid_settings() {
        let settings = BrowserSettings::default();
        assert!(settings.validate().is_ok());
    }

    #[test]
    fn test_validation_invalid_width() {
        let settings = BrowserSettings::default().with_window_size(50, 720);
        assert!(settings.validate().is_err());
    }

    #[test]
    fn test_validation_invalid_max_tabs() {
        let mut settings = BrowserSettings::default();
        settings.max_tabs = 0;
        assert!(settings.validate().is_err());
    }

    #[test]
    fn test_proxy_config() {
        let proxy = ProxyConfig::new("localhost", 8080)
            .with_type(ProxyType::Socks5)
            .with_auth("user", "pass");

        assert_eq!(proxy.host, "localhost");
        assert_eq!(proxy.port, 8080);
        assert_eq!(proxy.proxy_type, ProxyType::Socks5);
        assert_eq!(proxy.username, Some("user".to_string()));
        assert_eq!(proxy.password, Some("pass".to_string()));
        assert_eq!(proxy.to_url(), "socks5://user:pass@localhost:8080");
    }

    #[test]
    fn test_proxy_type_parsing() {
        assert_eq!("http".parse::<ProxyType>().unwrap(), ProxyType::Http);
        assert_eq!("https".parse::<ProxyType>().unwrap(), ProxyType::Https);
        assert_eq!("socks5".parse::<ProxyType>().unwrap(), ProxyType::Socks5);
        assert!("invalid".parse::<ProxyType>().is_err());
    }

    #[test]
    fn test_cli_args_merge() {
        let args = CliArgs {
            width: Some(1920),
            headless: Some(true),
            ..Default::default()
        };

        let settings = BrowserSettings::default().merge_with_args(&args);

        assert_eq!(settings.window_width, 1920);
        assert_eq!(settings.window_height, 720); // Unchanged
        assert!(settings.headless);
    }

    #[test]
    fn test_toml_serialization() {
        let settings = BrowserSettings::default();
        let toml_str = toml::to_string_pretty(&settings).unwrap();
        let parsed: BrowserSettings = toml::from_str(&toml_str).unwrap();

        assert_eq!(settings.window_width, parsed.window_width);
        assert_eq!(settings.headless, parsed.headless);
        assert_eq!(settings.api_port, parsed.api_port);
    }

    #[test]
    fn test_json_serialization() {
        let settings = BrowserSettings::default();
        let json_str = serde_json::to_string_pretty(&settings).unwrap();
        let parsed: BrowserSettings = serde_json::from_str(&json_str).unwrap();

        assert_eq!(settings.window_width, parsed.window_width);
        assert_eq!(settings.headless, parsed.headless);
        assert_eq!(settings.api_port, parsed.api_port);
    }
}
