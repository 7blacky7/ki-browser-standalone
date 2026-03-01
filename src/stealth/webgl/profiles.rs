//! Predefined WebGL/GPU profile definitions for fingerprint spoofing.
//!
//! Contains the [`WebGLProfile`] enum with GPU profiles for NVIDIA, AMD, Intel,
//! Apple Silicon, and software renderers. Each profile provides vendor, renderer,
//! architecture, and short vendor strings matching real-world GPU configurations.

/// Predefined WebGL/GPU profiles
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum WebGLProfile {
    // NVIDIA profiles
    NvidiaGtx1080,
    NvidiaGtx1660,
    NvidiaRtx3060,
    NvidiaRtx3080,
    NvidiaRtx4070,
    NvidiaRtx4090,

    // AMD profiles
    AmdRx580,
    AmdRx6700Xt,
    AmdRx7900Xt,

    // Intel integrated graphics
    IntelUhd620,
    IntelUhd630,
    IntelUhd770,
    IntelIrisXe,
    IntelArcA770,

    // Apple Silicon
    AppleM1,
    AppleM2,
    AppleM3,

    // Generic/Software
    SwiftShader,
    AngleDirect3D11,
}

impl WebGLProfile {
    /// Get all available profiles
    pub fn all() -> Vec<WebGLProfile> {
        vec![
            WebGLProfile::NvidiaGtx1080,
            WebGLProfile::NvidiaGtx1660,
            WebGLProfile::NvidiaRtx3060,
            WebGLProfile::NvidiaRtx3080,
            WebGLProfile::NvidiaRtx4070,
            WebGLProfile::NvidiaRtx4090,
            WebGLProfile::AmdRx580,
            WebGLProfile::AmdRx6700Xt,
            WebGLProfile::AmdRx7900Xt,
            WebGLProfile::IntelUhd620,
            WebGLProfile::IntelUhd630,
            WebGLProfile::IntelUhd770,
            WebGLProfile::IntelIrisXe,
            WebGLProfile::IntelArcA770,
            WebGLProfile::AppleM1,
            WebGLProfile::AppleM2,
            WebGLProfile::AppleM3,
            WebGLProfile::SwiftShader,
            WebGLProfile::AngleDirect3D11,
        ]
    }

    /// Get common desktop profiles (most likely to be seen)
    pub fn common_desktop() -> Vec<WebGLProfile> {
        vec![
            WebGLProfile::NvidiaGtx1660,
            WebGLProfile::NvidiaRtx3060,
            WebGLProfile::NvidiaRtx3080,
            WebGLProfile::AmdRx6700Xt,
            WebGLProfile::IntelUhd630,
            WebGLProfile::IntelIrisXe,
        ]
    }

