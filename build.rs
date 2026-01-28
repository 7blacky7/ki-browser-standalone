//! Build script for ki-browser-standalone
//!
//! This script handles:
//! 1. Checking for CEF binaries
//! 2. Downloading CEF binaries if not present
//! 3. Setting up linking for CEF libraries
//! 4. Copying CEF resources to output directory

use std::env;
use std::fs;
use std::io::{self, BufReader, Write};
use std::path::{Path, PathBuf};
use std::process::Command;

/// CEF version to download (should match the cef crate version)
const CEF_VERSION: &str = "131.3.5+g97e26f6+chromium-131.0.6778.205";

/// CEF download base URL
const CEF_DOWNLOAD_BASE: &str = "https://cef-builds.spotifycdn.com";

fn main() {
    println!("cargo:rerun-if-changed=build.rs");
    println!("cargo:rerun-if-env-changed=CEF_PATH");

    let manifest_dir = env::var("CARGO_MANIFEST_DIR").expect("CARGO_MANIFEST_DIR not set");
    let out_dir = env::var("OUT_DIR").expect("OUT_DIR not set");
    let profile = env::var("PROFILE").unwrap_or_else(|_| "debug".to_string());
    let target = env::var("TARGET").expect("TARGET not set");

    println!("cargo:warning=Building ki-browser-standalone for {} ({})", target, profile);

    // Determine platform
    let (os, arch) = parse_target(&target);
    println!("cargo:warning=Detected OS: {}, Arch: {}", os, arch);

    // Check for CEF path override
    let cef_path = env::var("CEF_PATH")
        .map(PathBuf::from)
        .unwrap_or_else(|_| PathBuf::from(&manifest_dir).join("cef"));

    println!("cargo:warning=CEF path: {}", cef_path.display());

    // Check if CEF binaries are present
    if !check_cef_binaries(&cef_path, os) {
        println!("cargo:warning=CEF binaries not found. Attempting to download...");

        // Try to download CEF
        if let Err(e) = download_cef(&cef_path, os, arch) {
            println!("cargo:warning=Failed to download CEF: {}", e);
            println!("cargo:warning=Please download CEF manually using:");
            if os == "windows" {
                println!("cargo:warning=  PowerShell: .\\scripts\\download_cef.ps1");
            } else {
                println!("cargo:warning=  Bash: ./scripts/download_cef.sh");
            }
            println!("cargo:warning=Or set CEF_PATH environment variable to your CEF installation");

            // Don't fail the build - let it continue and fail at link time with a clearer error
            return;
        }
    }

    // Set up linking
    setup_linking(&cef_path, os, &profile);

    // Copy resources to output directory
    let target_dir = PathBuf::from(&out_dir)
        .ancestors()
        .nth(3)
        .expect("Could not find target directory")
        .join(&profile);

    if let Err(e) = copy_cef_resources(&cef_path, &target_dir, os) {
        println!("cargo:warning=Failed to copy CEF resources: {}", e);
    }

    println!("cargo:warning=CEF setup complete!");
}

/// Parse the target triple to extract OS and architecture
fn parse_target(target: &str) -> (&'static str, &'static str) {
    let os = if target.contains("windows") {
        "windows"
    } else if target.contains("linux") {
        "linux"
    } else if target.contains("darwin") || target.contains("macos") {
        "macos"
    } else {
        panic!("Unsupported target OS: {}", target);
    };

    let arch = if target.contains("x86_64") || target.contains("x64") {
        "x64"
    } else if target.contains("aarch64") || target.contains("arm64") {
        "arm64"
    } else if target.contains("i686") || target.contains("i386") {
        "x86"
    } else {
        panic!("Unsupported target architecture: {}", target);
    };

    (os, arch)
}

/// Check if CEF binaries are present
fn check_cef_binaries(cef_path: &Path, os: &str) -> bool {
    let release_dir = cef_path.join("Release");

    if !release_dir.exists() {
        return false;
    }

    // Check for main library
    let lib_exists = match os {
        "windows" => release_dir.join("libcef.dll").exists(),
        "linux" => release_dir.join("libcef.so").exists(),
        "macos" => release_dir
            .join("Chromium Embedded Framework.framework")
            .exists(),
        _ => false,
    };

    if !lib_exists {
        return false;
    }

    // Check for essential resources
    let resources_dir = cef_path.join("Resources");
    if !resources_dir.exists() {
        return false;
    }

    // Check for essential files
    let essential_files = ["icudtl.dat"];
    for file in essential_files {
        // icudtl.dat can be in Release or Resources
        if !release_dir.join(file).exists() && !resources_dir.join(file).exists() {
            println!("cargo:warning=Missing essential CEF file: {}", file);
            return false;
        }
    }

    true
}

