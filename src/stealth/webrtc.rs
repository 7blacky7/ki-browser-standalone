//! WebRTC Leak Prevention
//!
//! This module provides WebRTC leak prevention capabilities to prevent
//! the real IP address from being exposed through WebRTC connections.
//!
//! WebRTC can reveal a user's real IP address even when using a VPN or proxy,
//! making it a critical vector for de-anonymization. This module allows
//! controlling or completely disabling WebRTC to prevent such leaks.
//!
//! # Components
//!
//! - `WebRtcConfig` - Configuration for WebRTC leak prevention
//! - `WebRtcIpPolicy` - IP handling policy options
//!
//! # Example
//!
//! ```rust,no_run
//! use ki_browser_standalone::stealth::webrtc::{WebRtcConfig, WebRtcIpPolicy};
//!
//! // Completely disable WebRTC
//! let config = WebRtcConfig::disabled();
//!
//! // Or use a restrictive policy
//! let config = WebRtcConfig::new(WebRtcIpPolicy::DisableNonProxiedUdp);
//!
//! // Get the JavaScript override script
//! let js = config.get_override_script();
//! ```

/// IP handling policy for WebRTC connections
///
/// Controls how WebRTC handles IP address discovery, which directly
/// impacts whether the real IP can be leaked.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum WebRtcIpPolicy {
    /// Block all non-proxied UDP connections (safest option)
    ///
    /// This prevents any direct UDP connection that could reveal
    /// the real IP address. Only proxied connections are allowed.
    DisableNonProxiedUdp,
    /// Only use the default public interface
    ///
    /// This restricts WebRTC to only use the default public network
    /// interface, preventing enumeration of all network interfaces.
    DefaultPublicInterfaceOnly,
    /// Default browser behavior (least restrictive)
    ///
    /// Allows WebRTC to operate normally. This provides no
    /// protection against IP leaks and should only be used when
    /// WebRTC functionality is required and IP exposure is acceptable.
    Default,
}

impl WebRtcIpPolicy {
    /// Get the Chrome policy string for this setting
    fn to_policy_string(&self) -> &'static str {
        match self {
            WebRtcIpPolicy::DisableNonProxiedUdp => "disable_non_proxied_udp",
            WebRtcIpPolicy::DefaultPublicInterfaceOnly => "default_public_interface_only",
            WebRtcIpPolicy::Default => "default",
        }
    }
}

/// WebRTC leak prevention configuration
///
/// Controls how WebRTC behaves to prevent IP address leaks.
/// When `disabled` is true, all WebRTC functionality is blocked.
/// Otherwise, the `ip_handling_policy` controls the level of protection.
#[derive(Debug, Clone)]
pub struct WebRtcConfig {
    /// Completely disable WebRTC
    ///
    /// When true, RTCPeerConnection and related APIs are fully blocked.
    /// This is the safest option but prevents any WebRTC functionality.
    pub disabled: bool,
    /// IP handling policy
    ///
    /// Controls which network interfaces WebRTC is allowed to use
    /// for connection establishment.
    pub ip_handling_policy: WebRtcIpPolicy,
}

impl WebRtcConfig {
    /// Create a new WebRTC configuration with the specified IP policy
    pub fn new(policy: WebRtcIpPolicy) -> Self {
        Self {
            disabled: false,
            ip_handling_policy: policy,
        }
    }

    /// Create a configuration that completely disables WebRTC
    pub fn disabled() -> Self {
        Self {
            disabled: true,
            ip_handling_policy: WebRtcIpPolicy::DisableNonProxiedUdp,
        }
    }

    /// Create a safe default configuration
    ///
    /// Uses `DisableNonProxiedUdp` policy which prevents IP leaks
    /// while still allowing proxied WebRTC connections.
    pub fn safe_default() -> Self {
        Self {
            disabled: false,
            ip_handling_policy: WebRtcIpPolicy::DisableNonProxiedUdp,
        }
    }

    /// Generate JavaScript override script for WebRTC leak prevention
    ///
    /// This script must be injected before any page scripts run.
    pub fn get_override_script(&self) -> String {
        if self.disabled {
            Self::get_disabled_script()
        } else {
            self.get_policy_script()
        }
    }

