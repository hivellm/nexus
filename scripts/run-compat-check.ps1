# Compatibility Test Runner with Detailed Output
# Run this in PowerShell and share the results

$ErrorActionPreference = "Continue"

Write-Host "═══════════════════════════════════════════════════════" -ForegroundColor Cyan
Write-Host "  Neo4j vs Nexus Compatibility Test" -ForegroundColor Cyan
Write-Host "═══════════════════════════════════════════════════════" -ForegroundColor Cyan
Write-Host ""

# Check if servers are running
Write-Host "Checking servers..." -ForegroundColor Yellow
try {
    $neo4jCheck = Invoke-WebRequest -Uri "http://localhost:7474" -TimeoutSec 5 -UseBasicParsing -ErrorAction Stop
    Write-Host "✓ Neo4j is running" -ForegroundColor Green
} catch {
    Write-Host "✗ Neo4j is NOT running on localhost:7474" -ForegroundColor Red
    exit 1
}

try {
    $nexusCheck = Invoke-WebRequest -Uri "http://localhost:15474" -TimeoutSec 5 -UseBasicParsing -ErrorAction Stop
    Write-Host "✓ Nexus is running" -ForegroundColor Green
} catch {
    Write-Host "✗ Nexus is NOT running on localhost:15474" -ForegroundColor Red
    exit 1
}

Write-Host ""
Write-Host "Running compatibility tests..." -ForegroundColor Yellow
Write-Host ""

# Run the actual test script
& ".\scripts\test-neo4j-nexus-compatibility-200.ps1" -Verbose

Write-Host ""
Write-Host "═══════════════════════════════════════════════════════" -ForegroundColor Cyan
Write-Host "  Test Complete!" -ForegroundColor Cyan
Write-Host "═══════════════════════════════════════════════════════" -ForegroundColor Cyan


