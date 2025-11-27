<#
.SYNOPSIS
    Run tests locally using the same configuration as CI (cargo nextest)

.DESCRIPTION
    This script runs tests using cargo-nextest with the same flags as GitHub Actions.
    Faster than Docker but may have environment differences.

.PARAMETER Install
    Install cargo-nextest if not present

.PARAMETER TestFilter
    Filter tests by name

.PARAMETER FailFast
    Stop on first failure (default: true)

.EXAMPLE
    .\scripts\test-ci-local.ps1
    
.EXAMPLE
    .\scripts\test-ci-local.ps1 -TestFilter "integration"
#>

param(
    [switch]$Install,
    [string]$TestFilter = "",
    [bool]$FailFast = $true
)

$ErrorActionPreference = "Stop"

Write-Host "========================================" -ForegroundColor Cyan
Write-Host "  Nexus Local CI Test Runner" -ForegroundColor Cyan
Write-Host "========================================" -ForegroundColor Cyan
Write-Host ""

# Check if cargo-nextest is installed
$nextestInstalled = cargo nextest --version 2>$null
if (-not $nextestInstalled -or $Install) {
    Write-Host "Installing cargo-nextest..." -ForegroundColor Yellow
    cargo install cargo-nextest --locked
    if ($LASTEXITCODE -ne 0) {
        Write-Host "ERROR: Failed to install cargo-nextest" -ForegroundColor Red
        exit 1
    }
}

Write-Host "Building tests..." -ForegroundColor Yellow
cargo build --tests --workspace
if ($LASTEXITCODE -ne 0) {
    Write-Host "ERROR: Build failed" -ForegroundColor Red
    exit 1
}

Write-Host ""
Write-Host "Running tests with nextest..." -ForegroundColor Yellow
Write-Host ""

# Build test command
$testArgs = @("nextest", "run", "--workspace", "--no-default-features")

if ($TestFilter) {
    $testArgs += "-E"
    $testArgs += "test($TestFilter)"
}

if (-not $FailFast) {
    $testArgs += "--no-fail-fast"
}

cargo @testArgs
$exitCode = $LASTEXITCODE

Write-Host ""
if ($exitCode -eq 0) {
    Write-Host "========================================" -ForegroundColor Green
    Write-Host "  ALL TESTS PASSED!" -ForegroundColor Green
    Write-Host "========================================" -ForegroundColor Green
} else {
    Write-Host "========================================" -ForegroundColor Red
    Write-Host "  TESTS FAILED (exit code: $exitCode)" -ForegroundColor Red
    Write-Host "========================================" -ForegroundColor Red
}

exit $exitCode