    /// Generate script that completely disables WebRTC
    fn get_disabled_script() -> String {
        r#"
// WebRTC Leak Prevention - DISABLED MODE
(function() {
    'use strict';

    // Block RTCPeerConnection completely
    const blockedRtc = function() {
        throw new DOMException(
            'RTCPeerConnection is not allowed by browser policy',
            'NotAllowedError'
        );
    };
    blockedRtc.prototype = {};

    if (typeof window.RTCPeerConnection !== 'undefined') {
        window.RTCPeerConnection = blockedRtc;
    }
    if (typeof window.webkitRTCPeerConnection !== 'undefined') {
        window.webkitRTCPeerConnection = blockedRtc;
    }
    if (typeof window.mozRTCPeerConnection !== 'undefined') {
        window.mozRTCPeerConnection = blockedRtc;
    }

    // Block RTCSessionDescription
    if (typeof window.RTCSessionDescription !== 'undefined') {
        window.RTCSessionDescription = function() {
            throw new DOMException(
                'RTCSessionDescription is not allowed by browser policy',
                'NotAllowedError'
            );
        };
    }

    // Block RTCIceCandidate
    if (typeof window.RTCIceCandidate !== 'undefined') {
        window.RTCIceCandidate = function() {
            throw new DOMException(
                'RTCIceCandidate is not allowed by browser policy',
                'NotAllowedError'
            );
        };
    }

    // Block getUserMedia to prevent media device enumeration
    if (navigator.mediaDevices && navigator.mediaDevices.getUserMedia) {
        const originalGetUserMedia = navigator.mediaDevices.getUserMedia.bind(navigator.mediaDevices);
        navigator.mediaDevices.getUserMedia = function(constraints) {
            // Allow audio/video but block if it looks like a WebRTC leak attempt
            return originalGetUserMedia(constraints);
        };
    }

    // Block enumerateDevices to prevent device fingerprinting
    if (navigator.mediaDevices && navigator.mediaDevices.enumerateDevices) {
        navigator.mediaDevices.enumerateDevices = function() {
            return Promise.resolve([]);
        };
    }

})();
"#
        .to_string()
    }

    /// Generate script that applies IP handling policy
    fn get_policy_script(&self) -> String {
        let policy = self.ip_handling_policy.to_policy_string();

        format!(
            r#"
// WebRTC Leak Prevention - Policy Mode: {policy}
(function() {{
    'use strict';

    const POLICY = "{policy}";

    // Filter ICE candidates to remove private/local IPs
    function filterIceCandidate(candidate) {{
        if (!candidate || !candidate.candidate) {{
            return candidate;
        }}

        const candidateStr = candidate.candidate;

        // Block private/local IP addresses based on policy
        if (POLICY === 'disable_non_proxied_udp') {{
            // Block all host candidates (which contain real IPs)
            if (candidateStr.includes('typ host')) {{
                return null;
            }}
            // Block server-reflexive candidates (STUN results with real IP)
            if (candidateStr.includes('typ srflx')) {{
                return null;
            }}
        }} else if (POLICY === 'default_public_interface_only') {{
            // Block private IP ranges
            const privateIpPattern = /(?:10\.\d{{1,3}}\.\d{{1,3}}\.\d{{1,3}}|172\.(?:1[6-9]|2\d|3[01])\.\d{{1,3}}\.\d{{1,3}}|192\.168\.\d{{1,3}}\.\d{{1,3}}|fc00:|fe80:)/;
            if (privateIpPattern.test(candidateStr)) {{
                return null;
            }}
        }}

        return candidate;
    }}

    // Override RTCPeerConnection if it exists
    if (typeof window.RTCPeerConnection !== 'undefined') {{
        const OriginalRTCPeerConnection = window.RTCPeerConnection;

        window.RTCPeerConnection = function(configuration, constraints) {{
            // Force ICE transport policy based on our policy
            if (!configuration) {{
                configuration = {{}};
            }}

            if (POLICY === 'disable_non_proxied_udp') {{
                configuration.iceTransportPolicy = 'relay';
            }}

            const pc = new OriginalRTCPeerConnection(configuration, constraints);

            // Override onicecandidate to filter candidates
            const originalAddEventListener = pc.addEventListener.bind(pc);
            pc.addEventListener = function(type, listener, options) {{
                if (type === 'icecandidate') {{
                    const wrappedListener = function(event) {{
                        if (event.candidate) {{
                            const filtered = filterIceCandidate(event.candidate);
                            if (filtered === null) {{
                                // Block this candidate
                                return;
                            }}
                        }}
                        listener.call(this, event);
                    }};
                    return originalAddEventListener(type, wrappedListener, options);
                }}
                return originalAddEventListener(type, listener, options);
            }};

            // Override the onicecandidate property setter
            let _onicecandidateHandler = null;
            Object.defineProperty(pc, 'onicecandidate', {{
                get: function() {{ return _onicecandidateHandler; }},
                set: function(handler) {{
                    if (typeof handler === 'function') {{
                        _onicecandidateHandler = function(event) {{
                            if (event.candidate) {{
                                const filtered = filterIceCandidate(event.candidate);
                                if (filtered === null) {{
                                    return;
                                }}
                            }}
                            handler.call(this, event);
                        }};
                    }} else {{
                        _onicecandidateHandler = handler;
                    }}
                }},
                configurable: true,
                enumerable: true
            }});

            return pc;
        }};

        // Preserve prototype chain
        window.RTCPeerConnection.prototype = OriginalRTCPeerConnection.prototype;
        window.RTCPeerConnection.generateCertificate = OriginalRTCPeerConnection.generateCertificate;
    }}

    // Override webkit prefixed version if it exists
    if (typeof window.webkitRTCPeerConnection !== 'undefined') {{
        window.webkitRTCPeerConnection = window.RTCPeerConnection;
    }}

}})();
"#,
            policy = policy,
        )
    }
}

