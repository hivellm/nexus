<#
.SYNOPSIS
    Run tests in Ubuntu Docker container (simulates GitHub Actions environment)

.DESCRIPTION
    This script builds and runs tests in an Ubuntu container similar to GitHub Actions.
    It helps detect issues before pushing to GitHub.

.PARAMETER Build
    Force rebuild the Docker image

.PARAMETER NoCache
    Build Docker image without cache

.PARAMETER TestFilter
    Filter tests by name (e.g., "test_create")

.EXAMPLE
    .\scripts\test-docker-ubuntu.ps1
    
.EXAMPLE
    .\scripts\test-docker-ubuntu.ps1 -Build
    
.EXAMPLE
    .\scripts\test-docker-ubuntu.ps1 -TestFilter "test_create"
#>

param(
    [switch]$Build,
    [switch]$NoCache,
    [string]$TestFilter = ""
)

$ErrorActionPreference = "Stop"

$imageName = "nexus-test-ubuntu"
$dockerfilePath = "scripts/docker/Dockerfile.test-ubuntu"
$projectRoot = (Get-Location).Path

Write-Host "========================================" -ForegroundColor Cyan
Write-Host "  Nexus Test Runner - Ubuntu Docker" -ForegroundColor Cyan
Write-Host "========================================" -ForegroundColor Cyan
Write-Host ""

# Check if Docker is running
try {
    docker info | Out-Null
} catch {
    Write-Host "ERROR: Docker is not running. Please start Docker Desktop." -ForegroundColor Red
    exit 1
}

# Check if image exists or build flag is set
$imageExists = docker images -q $imageName 2>$null
if (-not $imageExists -or $Build) {
    Write-Host "Building Docker image: $imageName" -ForegroundColor Yellow
    
    $buildArgs = @("build", "-f", $dockerfilePath, "-t", $imageName)
    if ($NoCache) {
        $buildArgs += "--no-cache"
    }
    $buildArgs += "."
    
    docker @buildArgs
    if ($LASTEXITCODE -ne 0) {
        Write-Host "ERROR: Failed to build Docker image" -ForegroundColor Red
        exit 1
    }
    Write-Host "Docker image built successfully!" -ForegroundColor Green
}

Write-Host ""
Write-Host "Running tests in Ubuntu container..." -ForegroundColor Yellow
Write-Host ""

# Prepare test command
$testCmd = "cargo nextest run --workspace --no-default-features"
if ($TestFilter) {
    $testCmd += " -E 'test($TestFilter)'"
}

# Run tests in Docker
$dockerArgs = @(
    "run",
    "--rm",
    "-v", "${projectRoot}:/workspace",
    "-w", "/workspace",
    $imageName,
    "bash", "-c", $testCmd
)

docker @dockerArgs
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

