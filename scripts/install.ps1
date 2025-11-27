# Nexus Installation Script for Windows
# Usage: powershell -c "irm https://raw.githubusercontent.com/hivellm/nexus/main/scripts/install.ps1 | iex"

$ErrorActionPreference = "Stop"

# Configuration
$REPO = "hivellm/nexus"
$INSTALL_DIR = if ($env:NEXUS_INSTALL_DIR) { $env:NEXUS_INSTALL_DIR } else { "$env:ProgramFiles\Nexus" }
$SERVICE_NAME = "Nexus"
$DATA_DIR = if ($env:NEXUS_DATA_DIR) { $env:NEXUS_DATA_DIR } else { "$env:ProgramData\Nexus" }
$LOG_DIR = "$env:ProgramData\Nexus\logs"

# Colors for output
function Write-ColorOutput {
    param(
        [string]$Message,
        [string]$Color = "White"
    )
    Write-Host $Message -ForegroundColor $Color
}

# Detect architecture
function Get-Architecture {
    $arch = (Get-WmiObject Win32_Processor).Architecture
    switch ($arch) {
        0 { return "x86_64" }  # x86
        5 { return "aarch64" } # ARM64
        9 { return "x86_64" }  # x64
        default { 
            Write-ColorOutput "Unsupported architecture: $arch" "Red"
            exit 1
        }
    }
}

# Get latest release version from GitHub
function Get-LatestVersion {
    $response = Invoke-RestMethod -Uri "https://api.github.com/repos/$REPO/releases/latest" -ErrorAction Stop
    $version = $response.tag_name -replace '^v', ''
    return $version
}

# Download binary from GitHub releases
function Get-Binary {
    param(
        [string]$Version,
        [string]$Platform
    )
    
    $downloadUrl = "https://github.com/$REPO/releases/download/v$Version/nexus-server-$Platform.exe"
    $tempFile = Join-Path $env:TEMP "nexus-install.exe"
    
    Write-ColorOutput "Downloading Nexus v$Version for $Platform..." "Yellow"
    
    try {
        Invoke-WebRequest -Uri $downloadUrl -OutFile $tempFile -ErrorAction Stop
        Write-ColorOutput "Download complete" "Green"
        return $tempFile
    }
    catch {
        Write-ColorOutput "Failed to download binary from $downloadUrl" "Red"
        Write-ColorOutput $_.Exception.Message "Red"
        exit 1
    }
}

# Install binary
function Install-Binary {
    param(
        [string]$BinaryPath
    )
    
    Write-ColorOutput "Installing binary to $INSTALL_DIR..." "Yellow"
    
    # Create install directory
    if (-not (Test-Path $INSTALL_DIR)) {
        New-Item -ItemType Directory -Path $INSTALL_DIR -Force | Out-Null
    }
    
    $targetPath = Join-Path $INSTALL_DIR "nexus-server.exe"
    
    # Copy binary
    Copy-Item -Path $BinaryPath -Destination $targetPath -Force
    
    # Add to PATH if not already present
    $currentPath = [Environment]::GetEnvironmentVariable("Path", "Machine")
    if ($currentPath -notlike "*$INSTALL_DIR*") {
        Write-ColorOutput "Adding $INSTALL_DIR to PATH..." "Yellow"
        [Environment]::SetEnvironmentVariable("Path", "$currentPath;$INSTALL_DIR", "Machine")
        $env:Path = "$env:Path;$INSTALL_DIR"
    }
    
    # Cleanup temp file
    Remove-Item -Path $BinaryPath -Force -ErrorAction SilentlyContinue
    
    Write-ColorOutput "Binary installed successfully" "Green"
}