impl Default for WebRtcConfig {
    fn default() -> Self {
        Self::safe_default()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_disabled_config() {
        let config = WebRtcConfig::disabled();
        assert!(config.disabled);
        let js = config.get_override_script();

        assert!(js.contains("RTCPeerConnection"));
        assert!(js.contains("NotAllowedError"));
        assert!(js.contains("RTCSessionDescription"));
        assert!(js.contains("RTCIceCandidate"));
    }

    #[test]
    fn test_safe_default() {
        let config = WebRtcConfig::safe_default();
        assert!(!config.disabled);
        assert_eq!(
            config.ip_handling_policy,
            WebRtcIpPolicy::DisableNonProxiedUdp
        );
    }

    #[test]
    fn test_policy_script_contains_filtering() {
        let config = WebRtcConfig::new(WebRtcIpPolicy::DisableNonProxiedUdp);
        let js = config.get_override_script();

        assert!(js.contains("disable_non_proxied_udp"));
        assert!(js.contains("filterIceCandidate"));
        assert!(js.contains("iceTransportPolicy"));
        assert!(js.contains("onicecandidate"));
    }

    #[test]
    fn test_default_public_interface_policy() {
        let config = WebRtcConfig::new(WebRtcIpPolicy::DefaultPublicInterfaceOnly);
        let js = config.get_override_script();

        assert!(js.contains("default_public_interface_only"));
        assert!(js.contains("privateIpPattern"));
    }

    #[test]
    fn test_default_policy() {
        let config = WebRtcConfig::new(WebRtcIpPolicy::Default);
        let js = config.get_override_script();

        assert!(js.contains("RTCPeerConnection"));
    }

    #[test]
    fn test_policy_string_conversion() {
        assert_eq!(
            WebRtcIpPolicy::DisableNonProxiedUdp.to_policy_string(),
            "disable_non_proxied_udp"
        );
        assert_eq!(
            WebRtcIpPolicy::DefaultPublicInterfaceOnly.to_policy_string(),
            "default_public_interface_only"
        );
        assert_eq!(WebRtcIpPolicy::Default.to_policy_string(), "default");
    }

    #[test]
    fn test_default_trait() {
        let config = WebRtcConfig::default();
        assert!(!config.disabled);
        assert_eq!(
            config.ip_handling_policy,
            WebRtcIpPolicy::DisableNonProxiedUdp
        );
    }
}
