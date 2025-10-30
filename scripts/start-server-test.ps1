# Start Nexus Server and Test
Write-Host "=== Starting Nexus Server ===" -ForegroundColor Cyan

# Kill existing processes
Write-Host "Stopping existing servers..." -ForegroundColor Yellow
wsl -d Ubuntu-24.04 -- bash -l -c "pkill -9 nexus-server"
Start-Sleep -Seconds 2

# Start server in background via WSL
Write-Host "Starting new server..." -ForegroundColor Green
$job = Start-Job -ScriptBlock {
    wsl -d Ubuntu-24.04 -- bash -l -c "cd /mnt/f/Node/hivellm/nexus && ./target/release/nexus-server"
}

# Wait for server to start
Write-Host "Waiting for server to start..." -ForegroundColor Yellow
Start-Sleep -Seconds 5

# Test if server is responding
Write-Host "Testing server..." -ForegroundColor Cyan
try {
    $stats = Invoke-RestMethod -Uri "http://localhost:15474/stats" -Method Get -TimeoutSec 5
    Write-Host "✅ Server is running!" -ForegroundColor Green
    Write-Host ""
    Write-Host "Stats:" -ForegroundColor Cyan
    Write-Host "  Nodes: $($stats.catalog.node_count)" -ForegroundColor White
    Write-Host "  Relationships: $($stats.catalog.rel_count)" -ForegroundColor White
    Write-Host "  Labels: $($stats.catalog.label_count)" -ForegroundColor White
    Write-Host "  Rel Types: $($stats.catalog.rel_type_count)" -ForegroundColor White
    Write-Host ""
    Write-Host "Server job ID: $($job.Id)" -ForegroundColor Gray
} catch {
    Write-Host "❌ Server failed to start or not responding" -ForegroundColor Red
    Write-Host "Error: $_" -ForegroundColor Red
    Stop-Job $job
}

Write-Host "=== Server Started ===" -ForegroundColor Cyan

