$nexusUrl = "http://127.0.0.1:15474"

Write-Host "=== Detailed COUNT Debug ===" -ForegroundColor Cyan

# Test 1: Check if server is alive
Write-Host "`n1. Server Health Check:" -ForegroundColor Yellow
try {
    $health = Invoke-RestMethod -Uri "$nexusUrl/health" -TimeoutSec 2
    Write-Host "   Server: Online" -ForegroundColor Green
} catch {
    Write-Host "   Server: OFFLINE - $_" -ForegroundColor Red
    exit 1
}

# Test 2: Check database stats
Write-Host "`n2. Database Stats:" -ForegroundColor Yellow
try {
    $stats = Invoke-RestMethod -Uri "$nexusUrl/stats" -TimeoutSec 2
    Write-Host "   Nodes: $($stats.catalog.node_count)" -ForegroundColor $(if ($stats.catalog.node_count -gt 0) { "Green" } else { "Red" })
    Write-Host "   Relationships: $($stats.catalog.rel_count)" -ForegroundColor $(if ($stats.catalog.rel_count -gt 0) { "Green" } else { "Red" })
    Write-Host "   Labels: $($stats.catalog.label_count)" -ForegroundColor Green
} catch {
    Write-Host "   Error: $_" -ForegroundColor Red
}

# Test 3: Simple MATCH without COUNT
Write-Host "`n3. MATCH (d:Document) RETURN d LIMIT 3:" -ForegroundColor Yellow
try {
    $body = @{ query = "MATCH (d:Document) RETURN d LIMIT 3" } | ConvertTo-Json
    $res = Invoke-RestMethod -Uri "$nexusUrl/cypher" -Method POST -Body $body -ContentType "application/json" -TimeoutSec 5
    Write-Host "   Rows returned: $($res.rows.Count)" -ForegroundColor $(if ($res.rows.Count -gt 0) { "Green" } else { "Red" })
    if ($res.rows.Count -gt 0) {
        Write-Host "   First row keys: $($res.rows[0].PSObject.Properties.Name -join ', ')" -ForegroundColor Gray
    }
} catch {
    Write-Host "   Error: $_" -ForegroundColor Red
}

# Test 4: COUNT query
Write-Host "`n4. MATCH (d:Document) RETURN count(d) AS total:" -ForegroundColor Yellow
try {
    $body = @{ query = "MATCH (d:Document) RETURN count(d) AS total" } | ConvertTo-Json
    $res = Invoke-RestMethod -Uri "$nexusUrl/cypher" -Method POST -Body $body -ContentType "application/json" -TimeoutSec 5
    Write-Host "   Columns: $($res.columns -join ', ')" -ForegroundColor Gray
    Write-Host "   Rows: $($res.rows.Count)" -ForegroundColor Gray
    if ($res.rows.Count -gt 0) {
        $count = $res.rows[0]
        Write-Host "   Count result: $count" -ForegroundColor $(if ($count -gt 0) { "Green" } else { "Red" })
    }
} catch {
    Write-Host "   Error: $_" -ForegroundColor Red
}

# Test 5: Try COUNT(*) instead
Write-Host "`n5. MATCH (d:Document) RETURN count(*) AS total:" -ForegroundColor Yellow
try {
    $body = @{ query = "MATCH (d:Document) RETURN count(*) AS total" } | ConvertTo-Json
    $res = Invoke-RestMethod -Uri "$nexusUrl/cypher" -Method POST -Body $body -ContentType "application/json" -TimeoutSec 5
    Write-Host "   Rows: $($res.rows.Count)" -ForegroundColor Gray
    if ($res.rows.Count -gt 0) {
        $count = $res.rows[0]
        Write-Host "   Count(*) result: $count" -ForegroundColor $(if ($count -gt 0) { "Green" } else { "Red" })
    }
} catch {
    Write-Host "   Error: $_" -ForegroundColor Red
}

Write-Host ""






