//! Configuration module for ki-browser-standalone.
//!
//! This module provides configuration management for the browser, including:
//! - Loading settings from files (TOML/JSON)
//! - Environment variable overrides
//! - CLI argument parsing
//! - Validation and defaults
//!
//! # Example
//!
//! ```rust,no_run
//! use ki_browser_standalone::config::BrowserSettings;
//!
//! // Load from default locations or create with defaults
//! let settings = BrowserSettings::default();
//!
//! // Load from a specific file
//! let settings = BrowserSettings::from_file("config.toml").unwrap();
//!
//! // Override with environment variables
//! let settings = settings.merge_with_env();
//! ```

mod settings;

pub use settings::{BrowserSettings, CliArgs, ConfigError, ProxyConfig, ProxyType};