    /// Get the vendor string for this profile
    pub fn vendor(&self) -> &'static str {
        match self {
            WebGLProfile::NvidiaGtx1080
            | WebGLProfile::NvidiaGtx1660
            | WebGLProfile::NvidiaRtx3060
            | WebGLProfile::NvidiaRtx3080
            | WebGLProfile::NvidiaRtx4070
            | WebGLProfile::NvidiaRtx4090 => "NVIDIA Corporation",

            WebGLProfile::AmdRx580 | WebGLProfile::AmdRx6700Xt | WebGLProfile::AmdRx7900Xt => {
                "AMD"
            }

            WebGLProfile::IntelUhd620
            | WebGLProfile::IntelUhd630
            | WebGLProfile::IntelUhd770
            | WebGLProfile::IntelIrisXe
            | WebGLProfile::IntelArcA770 => "Intel Inc.",

            WebGLProfile::AppleM1 | WebGLProfile::AppleM2 | WebGLProfile::AppleM3 => "Apple Inc.",

            WebGLProfile::SwiftShader => "Google Inc. (Google)",
            WebGLProfile::AngleDirect3D11 => "Google Inc. (NVIDIA)",
        }
    }

    /// Get the short vendor name for WebGPU adapter info
    pub fn vendor_short(&self) -> &'static str {
        match self {
            WebGLProfile::NvidiaGtx1080
            | WebGLProfile::NvidiaGtx1660
            | WebGLProfile::NvidiaRtx3060
            | WebGLProfile::NvidiaRtx3080
            | WebGLProfile::NvidiaRtx4070
            | WebGLProfile::NvidiaRtx4090 => "nvidia",

            WebGLProfile::AmdRx580 | WebGLProfile::AmdRx6700Xt | WebGLProfile::AmdRx7900Xt => {
                "amd"
            }

            WebGLProfile::IntelUhd620
            | WebGLProfile::IntelUhd630
            | WebGLProfile::IntelUhd770
            | WebGLProfile::IntelIrisXe
            | WebGLProfile::IntelArcA770 => "intel",

            WebGLProfile::AppleM1 | WebGLProfile::AppleM2 | WebGLProfile::AppleM3 => "apple",

            WebGLProfile::SwiftShader => "google",
            WebGLProfile::AngleDirect3D11 => "nvidia",
        }
    }

    /// Get the GPU architecture string for WebGPU adapter info
    pub fn architecture(&self) -> &'static str {
        match self {
            // NVIDIA architectures
            WebGLProfile::NvidiaGtx1080 | WebGLProfile::NvidiaGtx1660 => "turing",
            WebGLProfile::NvidiaRtx3060 | WebGLProfile::NvidiaRtx3080 => "ampere",
            WebGLProfile::NvidiaRtx4070 | WebGLProfile::NvidiaRtx4090 => "ada-lovelace",

            // AMD architectures
            WebGLProfile::AmdRx580 => "polaris",
            WebGLProfile::AmdRx6700Xt => "rdna-2",
            WebGLProfile::AmdRx7900Xt => "rdna-3",

            // Intel architectures
            WebGLProfile::IntelUhd620 | WebGLProfile::IntelUhd630 => "gen-9.5",
            WebGLProfile::IntelUhd770 => "gen-12",
            WebGLProfile::IntelIrisXe => "gen-12",
            WebGLProfile::IntelArcA770 => "alchemist",

            // Apple architectures
            WebGLProfile::AppleM1 => "apple-7",
            WebGLProfile::AppleM2 => "apple-8",
            WebGLProfile::AppleM3 => "apple-9",

            // Software/Generic
            WebGLProfile::SwiftShader => "swiftshader",
            WebGLProfile::AngleDirect3D11 => "turing",
        }
    }

    /// Get the renderer string for this profile
    pub fn renderer(&self) -> &'static str {
        match self {
            WebGLProfile::NvidiaGtx1080 => {
                "ANGLE (NVIDIA, NVIDIA GeForce GTX 1080 Direct3D11 vs_5_0 ps_5_0, D3D11)"
            }
            WebGLProfile::NvidiaGtx1660 => {
                "ANGLE (NVIDIA, NVIDIA GeForce GTX 1660 SUPER Direct3D11 vs_5_0 ps_5_0, D3D11)"
            }
            WebGLProfile::NvidiaRtx3060 => {
                "ANGLE (NVIDIA, NVIDIA GeForce RTX 3060 Direct3D11 vs_5_0 ps_5_0, D3D11)"
            }
            WebGLProfile::NvidiaRtx3080 => {
                "ANGLE (NVIDIA, NVIDIA GeForce RTX 3080 Direct3D11 vs_5_0 ps_5_0, D3D11)"
            }
            WebGLProfile::NvidiaRtx4070 => {
                "ANGLE (NVIDIA, NVIDIA GeForce RTX 4070 Direct3D11 vs_5_0 ps_5_0, D3D11)"
            }
            WebGLProfile::NvidiaRtx4090 => {
                "ANGLE (NVIDIA, NVIDIA GeForce RTX 4090 Direct3D11 vs_5_0 ps_5_0, D3D11)"
            }

            WebGLProfile::AmdRx580 => {
                "ANGLE (AMD, AMD Radeon RX 580 Series Direct3D11 vs_5_0 ps_5_0, D3D11)"
            }
            WebGLProfile::AmdRx6700Xt => {
                "ANGLE (AMD, AMD Radeon RX 6700 XT Direct3D11 vs_5_0 ps_5_0, D3D11)"
            }
            WebGLProfile::AmdRx7900Xt => {
                "ANGLE (AMD, AMD Radeon RX 7900 XT Direct3D11 vs_5_0 ps_5_0, D3D11)"
            }

            WebGLProfile::IntelUhd620 => {
                "ANGLE (Intel, Intel(R) UHD Graphics 620 Direct3D11 vs_5_0 ps_5_0, D3D11)"
            }
            WebGLProfile::IntelUhd630 => {
                "ANGLE (Intel, Intel(R) UHD Graphics 630 Direct3D11 vs_5_0 ps_5_0, D3D11)"
            }
            WebGLProfile::IntelUhd770 => {
                "ANGLE (Intel, Intel(R) UHD Graphics 770 Direct3D11 vs_5_0 ps_5_0, D3D11)"
            }
            WebGLProfile::IntelIrisXe => {
                "ANGLE (Intel, Intel(R) Iris(R) Xe Graphics Direct3D11 vs_5_0 ps_5_0, D3D11)"
            }
            WebGLProfile::IntelArcA770 => {
                "ANGLE (Intel, Intel(R) Arc(TM) A770 Graphics Direct3D11 vs_5_0 ps_5_0, D3D11)"
            }

            WebGLProfile::AppleM1 => "Apple M1",
            WebGLProfile::AppleM2 => "Apple M2",
            WebGLProfile::AppleM3 => "Apple M3",

            WebGLProfile::SwiftShader => {
                "ANGLE (Google, Vulkan 1.1.0 (SwiftShader Device (Subzero) (0x0000C0DE)), SwiftShader driver)"
            }
            WebGLProfile::AngleDirect3D11 => {
                "ANGLE (NVIDIA, NVIDIA GeForce GTX 1060 6GB Direct3D11 vs_5_0 ps_5_0, D3D11)"
            }
        }
    }
}
