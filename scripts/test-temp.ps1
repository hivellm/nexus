<#
.SYNOPSIS
    Run GitHub Actions workflows locally using act

.DESCRIPTION
    This script facilitates running GitHub Actions workflows locally
    using act, simulating the GitHub Actions environment in Docker.

.PARAMETER Job
    Name of the job to execute. Use "list" to see available jobs, 
    or "all" to run all jobs.

.EXAMPLE
    .\scripts\act-run-workflows.ps1
    Lists all available workflows

.EXAMPLE
    .\scripts\act-run-workflows.ps1 rust-tests
    Runs the rust-tests job

.EXAMPLE
    .\scripts\act-run-workflows.ps1 all
    Runs all jobs
#>

param(
    [string]$Job = "list"
)

$ErrorActionPreference = "Stop"

$ProjectRoot = (Get-Location).Path
$ActPath = Join-Path $ProjectRoot "act"

# Check if act is available
if (-not (Test-Path $ActPath)) {
    Write-Host "act not found. Downloading..." -ForegroundColor Yellow
    
    $ActUrl = "https://github.com/nektos/act/releases/latest/download/act_Linux_x86_64.tar.gz"
    $TempFile = Join-Path $env:TEMP "act.tar.gz"
    
    Invoke-WebRequest -Uri $ActUrl -OutFile $TempFile
    
    # Extract using WSL
    wsl -d Ubuntu-24.04 -- bash -l -c "cd /mnt/f/Node/hivellm/nexus && tar -xzf `$(wslpath -u '$TempFile') && chmod +x act"
    
    Remove-Item $TempFile -ErrorAction SilentlyContinue
}

# Check Docker (Windows PowerShell)
$dockerAvailable = $false
try {
    $null = docker info 2>&1
    if ($LASTEXITCODE -eq 0) {
        $dockerAvailable = $true
        Write-Host "Docker is accessible (Windows PowerShell)" -ForegroundColor Green
    }
} catch {
    # Try alternative check
    try {
        $dockerVersion = docker --version 2>&1
        if ($LASTEXITCODE -eq 0) {
            $dockerAvailable = $true
            Write-Host "Docker is accessible (Windows PowerShell)" -ForegroundColor Green
        }
    } catch {
        # Docker not available
    }
}

if (-not $dockerAvailable) {
    Write-Host "WARNING: Could not verify Docker in PowerShell" -ForegroundColor Yellow
    Write-Host "The script will attempt to connect anyway." -ForegroundColor Yellow
    Write-Host ""
    Write-Host "If you get connection errors, make sure:" -ForegroundColor Yellow
    Write-Host "1. Docker Desktop is running" -ForegroundColor Yellow
    Write-Host "2. Docker Desktop > Settings > General > Expose daemon on tcp://localhost:2375" -ForegroundColor Yellow
    Write-Host ""
}

# Get Windows host IP for WSL to connect to Docker
# In WSL2, the Windows host is typically the .1 address of the WSL network
# We'll get the WSL IP and replace the last octet with .1
$wslIpOutput = wsl -d Ubuntu-24.04 -- bash -l -c "hostname -I | awk '{print `$1}'"
$wslIp = $wslIpOutput.Trim()

if ($wslIp -match '^(\d+\.\d+\.\d+)\.\d+$') {
    # Replace last octet with .1 (Windows host in WSL2)
    $windowsHostIp = $matches[1] + ".1"
} else {
    # Fallback: try to get from route
    $routeOutput = wsl -d Ubuntu-24.04 -- bash -l -c "ip route show default"
    if ($routeOutput -match 'default via (\d+\.\d+\.\d+\.\d+)') {
        $windowsHostIp = $matches[1]
    } else {
        # Last resort: common WSL2 Windows host IP
        $windowsHostIp = "172.20.144.1"
    }
}

Write-Host "Windows host IP for WSL: $windowsHostIp" -ForegroundColor Cyan
Write-Host "Note: Docker Desktop must expose daemon on TCP port 2375 or 2376" -ForegroundColor Yellow
Write-Host "      Go to Docker Desktop > Settings > General > Expose daemon on tcp://localhost:2375" -ForegroundColor Yellow

# Compatible Docker image
$ActImage = "ghcr.io/catthehacker/ubuntu:act-latest"

# Function to list workflows
function List-Workflows {
    Write-Host "`nAvailable workflows:" -ForegroundColor Green
    Write-Host ""
    
    # Try TCP connection to Windows Docker
    $wslCommand = "cd /mnt/f/Node/hivellm/nexus && DOCKER_HOST=tcp://$windowsHostIp`:2375 ./act -l"
    
    wsl -d Ubuntu-24.04 -- bash -l -c $wslCommand
    
    Write-Host "`nTo run a specific job:" -ForegroundColor Yellow
    Write-Host "  .\scripts\act-run-workflows.ps1 <job-name>"
    Write-Host ""
    Write-Host "Examples:" -ForegroundColor Yellow
    Write-Host "  .\scripts\act-run-workflows.ps1 rust-tests"
    Write-Host "  .\scripts\act-run-workflows.ps1 lint"
    Write-Host "  .\scripts\act-run-workflows.ps1 codespell"
}

# Function to run a job
function Run-Job {
    param([string]$JobName)
    
    Write-Host "`nRunning job: $JobName" -ForegroundColor Green
    Write-Host ""
    
    # Connect to Docker on Windows via TCP
    # Docker Desktop must expose daemon on tcp://localhost:2375
    # In WSL, we connect to Windows host IP
    $wslCommand = "cd /mnt/f/Node/hivellm/nexus && DOCKER_HOST=tcp://$windowsHostIp`:2375 ./act -j `"$JobName`" --container-architecture linux/amd64 --image ubuntu-latest=`"$ActImage`" --pull=false --rm"
    
    wsl -d Ubuntu-24.04 -- bash -l -c $wslCommand
    
    if ($LASTEXITCODE -ne 0) {
        Write-Host "`nIf you got a connection error, make sure:" -ForegroundColor Yellow
        Write-Host "1. Docker Desktop is running" -ForegroundColor Yellow
        Write-Host "2. Docker Desktop > Settings > General > Expose daemon on tcp://localhost:2375" -ForegroundColor Yellow
        Write-Host "3. Without TLS (uncheck 'Use TLS')" -ForegroundColor Yellow
    }
}

# Function to run all jobs
function Run-All {
    Write-Host "Running all jobs..." -ForegroundColor Green
    Write-Host ""
    
    $jobs = @("rust-tests", "lint", "codespell")
    
    foreach ($job in $jobs) {
        Write-Host "=== Running $job ===" -ForegroundColor Yellow
        Run-Job $job
        Write-Host ""
    }
}

# Main
switch ($Job.ToLower()) {
    "list" { List-Workflows }
    "all" { Run-All }
    default { Run-Job $Job }
}
