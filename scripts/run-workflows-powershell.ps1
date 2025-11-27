<#
.SYNOPSIS
    Run GitHub Actions workflows locally using Docker directly in PowerShell

.DESCRIPTION
    This script runs the same commands as GitHub Actions workflows
    but directly in PowerShell using Docker, without needing act.

.PARAMETER Job
    Name of the job to execute: "rust-tests", "lint", or "codespell"

.EXAMPLE
    .\scripts\run-workflows-powershell.ps1 rust-tests
    Runs the rust-tests job

.EXAMPLE
    .\scripts\run-workflows-powershell.ps1 lint
    Runs the lint job
#>

param(
    [Parameter(Mandatory=$true)]
    [ValidateSet("rust-tests", "lint", "codespell")]
    [string]$Job
)

$ErrorActionPreference = "Stop"

# Check Docker
$dockerAvailable = $false
try {
    $dockerVersion = docker --version 2>&1
    if ($LASTEXITCODE -eq 0) {
        $dockerAvailable = $true
        Write-Host "Docker is accessible: $dockerVersion" -ForegroundColor Green
    }
} catch {
    # Docker not available
}

if (-not $dockerAvailable) {
    Write-Host "ERROR: Docker is not accessible" -ForegroundColor Red
    Write-Host "Make sure Docker Desktop is running." -ForegroundColor Yellow
    exit 1
}

$ProjectRoot = (Get-Location).Path
$ImageName = "nexus-github-actions-runner"

# Build Docker image if needed
function Build-Image {
    Write-Host "`nBuilding Docker image..." -ForegroundColor Yellow
    
    $dockerfile = @"
FROM ubuntu:22.04

ENV DEBIAN_FRONTEND=noninteractive

# Install dependencies
RUN apt-get update && apt-get install -y \
    build-essential \
    curl \
    git \
    pkg-config \
    libssl-dev \
    clang \
    lld \
    mold \
    && rm -rf /var/lib/apt/lists/*

# Install Rust stable
RUN curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh -s -- -y --default-toolchain stable
ENV PATH="/root/.cargo/bin:`${PATH}"

# Install cargo-nextest
RUN cargo install cargo-nextest --locked

# Configure mold linker
RUN mkdir -p /root/.cargo && \
    echo '[target.x86_64-unknown-linux-gnu]' >> /root/.cargo/config.toml && \
    echo 'linker = "clang"' >> /root/.cargo/config.toml && \
    echo 'rustflags = ["-C", "link-arg=-fuse-ld=mold"]' >> /root/.cargo/config.toml

WORKDIR /workspace
"@

    $dockerfilePath = Join-Path $env:TEMP "Dockerfile.workflow"
    $dockerfile | Out-File -FilePath $dockerfilePath -Encoding ASCII
    
    Write-Host "Building image (this may take a few minutes)..." -ForegroundColor Yellow
    docker build -t $ImageName -f $dockerfilePath $ProjectRoot 2>&1 | Out-Host
    if ($LASTEXITCODE -ne 0) {
        Write-Host "ERROR: Failed to build Docker image" -ForegroundColor Red
        Remove-Item $dockerfilePath -ErrorAction SilentlyContinue
        exit 1
    }
    
    Remove-Item $dockerfilePath -ErrorAction SilentlyContinue
    Write-Host "Docker image built successfully!" -ForegroundColor Green
}

# Check if image exists
$imageExists = docker images -q $ImageName 2>$null
if (-not $imageExists) {
    Build-Image
}

# Run job
Write-Host "`nRunning job: $Job" -ForegroundColor Green
Write-Host ""

switch ($Job) {
    "rust-tests" {
        Write-Host "Running Rust tests..." -ForegroundColor Cyan
        docker run --rm `
            -v "${ProjectRoot}:/workspace" `
            -w /workspace `
            -e CARGO_TERM_COLOR=always `
            $ImageName `
            bash -c "cargo build --tests --workspace && cargo nextest run --workspace --no-default-features"
    }
    
    "lint" {
        Write-Host "Running lint checks..." -ForegroundColor Cyan
        
        # Install nightly for fmt
        docker run --rm `
            -v "${ProjectRoot}:/workspace" `
            -w /workspace `
            -e CARGO_TERM_COLOR=always `
            $ImageName `
            bash -c @"
rustup toolchain install nightly --component rustfmt
cargo +nightly fmt --all -- --check
cargo clippy --workspace -- -D warnings
cargo clippy --workspace --all-targets -- -D warnings
cargo clippy --workspace --all-targets --all-features -- -D warnings
"@
    }
    
    "codespell" {
        Write-Host "Running codespell..." -ForegroundColor Cyan
        docker run --rm `
            -v "${ProjectRoot}:/workspace" `
            -w /workspace `
            $ImageName `
            bash -c "python3 -m pip install --upgrade 'codespell[toml]' && codespell --skip='*.lock,*.json,*.map,*.yaml,*.yml,target,node_modules,.git,dist' --ignore-words-list='crate,ser,deser'"
    }
}

$exitCode = $LASTEXITCODE

Write-Host ""
if ($exitCode -eq 0) {
    Write-Host "========================================" -ForegroundColor Green
    Write-Host "  JOB COMPLETED SUCCESSFULLY!" -ForegroundColor Green
    Write-Host "========================================" -ForegroundColor Green
} else {
    Write-Host "========================================" -ForegroundColor Red
    Write-Host "  JOB FAILED (exit code: $exitCode)" -ForegroundColor Red
    Write-Host "========================================" -ForegroundColor Red
}

exit $exitCode

