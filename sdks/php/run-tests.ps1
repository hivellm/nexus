# Find PHP installation
$phpPaths = @(
    "C:\php\php.exe",
    "C:\Program Files\PHP\php.exe",
    "$env:LOCALAPPDATA\Programs\PHP\php.exe",
    "$env:ProgramFiles\PHP\v8.3\php.exe"
)

# Search common winget installation locations
$wingetPath = Get-ChildItem -Path "$env:LOCALAPPDATA\Microsoft\WinGet\Packages" -Recurse -Filter "php.exe" -ErrorAction SilentlyContinue | Select-Object -First 1

if ($wingetPath) {
    $phpExe = $wingetPath.FullName
} else {
    foreach ($path in $phpPaths) {
        if (Test-Path $path) {
            $phpExe = $path
            break
        }
    }
}

if (-not $phpExe) {
    Write-Host "PHP not found. Searching system-wide..."
    $phpExe = (Get-ChildItem -Path "C:\" -Recurse -Filter "php.exe" -ErrorAction SilentlyContinue | Select-Object -First 1).FullName
}

if (-not $phpExe) {
    Write-Error "PHP not found!"
    exit 1
}

Write-Host "Using PHP: $phpExe"
& $phpExe --version

# Change to SDK directory
Set-Location "F:\Node\hivellm\nexus\sdks\php"

# Run composer dump-autoload
$composerPhar = "composer.phar"
if (Test-Path $composerPhar) {
    Write-Host "`nRunning composer dump-autoload..."
    & $phpExe $composerPhar dump-autoload
}

# Run PHPUnit tests
Write-Host "`nRunning PHPUnit tests..."
& $phpExe vendor/bin/phpunit --testdox
