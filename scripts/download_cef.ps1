#Requires -Version 5.1
<#
.SYNOPSIS
    Downloads and extracts CEF (Chromium Embedded Framework) binaries for ki-browser-standalone.

.DESCRIPTION
    This script downloads the CEF binaries required for building and running ki-browser-standalone.
    It automatically detects the system architecture and downloads the appropriate version.

.PARAMETER Version
    The CEF version to download. Defaults to the version specified in build.rs.

.PARAMETER Architecture
    The target architecture (x64, x86, arm64). Defaults to auto-detection.

.PARAMETER OutputPath
    The output directory for CEF files. Defaults to ./cef in the project root.

.PARAMETER Minimal
    Download the minimal distribution (smaller, but lacks some features).

.PARAMETER Force
    Force re-download even if CEF is already present.

.EXAMPLE
    .\download_cef.ps1
    Downloads CEF with default settings.

.EXAMPLE
    .\download_cef.ps1 -Force -Architecture x64
    Force download CEF for x64 architecture.
#>

[CmdletBinding()]
param(
    [Parameter()]
    [string]$Version = "131.3.5+g97e26f6+chromium-131.0.6778.205",

    [Parameter()]
    [ValidateSet("x64", "x86", "arm64", "auto")]
    [string]$Architecture = "auto",

    [Parameter()]
    [string]$OutputPath = "",

    [Parameter()]
    [switch]$Minimal,

    [Parameter()]
    [switch]$Force
)

# Error handling
$ErrorActionPreference = "Stop"

# Configuration
$CEF_DOWNLOAD_BASE = "https://cef-builds.spotifycdn.com"

function Write-Status {
    param([string]$Message, [string]$Type = "Info")

    $color = switch ($Type) {
        "Info"    { "Cyan" }
        "Success" { "Green" }
        "Warning" { "Yellow" }
        "Error"   { "Red" }
        default   { "White" }
    }

    $prefix = switch ($Type) {
        "Info"    { "[INFO]" }
        "Success" { "[OK]" }
        "Warning" { "[WARN]" }
        "Error"   { "[ERROR]" }
        default   { "[*]" }
    }

    Write-Host "$prefix $Message" -ForegroundColor $color
}

function Get-ProjectRoot {
    $scriptDir = Split-Path -Parent $MyInvocation.ScriptName
    return Split-Path -Parent $scriptDir
}

function Get-SystemArchitecture {
    $arch = [System.Environment]::GetEnvironmentVariable("PROCESSOR_ARCHITECTURE")
    switch ($arch) {
        "AMD64" { return "x64" }
        "ARM64" { return "arm64" }
        "x86"   { return "x86" }
        default {
            Write-Status "Unknown architecture: $arch, defaulting to x64" -Type Warning
            return "x64"
        }
    }
}

function Get-CefPlatformString {
    param([string]$Arch)

    switch ($Arch) {
        "x64"   { return "windows64" }
        "x86"   { return "windows32" }
        "arm64" { return "windowsarm64" }
        default { throw "Unsupported architecture: $Arch" }
    }
}

function Test-CefPresent {
    param([string]$CefPath)

    $releaseDir = Join-Path $CefPath "Release"
    $resourcesDir = Join-Path $CefPath "Resources"

    if (-not (Test-Path $releaseDir)) {
        return $false
    }

    $libcefPath = Join-Path $releaseDir "libcef.dll"
    if (-not (Test-Path $libcefPath)) {
        return $false
    }

    if (-not (Test-Path $resourcesDir)) {
        return $false
    }

    # Check for essential files
    $icudtlRelease = Join-Path $releaseDir "icudtl.dat"
    $icudtlResources = Join-Path $resourcesDir "icudtl.dat"
    if (-not (Test-Path $icudtlRelease) -and -not (Test-Path $icudtlResources)) {
        return $false
    }

    return $true
}

function Get-DownloadUrl {
    param(
        [string]$Ver,
        [string]$Platform,
        [bool]$IsMinimal
    )

    $encodedVersion = $Ver -replace '\+', '%2B'
    $suffix = if ($IsMinimal) { "_minimal" } else { "" }
    $filename = "cef_binary_${encodedVersion}_${Platform}${suffix}.tar.bz2"

    return @{
        Url = "$CEF_DOWNLOAD_BASE/$filename"
        Filename = $filename
    }
}

function Invoke-Download {
    param(
        [string]$Url,
        [string]$OutputFile
    )

    Write-Status "Downloading from: $Url"
    Write-Status "Saving to: $OutputFile"

    try {
        # Try using Invoke-WebRequest with progress
        $progressPreference = 'SilentlyContinue'  # Speeds up download significantly
        Invoke-WebRequest -Uri $Url -OutFile $OutputFile -UseBasicParsing
        $progressPreference = 'Continue'
        return $true
    }
    catch {
        Write-Status "Invoke-WebRequest failed, trying WebClient..." -Type Warning
        try {
            $webClient = New-Object System.Net.WebClient
            $webClient.DownloadFile($Url, $OutputFile)
            return $true
        }
        catch {
            Write-Status "Download failed: $_" -Type Error
            return $false
        }
    }
}

