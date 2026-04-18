# Nexus CLI Installation Script for Windows
# Usage: iwr -useb https://raw.githubusercontent.com/hivellm/nexus/main/nexus-cli/scripts/install.ps1 | iex

param(
    [string]$InstallDir = "$env:LOCALAPPDATA\Programs\nexus",
    [string]$Version = ""
)

$ErrorActionPreference = "Stop"

# Configuration
$Repo = "hivellm/nexus"
$BinaryName = "nexus.exe"
$ConfigDir = "$env:APPDATA\nexus"

function Write-Info { param($Message) Write-Host "[INFO] " -ForegroundColor Blue -NoNewline; Write-Host $Message }
function Write-Success { param($Message) Write-Host "[SUCCESS] " -ForegroundColor Green -NoNewline; Write-Host $Message }
function Write-Warn { param($Message) Write-Host "[WARNING] " -ForegroundColor Yellow -NoNewline; Write-Host $Message }
function Write-Err { param($Message) Write-Host "[ERROR] " -ForegroundColor Red -NoNewline; Write-Host $Message; exit 1 }

function Get-LatestVersion {
    if ($Version) {
        return $Version
    }

    try {
        $release = Invoke-RestMethod -Uri "https://api.github.com/repos/$Repo/releases/latest" -UseBasicParsing
        $ver = $release.tag_name -replace '^v', ''
        Write-Info "Latest version: $ver"
        return $ver
    }
    catch {
        $fallback = "0.11.0"
        Write-Warn "Could not determine latest version, using $fallback"
        return $fallback
    }
}

function Install-Binary {
    param($Ver)

    $arch = if ([Environment]::Is64BitOperatingSystem) { "x86_64" } else { "i686" }
    $downloadUrl = "https://github.com/$Repo/releases/download/v$Ver/nexus-windows-$arch.zip"

    Write-Info "Downloading from: $downloadUrl"

    # Create temp directory
    $tempDir = Join-Path $env:TEMP "nexus-install-$(Get-Random)"
    New-Item -ItemType Directory -Path $tempDir -Force | Out-Null

    try {
        # Download
        $zipPath = Join-Path $tempDir "nexus.zip"
        Invoke-WebRequest -Uri $downloadUrl -OutFile $zipPath -UseBasicParsing

        # Extract
        Write-Info "Extracting..."
        Expand-Archive -Path $zipPath -DestinationPath $tempDir -Force

        # Create install directory
        if (-not (Test-Path $InstallDir)) {
            New-Item -ItemType Directory -Path $InstallDir -Force | Out-Null
        }

        # Move binary
        Write-Info "Installing to $InstallDir..."
        $sourceBinary = Get-ChildItem -Path $tempDir -Filter "nexus.exe" -Recurse | Select-Object -First 1
        if ($sourceBinary) {
            Copy-Item -Path $sourceBinary.FullName -Destination (Join-Path $InstallDir $BinaryName) -Force
        }
        else {
            # If not found in archive, try building from source
            Write-Warn "Binary not found in release, attempting to use local build..."
            $localBinary = ".\target\release\nexus.exe"
            if (Test-Path $localBinary) {
                Copy-Item -Path $localBinary -Destination (Join-Path $InstallDir $BinaryName) -Force
            }
            else {
                Write-Err "Could not find nexus binary"
            }
        }
    }
    finally {
        # Cleanup
        Remove-Item -Path $tempDir -Recurse -Force -ErrorAction SilentlyContinue
    }
}

function Update-Path {
    $currentPath = [Environment]::GetEnvironmentVariable("PATH", "User")

    if ($currentPath -notlike "*$InstallDir*") {
        Write-Info "Adding $InstallDir to PATH..."
        $newPath = "$currentPath;$InstallDir"
        [Environment]::SetEnvironmentVariable("PATH", $newPath, "User")

        # Update current session
        $env:PATH = "$env:PATH;$InstallDir"

        Write-Success "PATH updated. You may need to restart your terminal."
    }
}

function New-DefaultConfig {
    if (-not (Test-Path $ConfigDir)) {
        Write-Info "Creating config directory: $ConfigDir"
        New-Item -ItemType Directory -Path $ConfigDir -Force | Out-Null
    }

    $configFile = Join-Path $ConfigDir "config.toml"
    if (-not (Test-Path $configFile)) {
        Write-Info "Creating default configuration..."
        @"
# Nexus CLI Configuration
# See: nexus config --help

url = "http://localhost:3000"
# username = "root"
# password = ""
# api_key = ""

# Connection profiles
# [profiles.production]
# url = "https://production.example.com:3000"
# api_key = "your-api-key"

# [profiles.staging]
# url = "https://staging.example.com:3000"
# api_key = "your-api-key"
"@ | Out-File -FilePath $configFile -Encoding utf8
    }
}

function Test-Installation {
    $binaryPath = Join-Path $InstallDir $BinaryName

    if (Test-Path $binaryPath) {
        try {
            $version = & $binaryPath --version 2>$null
            Write-Success "Nexus CLI installed successfully!"
            Write-Info "Version: $version"
            Write-Info "Location: $binaryPath"
        }
        catch {
            Write-Success "Binary installed at: $binaryPath"
        }
    }
    else {
        Write-Err "Installation failed - binary not found"
    }
}

# Main
function Main {
    Write-Host ""
    Write-Host "================================" -ForegroundColor Cyan
    Write-Host "  Nexus CLI Installer (Windows)" -ForegroundColor Cyan
    Write-Host "================================" -ForegroundColor Cyan
    Write-Host ""

    $ver = Get-LatestVersion
    Install-Binary -Ver $ver
    New-DefaultConfig
    Update-Path
    Test-Installation

    Write-Host ""
    Write-Host "Quick start:" -ForegroundColor Cyan
    Write-Host "  nexus --help"
    Write-Host "  nexus config init"
    Write-Host "  nexus db ping"
    Write-Host ""
}

Main
