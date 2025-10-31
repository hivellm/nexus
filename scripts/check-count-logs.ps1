#!/usr/bin/env pwsh
# Script to check COUNT query debug logs

Write-Host "=== Checking COUNT Query Debug Logs ===" -ForegroundColor Cyan

# 1. Stop any running Nexus server
Write-Host "`n1. Stopping existing Nexus server..." -ForegroundColor Yellow
$nexusProcess = Get-Process nexus-server -ErrorAction SilentlyContinue
if ($nexusProcess) {
    Stop-Process -Name nexus-server -Force
    Start-Sleep -Seconds 2
    Write-Host "Server stopped" -ForegroundColor Green
} else {
    Write-Host "No server running" -ForegroundColor Green
}

# 2. Start server in background with output capture
Write-Host "`n2. Starting Nexus server..." -ForegroundColor Yellow
$serverPath = "F:\Node\hivellm\nexus\target\release\nexus-server.exe"
if (-not (Test-Path $serverPath)) {
    Write-Host "ERROR: Server binary not found at $serverPath" -ForegroundColor Red
    exit 1
}

$logFile = "F:\Node\hivellm\nexus\server-debug.log"
if (Test-Path $logFile) {
    Remove-Item $logFile -Force
}

$serverJob = Start-Job -ScriptBlock {
    param($path, $log)
    & $path 2>&1 | Tee-Object -FilePath $log
} -ArgumentList $serverPath, $logFile

Write-Host "Server starting (Job ID: $($serverJob.Id))..." -ForegroundColor Green
Start-Sleep -Seconds 3

# 3. Check server health
Write-Host "`n3. Checking server health..." -ForegroundColor Yellow
try {
    $health = Invoke-RestMethod -Uri "http://localhost:7878/health" -Method GET -TimeoutSec 5
    Write-Host "Server is healthy" -ForegroundColor Green
} catch {
    Write-Host "ERROR: Server not responding: $_" -ForegroundColor Red
    Stop-Job -Job $serverJob
    Remove-Job -Job $serverJob
    exit 1
}

# 4. Execute COUNT query
Write-Host "`n4. Executing COUNT query..." -ForegroundColor Yellow
$query = "MATCH (d:Document) RETURN count(d) AS total"
Write-Host "Query: $query" -ForegroundColor Cyan

try {
    $response = Invoke-RestMethod -Uri "http://localhost:7878/api/cypher" -Method POST -Body (@{
        query = $query
    } | ConvertTo-Json) -ContentType "application/json" -TimeoutSec 10
    
    Write-Host "`nResponse:" -ForegroundColor Green
    $response | ConvertTo-Json -Depth 10
} catch {
    Write-Host "ERROR executing query: $_" -ForegroundColor Red
}

# 5. Wait a moment for logs to be written
Start-Sleep -Seconds 2

# 6. Display debug logs
Write-Host "`n5. Debug logs from server:" -ForegroundColor Yellow
if (Test-Path $logFile) {
    Write-Host "--- START OF LOGS ---" -ForegroundColor Cyan
    Get-Content $logFile | Select-Object -Last 100
    Write-Host "--- END OF LOGS ---" -ForegroundColor Cyan
} else {
    Write-Host "No log file found yet" -ForegroundColor Yellow
}

# 7. Execute a simple MATCH to compare
Write-Host "`n6. Executing simple MATCH for comparison..." -ForegroundColor Yellow
$matchQuery = "MATCH (d:Document) RETURN d LIMIT 5"
Write-Host "Query: $matchQuery" -ForegroundColor Cyan

try {
    $matchResponse = Invoke-RestMethod -Uri "http://localhost:7878/api/cypher" -Method POST -Body (@{
        query = $matchQuery
    } | ConvertTo-Json) -ContentType "application/json" -TimeoutSec 10
    
    Write-Host "`nMatch Response:" -ForegroundColor Green
    Write-Host "Rows returned: $($matchResponse.data.length)" -ForegroundColor Cyan
} catch {
    Write-Host "ERROR executing MATCH: $_" -ForegroundColor Red
}

# 8. Keep server running for manual inspection
Write-Host "`n7. Server is still running (Job ID: $($serverJob.Id))" -ForegroundColor Cyan
Write-Host "To view live logs: Get-Content '$logFile' -Wait -Tail 50" -ForegroundColor Yellow
Write-Host "To stop server: Stop-Job -Id $($serverJob.Id); Remove-Job -Id $($serverJob.Id)" -ForegroundColor Yellow
Write-Host "`nPress Enter to stop server and exit..." -ForegroundColor Yellow
Read-Host

Stop-Job -Job $serverJob
Remove-Job -Job $serverJob
Write-Host "Server stopped" -ForegroundColor Green