function Expand-TarBz2 {
    param(
        [string]$ArchivePath,
        [string]$DestinationPath
    )

    Write-Status "Extracting archive..."

    # Create destination if it doesn't exist
    if (-not (Test-Path $DestinationPath)) {
        New-Item -ItemType Directory -Path $DestinationPath -Force | Out-Null
    }

    # Use tar (available in Windows 10 1803+)
    try {
        $tarArgs = @("-xjf", "`"$ArchivePath`"", "-C", "`"$DestinationPath`"")
        $process = Start-Process -FilePath "tar" -ArgumentList $tarArgs -NoNewWindow -Wait -PassThru

        if ($process.ExitCode -ne 0) {
            throw "tar extraction failed with exit code $($process.ExitCode)"
        }

        return $true
    }
    catch {
        Write-Status "tar extraction failed: $_" -Type Error
        Write-Status "Make sure you have Windows 10 version 1803 or later" -Type Warning
        return $false
    }
}

function Move-CefContents {
    param(
        [string]$SourceDir,
        [string]$DestDir
    )

    Write-Status "Moving CEF files to $DestDir"

    # Find the extracted CEF directory
    $cefDir = Get-ChildItem -Path $SourceDir -Directory | Where-Object { $_.Name -like "cef_binary_*" } | Select-Object -First 1

    if (-not $cefDir) {
        throw "Could not find extracted CEF directory in $SourceDir"
    }

    Write-Status "Found CEF directory: $($cefDir.Name)"

    # Create destination directory
    if (Test-Path $DestDir) {
        Write-Status "Removing existing CEF directory..." -Type Warning
        Remove-Item -Path $DestDir -Recurse -Force
    }

    # Move contents
    Move-Item -Path $cefDir.FullName -Destination $DestDir -Force

    return $true
}

# Main execution
function Main {
    Write-Host ""
    Write-Host "=================================" -ForegroundColor Cyan
    Write-Host "  CEF Download Script for Windows" -ForegroundColor Cyan
    Write-Host "=================================" -ForegroundColor Cyan
    Write-Host ""

    # Determine project root and output path
    $projectRoot = Get-ProjectRoot

    if ([string]::IsNullOrEmpty($OutputPath)) {
        $OutputPath = Join-Path $projectRoot "cef"
    }

    Write-Status "Project root: $projectRoot"
    Write-Status "CEF output path: $OutputPath"

    # Check if CEF already exists
    if ((Test-CefPresent $OutputPath) -and -not $Force) {
        Write-Status "CEF binaries already present at $OutputPath" -Type Success
        Write-Status "Use -Force to re-download"
        return
    }

    # Determine architecture
    $arch = if ($Architecture -eq "auto") { Get-SystemArchitecture } else { $Architecture }
    Write-Status "Target architecture: $arch"

    # Get platform string
    $platform = Get-CefPlatformString $arch
    Write-Status "CEF platform: $platform"

    # Get download URL
    $downloadInfo = Get-DownloadUrl -Ver $Version -Platform $platform -IsMinimal $Minimal
    Write-Status "CEF version: $Version"

    # Create temp directory
    $tempDir = Join-Path $env:TEMP "cef_download_$(Get-Random)"
    New-Item -ItemType Directory -Path $tempDir -Force | Out-Null
    Write-Status "Temp directory: $tempDir"

    try {
        # Download
        $archivePath = Join-Path $tempDir $downloadInfo.Filename
        $downloadSuccess = Invoke-Download -Url $downloadInfo.Url -OutputFile $archivePath

        if (-not $downloadSuccess) {
            throw "Failed to download CEF"
        }

        Write-Status "Download complete!" -Type Success

        # Verify file exists and has content
        $fileInfo = Get-Item $archivePath
        Write-Status "Archive size: $([math]::Round($fileInfo.Length / 1MB, 2)) MB"

        if ($fileInfo.Length -lt 1MB) {
            throw "Downloaded file seems too small, may be corrupted"
        }

        # Extract
        $extractSuccess = Expand-TarBz2 -ArchivePath $archivePath -DestinationPath $tempDir

        if (-not $extractSuccess) {
            throw "Failed to extract CEF archive"
        }

        Write-Status "Extraction complete!" -Type Success

        # Move to final location
        $moveSuccess = Move-CefContents -SourceDir $tempDir -DestDir $OutputPath

        if (-not $moveSuccess) {
            throw "Failed to move CEF files"
        }

        Write-Status "CEF installation complete!" -Type Success

        # Verify installation
        if (Test-CefPresent $OutputPath) {
            Write-Host ""
            Write-Status "CEF binaries successfully installed to:" -Type Success
            Write-Host "  $OutputPath" -ForegroundColor Green
            Write-Host ""
            Write-Status "You can now build ki-browser-standalone with 'cargo build'" -Type Info
        }
        else {
            Write-Status "CEF installation verification failed" -Type Warning
        }
    }
    finally {
        # Cleanup temp directory
        if (Test-Path $tempDir) {
            Write-Status "Cleaning up temp files..."
            Remove-Item -Path $tempDir -Recurse -Force -ErrorAction SilentlyContinue
        }
    }
}

# Run main function
Main
