# Install PHP and Composer on Windows
$ErrorActionPreference = "Stop"

$phpDir = "C:\php"
$phpZip = "$phpDir\php.zip"
$phpUrl = "https://windows.php.net/downloads/releases/php-8.3.15-nts-Win32-vs16-x64.zip"
$composerUrl = "https://getcomposer.org/download/latest-stable/composer.phar"

Write-Host "Creating PHP directory..."
if (-not (Test-Path $phpDir)) {
    New-Item -ItemType Directory -Path $phpDir -Force | Out-Null
}

Write-Host "Downloading PHP 8.3.13..."
Invoke-WebRequest -Uri $phpUrl -OutFile $phpZip -UseBasicParsing

Write-Host "Extracting PHP..."
Expand-Archive -Path $phpZip -DestinationPath $phpDir -Force
Remove-Item $phpZip

# Copy php.ini-development to php.ini
$phpIniDev = "$phpDir\php.ini-development"
$phpIni = "$phpDir\php.ini"
if (Test-Path $phpIniDev) {
    Copy-Item $phpIniDev $phpIni -Force
    # Enable extensions needed for Composer
    (Get-Content $phpIni) -replace ';extension=curl', 'extension=curl' `
        -replace ';extension=openssl', 'extension=openssl' `
        -replace ';extension=mbstring', 'extension=mbstring' `
        -replace ';extension_dir = "ext"', 'extension_dir = "ext"' | Set-Content $phpIni
}

Write-Host "Downloading Composer..."
Invoke-WebRequest -Uri $composerUrl -OutFile "$phpDir\composer.phar" -UseBasicParsing

# Create composer.bat wrapper
@"
@echo off
php "%~dp0composer.phar" %*
"@ | Out-File -FilePath "$phpDir\composer.bat" -Encoding ASCII

Write-Host "Adding PHP to PATH..."
$env:Path = "$phpDir;$env:Path"

Write-Host ""
Write-Host "Installation complete!"
Write-Host "PHP: $phpDir\php.exe"
Write-Host "Composer: $phpDir\composer.bat"
Write-Host ""
Write-Host "Testing installation..."
& "$phpDir\php.exe" -v
