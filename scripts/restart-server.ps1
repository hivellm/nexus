param(
    [switch]$CleanCatalog
)

Write-Host "=== Restarting Nexus Server ===" -ForegroundColor Cyan
Write-Host ""

Write-Host "Stopping old server..." -ForegroundColor Yellow
try {
    wsl -d Ubuntu-24.04 -- bash -l -c "pkill -f nexus-server" | Out-Null
    Write-Host "  Linux processes terminated" -ForegroundColor Gray
} catch {
    Write-Host "  Unable to terminate Linux processes via pkill (may already be stopped)" -ForegroundColor DarkYellow
}

Get-Process -Name nexus-server -ErrorAction SilentlyContinue | ForEach-Object {
    try {
        $_ | Stop-Process -Force -ErrorAction Stop
        Write-Host "  Windows proxy process $($_.Id) terminated" -ForegroundColor Gray
    } catch {
        Write-Host "  Warning: could not stop Windows process $($_.Id): $_" -ForegroundColor DarkYellow
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
$startCommand = "cd /mnt/f/Node/hivellm/nexus && env NEXUS_DATA_DIR=/mnt/f/Node/hivellm/nexus/data RUST_LOG=debug ./target/release/nexus-server"
Start-Process -FilePath "wsl" -ArgumentList "-d", "Ubuntu-24.04", "--", "bash", "-l", "-c", $startCommand -WindowStyle Hidden

Write-Host "Waiting for server to start..." -ForegroundColor Yellow
Start-Sleep -Seconds 5

try {
    $response = Invoke-RestMethod -Uri "http://localhost:15474/stats" -Method Get -ErrorAction Stop
    Write-Host "✅ Server is running!" -ForegroundColor Green
    Write-Host ""
    Write-Host "Stats:" -ForegroundColor Cyan
    Write-Host "  Nodes: $($response.catalog.node_count)" -ForegroundColor White
    Write-Host "  Relationships: $($response.catalog.rel_count)" -ForegroundColor White
    Write-Host "  Labels: $($response.catalog.label_count)" -ForegroundColor White
    Write-Host "  Rel Types: $($response.catalog.rel_type_count)" -ForegroundColor White
} catch {
    Write-Host "❌ Server failed to start" -ForegroundColor Red
    Write-Host "Error: $_" -ForegroundColor Red
}

Write-Host ""
Write-Host "=== Server Restart Complete ===" -ForegroundColor Cyan
