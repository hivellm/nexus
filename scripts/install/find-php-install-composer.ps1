# Find PHP and install Composer
$ErrorActionPreference = "Stop"

# Refresh PATH from environment
$env:Path = [System.Environment]::GetEnvironmentVariable("Path", "Machine") + ";" + [System.Environment]::GetEnvironmentVariable("Path", "User")

# Find PHP
$phpPath = $null
$possiblePaths = @(
    "C:\php\php.exe",
    "C:\Program Files\PHP\php.exe",
    "C:\Program Files (x86)\PHP\php.exe",
    "$env:LOCALAPPDATA\Programs\PHP\php.exe",
    "$env:LOCALAPPDATA\Programs\PHP\v8.3\php.exe",
    "$env:ProgramFiles\PHP\v8.3\php.exe"
)

# Search in PATH
$pathDirs = $env:Path -split ";"
foreach ($dir in $pathDirs) {
    $candidate = Join-Path $dir "php.exe"
    if (Test-Path $candidate) {
        $phpPath = $candidate
        Write-Host "Found PHP in PATH: $phpPath"
        break
    }
}

# Search in common locations
if (-not $phpPath) {
    foreach ($path in $possiblePaths) {
        if (Test-Path $path) {
            $phpPath = $path
            Write-Host "Found PHP at: $phpPath"
            break
        }
    }
}

if (-not $phpPath) {
    Write-Host "PHP not found. Searching recursively..."
    $found = Get-ChildItem -Path "C:\" -Filter "php.exe" -Recurse -ErrorAction SilentlyContinue | Select-Object -First 1
    if ($found) {
        $phpPath = $found.FullName
        Write-Host "Found PHP at: $phpPath"
    }
}

if (-not $phpPath) {
    Write-Error "PHP not found!"
    exit 1
}

# Test PHP
Write-Host ""
Write-Host "Testing PHP..."
& $phpPath -v

# Get PHP directory
$phpDir = Split-Path $phpPath

# Check if Composer already exists
$composerPath = Join-Path $phpDir "composer.phar"
if (Test-Path $composerPath) {
    Write-Host ""
    Write-Host "Composer already installed at: $composerPath"
} else {
    # Download Composer
    Write-Host ""
    Write-Host "Downloading Composer..."
    $composerUrl = "https://getcomposer.org/download/latest-stable/composer.phar"
    Invoke-WebRequest -Uri $composerUrl -OutFile $composerPath -UseBasicParsing
    Write-Host "Composer downloaded to: $composerPath"
}

# Create composer.bat if it doesn't exist
$composerBat = Join-Path $phpDir "composer.bat"
if (-not (Test-Path $composerBat)) {
    @"
@echo off
"$phpPath" "$composerPath" %*
"@ | Out-File -FilePath $composerBat -Encoding ASCII
    Write-Host "Created composer.bat at: $composerBat"
}

Write-Host ""
Write-Host "Installation complete!"
Write-Host "PHP: $phpPath"
Write-Host "Composer: $composerPath"
Write-Host ""
Write-Host "To use, run:"
Write-Host "  $phpPath $composerPath [command]"
Write-Host "Or add $phpDir to your PATH"