/// Download CEF binaries
fn download_cef(cef_path: &Path, os: &str, arch: &str) -> Result<(), Box<dyn std::error::Error>> {
    // Construct download URL
    let platform = match (os, arch) {
        ("windows", "x64") => "windows64",
        ("windows", "x86") => "windows32",
        ("windows", "arm64") => "windowsarm64",
        ("linux", "x64") => "linux64",
        ("linux", "arm64") => "linuxarm64",
        ("macos", "x64") => "macosx64",
        ("macos", "arm64") => "macosarm64",
        _ => return Err(format!("Unsupported platform: {}-{}", os, arch).into()),
    };

    let cef_filename = format!(
        "cef_binary_{}_{}",
        CEF_VERSION.replace('+', "%2B"),
        platform
    );
    let archive_name = format!("{}_minimal.tar.bz2", cef_filename);
    let download_url = format!("{}/{}", CEF_DOWNLOAD_BASE, archive_name);

    println!("cargo:warning=Downloading CEF from: {}", download_url);

    // Create temp directory for download
    let temp_dir = env::temp_dir().join("cef_download");
    fs::create_dir_all(&temp_dir)?;

    let archive_path = temp_dir.join(&archive_name);

    // Download using curl or PowerShell
    if os == "windows" {
        // Use PowerShell on Windows
        let status = Command::new("powershell")
            .args([
                "-NoProfile",
                "-Command",
                &format!(
                    "Invoke-WebRequest -Uri '{}' -OutFile '{}' -UseBasicParsing",
                    download_url,
                    archive_path.display()
                ),
            ])
            .status()?;

        if !status.success() {
            return Err("Failed to download CEF with PowerShell".into());
        }
    } else {
        // Use curl on Unix-like systems
        let status = Command::new("curl")
            .args(["-L", "-o", &archive_path.to_string_lossy(), &download_url])
            .status()?;

        if !status.success() {
            return Err("Failed to download CEF with curl".into());
        }
    }

    println!("cargo:warning=Download complete. Extracting...");

    // Create CEF directory
    fs::create_dir_all(cef_path)?;

    // Extract archive
    if os == "windows" {
        // Use tar on Windows (available in Windows 10+)
        let status = Command::new("tar")
            .args([
                "-xjf",
                &archive_path.to_string_lossy(),
                "-C",
                &temp_dir.to_string_lossy(),
            ])
            .status()?;

        if !status.success() {
            return Err("Failed to extract CEF archive".into());
        }
    } else {
        // Use tar on Unix
        let status = Command::new("tar")
            .args([
                "-xjf",
                &archive_path.to_string_lossy(),
                "-C",
                &temp_dir.to_string_lossy(),
            ])
            .status()?;

        if !status.success() {
            return Err("Failed to extract CEF archive".into());
        }
    }

    // Find extracted directory
    let extracted_dir_name = format!(
        "cef_binary_{}_{}",
        CEF_VERSION.replace('+', "%2B").replace("%2B", "+"),
        platform
    );

    // Try different possible directory names
    let mut extracted_dir = temp_dir.join(&extracted_dir_name);
    if !extracted_dir.exists() {
        // Try without URL encoding
        let alt_name = format!("cef_binary_{}_{}", CEF_VERSION, platform);
        extracted_dir = temp_dir.join(&alt_name);
    }
    if !extracted_dir.exists() {
        // Try finding any cef_binary directory
        for entry in fs::read_dir(&temp_dir)? {
            let entry = entry?;
            let name = entry.file_name().to_string_lossy().to_string();
            if name.starts_with("cef_binary_") {
                extracted_dir = entry.path();
                break;
            }
        }
    }

    if !extracted_dir.exists() {
        return Err(format!(
            "Could not find extracted CEF directory in {}",
            temp_dir.display()
        )
        .into());
    }

    println!(
        "cargo:warning=Moving CEF files from {} to {}",
        extracted_dir.display(),
        cef_path.display()
    );

    // Copy contents to cef_path
    copy_dir_contents(&extracted_dir, cef_path)?;

    // Clean up
    let _ = fs::remove_dir_all(&temp_dir);

    println!("cargo:warning=CEF extraction complete!");

    Ok(())
}

/// Copy directory contents recursively
fn copy_dir_contents(src: &Path, dst: &Path) -> io::Result<()> {
    if !dst.exists() {
        fs::create_dir_all(dst)?;
    }

    for entry in fs::read_dir(src)? {
        let entry = entry?;
        let src_path = entry.path();
        let dst_path = dst.join(entry.file_name());

        if src_path.is_dir() {
            copy_dir_contents(&src_path, &dst_path)?;
        } else {
            fs::copy(&src_path, &dst_path)?;
        }
    }

    Ok(())
}

