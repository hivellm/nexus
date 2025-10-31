$NexusUri = "http://localhost:15474"

Write-Host "`n[CHECK] Testing if server has latest changes..." -ForegroundColor Cyan

# Test 1: Check health endpoint
try {
    $health = Invoke-RestMethod -Uri "$NexusUri/health" -Method GET
    Write-Host "  Server version: $($health.version)" -ForegroundColor Yellow
} catch {
    Write-Host "  Health check failed" -ForegroundColor Red
}

# Test 2: Send a simple DETACH DELETE to trigger debug logs
$body = @{ query = "MATCH (n:NonExistent) DETACH DELETE n" } | ConvertTo-Json
try {
    $result = Invoke-RestMethod -Uri "$NexusUri/cypher" -Method POST -Headers @{"Content-Type" = "application/json"} -Body $body
    Write-Host "  Query executed: columns=$($result.columns.Count), rows=$($result.rows.Count)" -ForegroundColor Yellow
    
    if ($result.columns.Count -eq 0 -and $result.rows.Count -eq 0) {
        Write-Host "  ✅ DELETE is working correctly!" -ForegroundColor Green
    } else {
        Write-Host "  ❌ DELETE still returns data (old binary?)" -ForegroundColor Red
    }
} catch {
    Write-Host "  Query failed: $_" -ForegroundColor Red
}

Write-Host "`nNote: Check server stderr logs for [DEBUG PARSER] and [DEBUG LIB] messages" -ForegroundColor Cyan

