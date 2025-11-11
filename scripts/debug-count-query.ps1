$nexusUrl = "http://127.0.0.1:15474"

Write-Host "=== Debug COUNT Query ===" -ForegroundColor Cyan

# Test 1: Simple MATCH without COUNT
Write-Host "`nTest 1: MATCH (d:Document) RETURN d LIMIT 5" -ForegroundColor Yellow
try {
    $body = @{ query = "MATCH (d:Document) RETURN d LIMIT 5" } | ConvertTo-Json
    $res = Invoke-RestMethod -Uri "$nexusUrl/cypher" -Method POST -Body $body -ContentType "application/json"
    Write-Host "  Rows returned: $($res.rows.Count)" -ForegroundColor Green
    if ($res.rows.Count -gt 0) {
        Write-Host "  First row:" -ForegroundColor Gray
        $res.rows[0] | ConvertTo-Json -Depth 3 | Write-Host
    }
} catch {
    Write-Host "  ERROR: $_" -ForegroundColor Red
}

# Test 2: COUNT query
Write-Host "`nTest 2: MATCH (d:Document) RETURN count(d) AS total" -ForegroundColor Yellow
try {
    $body = @{ query = "MATCH (d:Document) RETURN count(d) AS total" } | ConvertTo-Json
    $res = Invoke-RestMethod -Uri "$nexusUrl/cypher" -Method POST -Body $body -ContentType "application/json"
    Write-Host "  Full response:" -ForegroundColor Green
    $res | ConvertTo-Json -Depth 5 | Write-Host
    Write-Host "`n  Parsed:" -ForegroundColor Green
    Write-Host "    Columns: $($res.columns -join ', ')"
    Write-Host "    Rows: $($res.rows.Count)"
    if ($res.rows.Count -gt 0) {
        Write-Host "    First row: $($res.rows[0] | ConvertTo-Json -Compress)"
    }
} catch {
    Write-Host "  ERROR: $_" -ForegroundColor Red
}

# Test 3: COUNT(*) without variable
Write-Host "`nTest 3: MATCH (d:Document) RETURN count(*) AS total" -ForegroundColor Yellow
try {
    $body = @{ query = "MATCH (d:Document) RETURN count(*) AS total" } | ConvertTo-Json
    $res = Invoke-RestMethod -Uri "$nexusUrl/cypher" -Method POST -Body $body -ContentType "application/json"
    Write-Host "  Rows: $($res.rows.Count)" -ForegroundColor Green
    if ($res.rows.Count -gt 0) {
        Write-Host "  First row: $($res.rows[0] | ConvertTo-Json -Compress)"
    }
} catch {
    Write-Host "  ERROR: $_" -ForegroundColor Red
}

Write-Host ""




















