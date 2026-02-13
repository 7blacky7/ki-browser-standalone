#!/usr/bin/env bash
#
# Download and extract CEF (Chromium Embedded Framework) binaries for ki-browser-standalone
#
# Usage:
#   ./download_cef.sh [options]
#
# Options:
#   -v, --version VERSION    CEF version to download (default: see CEF_VERSION below)
#   -a, --arch ARCH          Target architecture: x64, arm64 (default: auto-detect)
#   -o, --output PATH        Output directory (default: ./cef)
#   -m, --minimal            Download minimal distribution
#   -f, --force              Force re-download even if CEF exists
#   -h, --help               Show this help message
#

set -euo pipefail

# Configuration
CEF_VERSION="131.3.5+g97e26f6+chromium-131.0.6778.205"
CEF_DOWNLOAD_BASE="https://cef-builds.spotifycdn.com"

# Colors for output
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
CYAN='\033[0;36m'
NC='\033[0m' # No Color

# Default values
ARCHITECTURE="auto"
OUTPUT_PATH=""
MINIMAL=false
FORCE=false

# Get script directory and project root
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PROJECT_ROOT="$(dirname "$SCRIPT_DIR")"

# Logging functions
log_info() {
    echo -e "${CYAN}[INFO]${NC} $1"
}

log_success() {
    echo -e "${GREEN}[OK]${NC} $1"
}

log_warning() {
    echo -e "${YELLOW}[WARN]${NC} $1"
}

log_error() {
    echo -e "${RED}[ERROR]${NC} $1"
}

# Show help
show_help() {
    cat << EOF
CEF Download Script for Linux/macOS

Usage: $(basename "$0") [options]

Options:
    -v, --version VERSION    CEF version to download
                             Default: $CEF_VERSION
    -a, --arch ARCH          Target architecture: x64, arm64
                             Default: auto-detect
    -o, --output PATH        Output directory for CEF files
                             Default: ./cef (relative to project root)
    -m, --minimal            Download minimal distribution (smaller)
    -f, --force              Force re-download even if CEF exists
    -h, --help               Show this help message

Examples:
    $(basename "$0")                    # Download with defaults
    $(basename "$0") -f                 # Force re-download
    $(basename "$0") -a x64 -m          # Download minimal x64 version

EOF
}

# Detect operating system
detect_os() {
    case "$(uname -s)" in
        Linux*)     echo "linux";;
        Darwin*)    echo "macos";;
        CYGWIN*|MINGW*|MSYS*) echo "windows";;
        *)          echo "unknown";;
    esac
}

# Detect architecture
detect_arch() {
    local arch
    arch="$(uname -m)"

    case "$arch" in
        x86_64|amd64)   echo "x64";;
        aarch64|arm64)  echo "arm64";;
        i386|i686)      echo "x86";;
        *)
            log_warning "Unknown architecture: $arch, defaulting to x64"
            echo "x64"
            ;;
    esac
}

# Get CEF platform string
get_platform_string() {
    local os="$1"
    local arch="$2"

    case "$os-$arch" in
        linux-x64)      echo "linux64";;
        linux-arm64)    echo "linuxarm64";;
        macos-x64)      echo "macosx64";;
        macos-arm64)    echo "macosarm64";;
        windows-x64)    echo "windows64";;
        windows-x86)    echo "windows32";;
        windows-arm64)  echo "windowsarm64";;
        *)
            log_error "Unsupported platform: $os-$arch"
            exit 1
            ;;
    esac
}

# Check if CEF is already present
check_cef_present() {
    local cef_path="$1"
    local os="$2"

    local release_dir="$cef_path/Release"
    local resources_dir="$cef_path/Resources"

    if [[ ! -d "$release_dir" ]]; then
        return 1
    fi

    # Check for main library
    case "$os" in
        linux)
            [[ -f "$release_dir/libcef.so" ]] || return 1
            ;;
        macos)
            [[ -d "$release_dir/Chromium Embedded Framework.framework" ]] || return 1
            ;;
        windows)
            [[ -f "$release_dir/libcef.dll" ]] || return 1
            ;;
    esac

    if [[ ! -d "$resources_dir" ]]; then
        return 1
    fi

    # Check for icudtl.dat
    if [[ ! -f "$release_dir/icudtl.dat" ]] && [[ ! -f "$resources_dir/icudtl.dat" ]]; then
        return 1
    fi

    return 0
}

# Download file with progress
download_file() {
    local url="$1"
    local output="$2"

    log_info "Downloading: $url"
    log_info "Output: $output"

    # Try curl first, then wget
    if command -v curl &> /dev/null; then
        curl -L --progress-bar -o "$output" "$url"
    elif command -v wget &> /dev/null; then
        wget --progress=bar:force -O "$output" "$url"
    else
        log_error "Neither curl nor wget found. Please install one of them."
        exit 1
    fi

    # Verify download
    if [[ ! -f "$output" ]] || [[ ! -s "$output" ]]; then
        log_error "Download failed or file is empty"
        return 1
    fi

    return 0
}

# Extract tar.bz2 archive
extract_archive() {
    local archive="$1"
    local dest="$2"

    log_info "Extracting archive..."

    mkdir -p "$dest"

    # Check for tar with bzip2 support
    if command -v tar &> /dev/null; then
        tar -xjf "$archive" -C "$dest"
    else
        log_error "tar command not found"
        return 1
    fi

    return 0
}

