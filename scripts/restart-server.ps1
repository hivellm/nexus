param(
    [switch]$CleanCatalog
)

Write-Host "=== Restarting Nexus Server ===" -ForegroundColor Cyan
Write-Host ""

Write-Host "Stopping old server..." -ForegroundColor Yellow
Get-Process -Name nexus-server -ErrorAction SilentlyContinue | ForEach-Object {
    try {
        $_ | Stop-Process -Force -ErrorAction Stop
        Write-Host "  Windows process $($_.Id) terminated" -ForegroundColor Gray
    } catch {
        Write-Host "  Warning: could not stop process $($_.Id): $_" -ForegroundColor DarkYellow
    }
}

Start-Sleep -Seconds 2

if ($CleanCatalog) {
    Write-Host "Cleaning catalog directory as requested..." -ForegroundColor Yellow
    $catalogPath = "F:\Node\hivellm\nexus\data\catalog"
    if (Test-Path $catalogPath) {
        Remove-Item -Path $catalogPath -Recurse -Force -ErrorAction SilentlyContinue
        Write-Host "  Catalog directory removed" -ForegroundColor Gray
    } else {
        Write-Host "  Catalog directory not found" -ForegroundColor DarkYellow
    }
} else {
    Write-Host "Preserving existing catalog data (persistent storage)." -ForegroundColor Green
}

Write-Host "Starting new server..." -ForegroundColor Green
$scriptPath = Join-Path $PSScriptRoot "start-server.sh"
$wslPath = $scriptPath.Replace('\', '/').Replace('F:', '/mnt/f')
$serverPid = wsl -d Ubuntu-24.04 -- bash "$wslPath"
Write-Host "  Server process started (PID: $serverPid)" -ForegroundColor Gray
Write-Host "  Logs available at: /tmp/nexus-server.log" -ForegroundColor Gray

Write-Host "Waiting for server to start..." -ForegroundColor Yellow
$maxWait = 30
$waited = 0
$serverReady = $false

while ($waited -lt $maxWait) {
    Start-Sleep -Seconds 1
    $waited++
    try {
        $response = Invoke-RestMethod -Uri "http://localhost:15474/stats" -Method Get -ErrorAction Stop -TimeoutSec 1
        $serverReady = $true
        break
    } catch {
        Write-Host "  Waiting... ($waited/$maxWait seconds)" -ForegroundColor Gray
    }
}

if ($serverReady) {
    Write-Host "✅ Server is running!" -ForegroundColor Green
    Write-Host ""
    Write-Host "Stats:" -ForegroundColor Cyan
    Write-Host "  Nodes: $($response.catalog.node_count)" -ForegroundColor White
    Write-Host "  Relationships: $($response.catalog.rel_count)" -ForegroundColor White
    Write-Host "  Labels: $($response.catalog.label_count)" -ForegroundColor White
    Write-Host "  Rel Types: $($response.catalog.rel_type_count)" -ForegroundColor White
} else {
    Write-Host "❌ Server failed to start within $maxWait seconds" -ForegroundColor Red
    Write-Host "Check logs: wsl tail -50 /tmp/nexus-server.log" -ForegroundColor Yellow
    exit 1
}

Write-Host ""
Write-Host "=== Server Restart Complete ===" -ForegroundColor Cyan