/// Set up linking for CEF
fn setup_linking(cef_path: &Path, os: &str, profile: &str) {
    let release_dir = cef_path.join("Release");
    let debug_dir = cef_path.join("Debug");

    // Determine library directory based on build profile
    let lib_dir = if profile == "debug" && debug_dir.exists() {
        debug_dir
    } else {
        release_dir.clone()
    };

    // Add library search path
    println!("cargo:rustc-link-search=native={}", lib_dir.display());

    match os {
        "windows" => {
            // On Windows, link against libcef.lib (import library)
            // The actual libcef.dll must be in the executable's directory at runtime
            println!("cargo:rustc-link-search=native={}", release_dir.display());

            // Link against CEF wrapper library if built
            let wrapper_lib = cef_path.join("build").join("libcef_dll_wrapper");
            if wrapper_lib.exists() {
                println!("cargo:rustc-link-search=native={}", wrapper_lib.display());
            }

            // Note: The cef crate handles the actual linking
            // We just need to ensure the paths are set up
        }
        "linux" => {
            // On Linux, link against libcef.so
            println!("cargo:rustc-link-search=native={}", lib_dir.display());

            // Set rpath for runtime library loading
            println!("cargo:rustc-link-arg=-Wl,-rpath,$ORIGIN");
            println!("cargo:rustc-link-arg=-Wl,-rpath,$ORIGIN/../lib");
        }
        "macos" => {
            // On macOS, link against the framework
            let framework_path = lib_dir.join("Chromium Embedded Framework.framework");
            if framework_path.exists() {
                println!(
                    "cargo:rustc-link-search=framework={}",
                    lib_dir.display()
                );
            }
        }
        _ => {}
    }

    // Export CEF path for runtime
    println!("cargo:rustc-env=CEF_PATH={}", cef_path.display());
}

/// Copy CEF resources to the output directory
fn copy_cef_resources(
    cef_path: &Path,
    target_dir: &Path,
    os: &str,
) -> Result<(), Box<dyn std::error::Error>> {
    println!(
        "cargo:warning=Copying CEF resources to {}",
        target_dir.display()
    );

    let release_dir = cef_path.join("Release");
    let resources_dir = cef_path.join("Resources");

    // Create target directory if needed
    fs::create_dir_all(target_dir)?;

    // Files to copy from Release directory
    let release_files: Vec<&str> = match os {
        "windows" => vec![
            "libcef.dll",
            "chrome_elf.dll",
            "d3dcompiler_47.dll",
            "libEGL.dll",
            "libGLESv2.dll",
            "vk_swiftshader.dll",
            "vk_swiftshader_icd.json",
            "vulkan-1.dll",
            "icudtl.dat",
            "v8_context_snapshot.bin",
            "snapshot_blob.bin",
        ],
        "linux" => vec![
            "libcef.so",
            "libEGL.so",
            "libGLESv2.so",
            "libvk_swiftshader.so",
            "libvulkan.so.1",
            "vk_swiftshader_icd.json",
            "icudtl.dat",
            "v8_context_snapshot.bin",
            "snapshot_blob.bin",
            "chrome-sandbox",
        ],
        "macos" => vec![
            // macOS uses a framework bundle
        ],
        _ => vec![],
    };

    // Copy Release files
    for file in release_files {
        let src = release_dir.join(file);
        let dst = target_dir.join(file);
        if src.exists() {
            if let Err(e) = fs::copy(&src, &dst) {
                println!(
                    "cargo:warning=Failed to copy {}: {}",
                    file, e
                );
            }
        }
    }

    // Resource files to copy
    let resource_files = [
        "cef.pak",
        "cef_100_percent.pak",
        "cef_200_percent.pak",
        "cef_extensions.pak",
        "devtools_resources.pak",
    ];

    // Copy resource files
    for file in resource_files {
        let src = resources_dir.join(file);
        let dst = target_dir.join(file);
        if src.exists() {
            if let Err(e) = fs::copy(&src, &dst) {
                println!(
                    "cargo:warning=Failed to copy {}: {}",
                    file, e
                );
            }
        }
    }

    // Copy icudtl.dat from Resources if not in Release
    let icudtl_src = resources_dir.join("icudtl.dat");
    let icudtl_dst = target_dir.join("icudtl.dat");
    if icudtl_src.exists() && !icudtl_dst.exists() {
        fs::copy(&icudtl_src, &icudtl_dst)?;
    }

    // Copy locales directory
    let locales_src = resources_dir.join("locales");
    let locales_dst = target_dir.join("locales");
    if locales_src.exists() {
        copy_dir_contents(&locales_src, &locales_dst)?;
    }

    // On macOS, copy the framework
    if os == "macos" {
        let framework_src = release_dir.join("Chromium Embedded Framework.framework");
        let framework_dst = target_dir.join("Chromium Embedded Framework.framework");
        if framework_src.exists() {
            copy_dir_contents(&framework_src, &framework_dst)?;
        }
    }

    Ok(())
}
