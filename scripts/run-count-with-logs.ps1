#!/usr/bin/env pwsh
# Run COUNT query and capture debug logs

Write-Host "=== Starting Nexus with Debug Logs ===" -ForegroundColor Cyan

# Start server and pipe output to file
$logFileOut = "F:\Node\hivellm\nexus\debug-output.log"
$logFileErr = "F:\Node\hivellm\nexus\debug-error.log"
$serverPath = "F:\Node\hivellm\nexus\target\release\nexus-server.exe"

Write-Host "Starting server (logs will be written to $logFileOut and $logFileErr)..." -ForegroundColor Yellow
Start-Process -FilePath $serverPath -RedirectStandardOutput $logFileOut -RedirectStandardError $logFileErr -NoNewWindow

Write-Host "Waiting for server to start..." -ForegroundColor Yellow
Start-Sleep -Seconds 5

# Test health
try {
    $health = Invoke-RestMethod -Uri "http://localhost:15474/health" -Method GET -TimeoutSec 5
    Write-Host "Server is healthy!" -ForegroundColor Green
} catch {
    Write-Host "ERROR: Server not responding" -ForegroundColor Red
    if (Test-Path $logFileErr) {
        Write-Host "Error log:" -ForegroundColor Yellow
        Get-Content $logFileErr | Select-Object -Last 20
    }
    if (Test-Path $logFileOut) {
        Write-Host "Output log:" -ForegroundColor Yellow
        Get-Content $logFileOut | Select-Object -Last 20
    }
    exit 1
}

# Execute COUNT query
Write-Host "`n=== Executing COUNT query ===" -ForegroundColor Cyan
$query = "MATCH (d:Document) RETURN count(d) AS total"
Write-Host "Query: $query" -ForegroundColor Yellow

try {
    $response = Invoke-RestMethod -Uri "http://localhost:15474/cypher" -Method POST -Body (@{
        query = $query
    } | ConvertTo-Json) -ContentType "application/json" -TimeoutSec 10
    
    Write-Host "`nResponse:" -ForegroundColor Green
    $response | ConvertTo-Json -Depth 10
} catch {
    Write-Host "ERROR executing query: $_" -ForegroundColor Red
}

# Wait for logs to flush
Start-Sleep -Seconds 2

# Show relevant debug logs
Write-Host "`n=== Debug Logs ===" -ForegroundColor Cyan
Write-Host "Error log (stderr):" -ForegroundColor Yellow
if (Test-Path $logFileErr) {
    Get-Content $logFileErr | Select-Object -Last 150 | Where-Object { $_ -match "DEBUG|ERROR|WARN|implicit|COUNT|aggregate" }
} else {
    Write-Host "No error log found" -ForegroundColor Gray
}

Write-Host "`nOutput log (stdout):" -ForegroundColor Yellow
if (Test-Path $logFileOut) {
    Get-Content $logFileOut | Select-Object -Last 150 | Where-Object { $_ -match "DEBUG|ERROR|WARN|implicit|COUNT|aggregate" }
} else {
    Write-Host "No output log found" -ForegroundColor Gray
}

Write-Host "`n=== Full logs available at: ===" -ForegroundColor Yellow
Write-Host "  Stdout: $logFileOut" -ForegroundColor Cyan
Write-Host "  Stderr: $logFileErr" -ForegroundColor Cyan
Write-Host "Server is still running. Press Ctrl+C to stop." -ForegroundColor Yellow