# Create Windows Service
function Install-Service {
    Write-ColorOutput "Creating Windows service..." "Yellow"
    
    # Create data and log directories
    if (-not (Test-Path $DATA_DIR)) {
        New-Item -ItemType Directory -Path $DATA_DIR -Force | Out-Null
    }
    if (-not (Test-Path $LOG_DIR)) {
        New-Item -ItemType Directory -Path $LOG_DIR -Force | Out-Null
    }
    
    $binaryPath = Join-Path $INSTALL_DIR "nexus-server.exe"
    
    # Check if service already exists
    $existingService = Get-Service -Name $SERVICE_NAME -ErrorAction SilentlyContinue
    if ($existingService) {
        Write-ColorOutput "Service already exists, stopping and removing..." "Yellow"
        Stop-Service -Name $SERVICE_NAME -Force -ErrorAction SilentlyContinue
        sc.exe delete $SERVICE_NAME | Out-Null
        Start-Sleep -Seconds 2
    }
    
    # Create service using sc.exe
    $result = sc.exe create $SERVICE_NAME `
        binPath= "`"$binaryPath`"" `
        start= auto `
        DisplayName= "Nexus Graph Database Server" `
        Description= "Nexus graph database server with native vector search"
    
    # Set environment variable for data directory
    [Environment]::SetEnvironmentVariable("NEXUS_DATA_DIR", $DATA_DIR, "Machine")
    
    if ($LASTEXITCODE -ne 0) {
        Write-ColorOutput "Failed to create service: $result" "Red"
        exit 1
    }
    
    # Configure service recovery
    sc.exe failure $SERVICE_NAME reset= 86400 actions= restart/5000/restart/10000/restart/20000 | Out-Null
    
    Write-ColorOutput "Service created successfully" "Green"
}

# Start service
function Start-NexusService {
    Write-ColorOutput "Starting Nexus service..." "Yellow"
    
    try {
        Start-Service -Name $SERVICE_NAME -ErrorAction Stop
        Write-ColorOutput "Service started successfully" "Green"
    }
    catch {
        Write-ColorOutput "Failed to start service: $_.Exception.Message" "Red"
        Write-ColorOutput "You may need to start it manually: Start-Service -Name $SERVICE_NAME" "Yellow"
    }
}

# Verify installation
function Test-Installation {
    Write-ColorOutput "Verifying installation..." "Yellow"
    
    $binaryPath = Join-Path $INSTALL_DIR "nexus-server.exe"
    
    if (-not (Test-Path $binaryPath)) {
        Write-ColorOutput "Nexus server binary not found at $binaryPath" "Red"
        return $false
    }
    
    try {
        $version = & $binaryPath --version 2>&1
        Write-ColorOutput "Nexus installed successfully!" "Green"
        Write-ColorOutput "Version: $version" "Green"
        Write-ColorOutput "Binary location: $binaryPath" "Green"
        
        $service = Get-Service -Name $SERVICE_NAME -ErrorAction SilentlyContinue
        if ($service) {
            Write-ColorOutput "Service status: $($service.Status)" "Green"
        }
        
        return $true
    }
    catch {
        Write-ColorOutput "Failed to verify installation: $_.Exception.Message" "Red"
        return $false
    }
}

# Main installation function
function Main {
    Write-ColorOutput "=== Nexus Installation ===" "Green"
    Write-Host ""
    
    # Check if running as administrator
    $isAdmin = ([Security.Principal.WindowsPrincipal] [Security.Principal.WindowsIdentity]::GetCurrent()).IsInRole([Security.Principal.WindowsBuiltInRole]::Administrator)
    if (-not $isAdmin) {
        Write-ColorOutput "This script requires administrator privileges." "Red"
        Write-ColorOutput "Please run PowerShell as Administrator and try again." "Yellow"
        exit 1
    }
    
    # Detect platform
    $platform = Get-Architecture
    Write-ColorOutput "Detected platform: $platform" "Yellow"
    
    # Get latest version
    Write-ColorOutput "Fetching latest version..." "Yellow"
    $version = Get-LatestVersion
    Write-ColorOutput "Latest version: v$version" "Green"
    
    # Download binary
    $tempBinary = Get-Binary -Version $version -Platform $platform
    
    # Install binary
    Install-Binary -BinaryPath $tempBinary
    
    # Create and start service
    Install-Service
    Start-NexusService
    
    # Verify
    if (Test-Installation) {
        Write-Host ""
        Write-ColorOutput "Installation complete!" "Green"
        Write-Host ""
        Write-ColorOutput "Usage:" "Yellow"
        Write-Host "  nexus-server --help"
        Write-Host ""
        Write-Host "Server will be available at: http://localhost:15474"
        Write-Host ""
        Write-ColorOutput "Service management:" "Yellow"
        Write-Host "  Get-Service -Name $SERVICE_NAME"
        Write-Host "  Start-Service -Name $SERVICE_NAME"
        Write-Host "  Stop-Service -Name $SERVICE_NAME"
        Write-Host "  Restart-Service -Name $SERVICE_NAME"
    }
    else {
        Write-ColorOutput "Installation completed with errors. Please check the output above." "Red"
        exit 1
    }
}

# Run main function
Main