# Move CEF contents to final location
move_cef_contents() {
    local source_dir="$1"
    local dest_dir="$2"

    log_info "Moving CEF files to $dest_dir"

    # Find extracted directory
    local cef_dir
    cef_dir=$(find "$source_dir" -maxdepth 1 -type d -name "cef_binary_*" | head -n 1)

    if [[ -z "$cef_dir" ]] || [[ ! -d "$cef_dir" ]]; then
        log_error "Could not find extracted CEF directory in $source_dir"
        return 1
    fi

    log_info "Found CEF directory: $(basename "$cef_dir")"

    # Remove existing destination
    if [[ -d "$dest_dir" ]]; then
        log_warning "Removing existing CEF directory..."
        rm -rf "$dest_dir"
    fi

    # Move to final location
    mv "$cef_dir" "$dest_dir"

    return 0
}

# Set executable permissions on Linux
set_permissions() {
    local cef_path="$1"
    local os="$2"

    if [[ "$os" == "linux" ]]; then
        # Set execute permission on chrome-sandbox if present
        local sandbox="$cef_path/Release/chrome-sandbox"
        if [[ -f "$sandbox" ]]; then
            log_info "Setting permissions on chrome-sandbox..."
            chmod 4755 "$sandbox" 2>/dev/null || {
                log_warning "Could not set SUID on chrome-sandbox (requires sudo)"
                log_warning "Run: sudo chown root:root $sandbox && sudo chmod 4755 $sandbox"
            }
        fi
    fi
}

# Parse command line arguments
parse_args() {
    while [[ $# -gt 0 ]]; do
        case "$1" in
            -v|--version)
                CEF_VERSION="$2"
                shift 2
                ;;
            -a|--arch)
                ARCHITECTURE="$2"
                shift 2
                ;;
            -o|--output)
                OUTPUT_PATH="$2"
                shift 2
                ;;
            -m|--minimal)
                MINIMAL=true
                shift
                ;;
            -f|--force)
                FORCE=true
                shift
                ;;
            -h|--help)
                show_help
                exit 0
                ;;
            *)
                log_error "Unknown option: $1"
                show_help
                exit 1
                ;;
        esac
    done
}

# Main function
main() {
    echo ""
    echo "================================="
    echo "  CEF Download Script for Unix"
    echo "================================="
    echo ""

    # Parse arguments
    parse_args "$@"

    # Detect OS
    local os
    os=$(detect_os)
    log_info "Detected OS: $os"

    if [[ "$os" == "unknown" ]]; then
        log_error "Unsupported operating system"
        exit 1
    fi

    # Set default output path
    if [[ -z "$OUTPUT_PATH" ]]; then
        OUTPUT_PATH="$PROJECT_ROOT/cef"
    fi

    log_info "Project root: $PROJECT_ROOT"
    log_info "CEF output path: $OUTPUT_PATH"

    # Check if CEF already exists
    if check_cef_present "$OUTPUT_PATH" "$os" && [[ "$FORCE" == "false" ]]; then
        log_success "CEF binaries already present at $OUTPUT_PATH"
        log_info "Use -f or --force to re-download"
        return
    fi

    # Determine architecture
    local arch
    if [[ "$ARCHITECTURE" == "auto" ]]; then
        arch=$(detect_arch)
    else
        arch="$ARCHITECTURE"
    fi
    log_info "Target architecture: $arch"

    # Get platform string
    local platform
    platform=$(get_platform_string "$os" "$arch")
    log_info "CEF platform: $platform"

    # Construct download URL
    local encoded_version="${CEF_VERSION//+/%2B}"
    local suffix=""
    if [[ "$MINIMAL" == "true" ]]; then
        suffix="_minimal"
    fi
    local filename="cef_binary_${encoded_version}_${platform}${suffix}.tar.bz2"
    local download_url="${CEF_DOWNLOAD_BASE}/${filename}"

    log_info "CEF version: $CEF_VERSION"

    # Create temp directory
    local temp_dir
    temp_dir=$(mktemp -d)
    log_info "Temp directory: $temp_dir"

    # Cleanup function
    cleanup() {
        log_info "Cleaning up temp files..."
        rm -rf "$temp_dir"
    }
    trap cleanup EXIT

    # Download
    local archive_path="$temp_dir/$filename"
    if ! download_file "$download_url" "$archive_path"; then
        log_error "Failed to download CEF"
        exit 1
    fi

    log_success "Download complete!"

    # Show file size
    local file_size
    file_size=$(du -h "$archive_path" | cut -f1)
    log_info "Archive size: $file_size"

    # Extract
    if ! extract_archive "$archive_path" "$temp_dir"; then
        log_error "Failed to extract CEF archive"
        exit 1
    fi

    log_success "Extraction complete!"

    # Move to final location
    if ! move_cef_contents "$temp_dir" "$OUTPUT_PATH"; then
        log_error "Failed to move CEF files"
        exit 1
    fi

    # Set permissions
    set_permissions "$OUTPUT_PATH" "$os"

    log_success "CEF installation complete!"

    # Verify installation
    if check_cef_present "$OUTPUT_PATH" "$os"; then
        echo ""
        log_success "CEF binaries successfully installed to:"
        echo "  $OUTPUT_PATH"
        echo ""
        log_info "You can now build ki-browser-standalone with 'cargo build'"
    else
        log_warning "CEF installation verification failed"
    fi
}

# Run main function
main "$@"
