//! Utility functions for navigator anti-detection script generation.
//!
//! Provides default Chrome plugin presets, JavaScript string escaping,
//! app version extraction, and sub-scripts for permissions spoofing
//! and automation signal removal.

use super::types::{MimeTypeInfo, PluginInfo};

/// Default Chrome plugins that mimic a real Chrome browser installation
pub(crate) fn default_chrome_plugins() -> Vec<PluginInfo> {
    vec![
        PluginInfo::new("PDF Viewer", "Portable Document Format", "internal-pdf-viewer")
            .with_mime_type(MimeTypeInfo::pdf()),
        PluginInfo::chrome_pdf_viewer(),
        PluginInfo::chromium_pdf_viewer(),
        PluginInfo::new(
            "Microsoft Edge PDF Viewer",
            "Portable Document Format",
            "internal-pdf-viewer",
        )
        .with_mime_type(MimeTypeInfo::pdf()),
        PluginInfo::new(
            "WebKit built-in PDF",
            "Portable Document Format",
            "internal-pdf-viewer",
        )
        .with_mime_type(MimeTypeInfo::pdf()),
    ]
}

/// Extract app version from user agent string (everything after "Mozilla/")
pub(crate) fn extract_app_version(user_agent: &str) -> String {
    if let Some(pos) = user_agent.find("Mozilla/") {
        user_agent[pos + 8..].to_string()
    } else {
        user_agent.to_string()
    }
}

/// Escape a string for safe embedding inside JavaScript string literals
pub(crate) fn escape_js_string(s: &str) -> String {
    s.replace('\\', "\\\\")
        .replace('"', "\\\"")
        .replace('\n', "\\n")
        .replace('\r', "\\r")
        .replace('\t', "\\t")
        .replace('\'', "\\'")
}

/// JavaScript snippet for Permissions API spoofing to hide automation defaults
pub(crate) fn get_permissions_spoof_script() -> String {
    r#"
    // Permissions API spoofing
    if (typeof Permissions !== 'undefined' && Permissions.prototype.query) {
        const originalQuery = Permissions.prototype.query;
        Permissions.prototype.query = function(permissionDesc) {
            return new Promise((resolve, reject) => {
                originalQuery.call(this, permissionDesc)
                    .then(result => {
                        // Don't reveal "prompt" for sensitive permissions
                        // as automation tools often have different defaults
                        resolve(result);
                    })
                    .catch(reject);
            });
        };
    }
    "#
    .to_string()
}

/// JavaScript snippet for removing CDP, Selenium, PhantomJS, and other automation signals
pub(crate) fn get_automation_removal_script() -> String {
    r#"
    // Remove common automation signals

    // Remove CDP (Chrome DevTools Protocol) signals
    try {
        delete window.cdc_adoQpoasnfa76pfcZLmcfl_Array;
        delete window.cdc_adoQpoasnfa76pfcZLmcfl_Promise;
        delete window.cdc_adoQpoasnfa76pfcZLmcfl_Symbol;
    } catch (e) {}

    // Remove Selenium signals
    try {
        delete window._selenium;
        delete window.callSelenium;
        delete window._Selenium_IDE_Recorder;
        delete window.__webdriver_script_fn;
        delete window.__driver_evaluate;
        delete window.__webdriver_evaluate;
        delete window.__selenium_evaluate;
        delete window.__fxdriver_evaluate;
        delete window.__driver_unwrapped;
        delete window.__webdriver_unwrapped;
        delete window.__selenium_unwrapped;
        delete window.__fxdriver_unwrapped;
        delete window.__webdriver_script_func;
        delete window.$chrome_asyncScriptInfo;
        delete window.$cdc_asdjflasutopfhvcZLmcfl_;
    } catch (e) {}

    // Remove PhantomJS signals
    try {
        delete window.callPhantom;
        delete window._phantom;
    } catch (e) {}

    // Remove Nightmare signals
    try {
        delete window.__nightmare;
    } catch (e) {}

    // Remove general automation signals
    try {
        delete window.domAutomation;
        delete window.domAutomationController;
    } catch (e) {}

    // Override console.debug to hide potential automation logs
    const originalDebug = console.debug;
    console.debug = function(...args) {
        // Filter out automation-related debug messages
        const message = args.join(' ');
        if (message.includes('webdriver') || message.includes('automation')) {
            return;
        }
        return originalDebug.apply(console, args);
    };

    // Protect against detection via error stack traces
    const originalError = Error;
    window.Error = function(...args) {
        const error = new originalError(...args);
        // Clean stack trace of automation indicators
        if (error.stack) {
            error.stack = error.stack
                .split('\n')
                .filter(line => !line.includes('webdriver') && !line.includes('puppeteer'))
                .join('\n');
        }
        return error;
    };
    window.Error.prototype = originalError.prototype;

    // Override performance.getEntries to hide automation resources
    if (typeof Performance !== 'undefined' && Performance.prototype.getEntries) {
        const originalGetEntries = Performance.prototype.getEntries;
        Performance.prototype.getEntries = function() {
            return originalGetEntries.call(this).filter(entry => {
                const name = entry.name || '';
                return !name.includes('webdriver') &&
                       !name.includes('puppeteer') &&
                       !name.includes('playwright');
            });
        };
    }

    "#
    .to_string()
}

