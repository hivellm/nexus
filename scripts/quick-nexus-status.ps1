$nexusUrl = "http://127.0.0.1:15474"

Write-Host "=== Nexus Quick Status ===" -ForegroundColor Cyan

# Health check
Write-Host "`nHealth Check:" -ForegroundColor Yellow
try {
    $health = Invoke-RestMethod -Uri "$nexusUrl/health" -TimeoutSec 2
    Write-Host "  Status: Online" -ForegroundColor Green
    Write-Host "  Response: $($health | ConvertTo-Json -Compress)"
} catch {
    Write-Host "  Status: OFFLINE or Error" -ForegroundColor Red
    Write-Host "  Error: $_"
    exit 1
}

# Stats check
Write-Host "`nDatabase Stats:" -ForegroundColor Yellow
try {
    $stats = Invoke-RestMethod -Uri "$nexusUrl/stats" -TimeoutSec 5
    Write-Host "  Stats:" -ForegroundColor Green
    $stats | ConvertTo-Json -Depth 5 | Write-Host
} catch {
    Write-Host "  Stats endpoint error: $_" -ForegroundColor Red
}

# Quick count test
Write-Host "`nQuick Count Test:" -ForegroundColor Yellow
$testQuery = "MATCH (d:Document) RETURN count(d) AS total"
try {
    $body = @{ query = $testQuery } | ConvertTo-Json
    $res = Invoke-RestMethod -Uri "$nexusUrl/cypher" -Method POST -Body $body -ContentType "application/json" -TimeoutSec 5
    
    Write-Host "  Query: $testQuery" -ForegroundColor Gray
    if ($res.rows -and $res.rows.Count -gt 0) {
        $count = $res.rows[0]
        Write-Host "  Document count: $count" -ForegroundColor $(if ($count -gt 0) { "Green" } else { "Yellow" })
        if ($count -eq 0) {
            Write-Host "  WARNING: Database appears empty. You may need to run import script." -ForegroundColor Red
        }
    } else {
        Write-Host "  No results returned" -ForegroundColor Yellow
    }
} catch {
    Write-Host "  Query error: $_" -ForegroundColor Red
}

Write-Host ""






