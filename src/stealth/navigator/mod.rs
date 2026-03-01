//! Navigator property management for anti-detection.
//!
//! Provides comprehensive navigator property overrides to prevent bot detection.
//! The most critical aspect is ensuring `navigator.webdriver` is NEVER exposed as `true`.
//!
//! # Submodules
//!
//! - `types` - Core structs: `PluginInfo`, `MimeTypeInfo`, `NavigatorOverrides`
//! - `script` - JavaScript override script generation for navigator spoofing
//! - `builder` - Builder pattern for constructing `NavigatorOverrides`
//! - `helpers` - Utility functions for JS escaping, default plugins, and sub-scripts

mod builder;
mod helpers;
mod script;
mod types;

pub use builder::NavigatorOverridesBuilder;
pub use types::{MimeTypeInfo, NavigatorOverrides, PluginInfo};