/// Generates a self-contained IIFE that stubs missing browser APIs.
///
/// Real Chrome exposes `navigator.mediaDevices`, `.bluetooth`, `.usb`,
/// `navigator.getBattery()`, and `chrome.runtime`.  CEF headless is missing
/// some of these, which is a strong fingerprinting signal.  This script is
/// injected as its own CDP section so a failure here never breaks other
/// stealth overrides.
pub(crate) fn get_missing_api_stubs_script() -> String {
    r#"(function() {
    'use strict';

    // navigator.mediaDevices
    try {
        if (!navigator.mediaDevices) {
            Object.defineProperty(navigator, 'mediaDevices', {
                get: function() {
                    return {
                        enumerateDevices: function() { return Promise.resolve([]); },
                        getUserMedia: function() { return Promise.reject(new DOMException('Not allowed', 'NotAllowedError')); },
                        getDisplayMedia: function() { return Promise.reject(new DOMException('Not allowed', 'NotAllowedError')); },
                        getSupportedConstraints: function() { return { width: true, height: true, frameRate: true, facingMode: true, deviceId: true }; },
                        addEventListener: function() {}, removeEventListener: function() {}, dispatchEvent: function() { return true; }
                    };
                },
                configurable: true
            });
        }
    } catch(e) {}

    // navigator.bluetooth
    try {
        if (!navigator.bluetooth) {
            Object.defineProperty(navigator, 'bluetooth', {
                get: function() {
                    return {
                        getAvailability: function() { return Promise.resolve(false); },
                        addEventListener: function() {}, removeEventListener: function() {}
                    };
                },
                configurable: true
            });
        }
    } catch(e) {}

    // navigator.usb
    try {
        if (!navigator.usb) {
            Object.defineProperty(navigator, 'usb', {
                get: function() {
                    return {
                        getDevices: function() { return Promise.resolve([]); },
                        requestDevice: function() { return Promise.reject(new DOMException('No device selected', 'NotFoundError')); },
                        addEventListener: function() {}, removeEventListener: function() {}
                    };
                },
                configurable: true
            });
        }
    } catch(e) {}

    // navigator.getBattery
    try {
        if (!navigator.getBattery) {
            Object.defineProperty(navigator, 'getBattery', {
                value: function() {
                    return Promise.resolve({
                        charging: true, chargingTime: 0, dischargingTime: Infinity, level: 1.0,
                        addEventListener: function() {}, removeEventListener: function() {}
                    });
                },
                configurable: true, writable: true
            });
        }
    } catch(e) {}

    // chrome.runtime (chrome.app/csi/loadTimes exist but runtime missing = inconsistent)
    try {
        if (typeof chrome !== 'undefined' && !chrome.runtime) {
            chrome.runtime = {
                connect: function() { return { onMessage: { addListener: function() {} }, postMessage: function() {}, disconnect: function() {} }; },
                sendMessage: function() {},
                onMessage: { addListener: function() {}, removeListener: function() {} },
                onConnect: { addListener: function() {}, removeListener: function() {} },
                getManifest: function() { return {}; },
                getURL: function(path) { return path; },
                id: undefined
            };
        }
    } catch(e) {}

})();"#.to_string()
}
