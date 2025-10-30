$nexusUrl = "http://127.0.0.1:15474"

Write-Host "=== Debug: Relationship Count Query ===" -ForegroundColor Cyan

# Test query
$query = "MATCH ()-[r]->() RETURN count(r) AS total"

Write-Host "`nQuery: $query`n" -ForegroundColor Yellow

# Get raw response
try {
    $body = @{ query = $query } | ConvertTo-Json
    $res = Invoke-RestMethod -Uri "$nexusUrl/cypher" -Method POST -Body $body -ContentType "application/json"
    
    Write-Host "Full Response:" -ForegroundColor Green
    $res | ConvertTo-Json -Depth 10 | Write-Host
    
    Write-Host "`nParsed:" -ForegroundColor Green
    Write-Host "  Columns: $($res.columns -join ', ')"
    Write-Host "  Rows: $($res.rows.Count)"
    if ($res.rows.Count -gt 0) {
        Write-Host "  First row: $($res.rows[0] | ConvertTo-Json -Compress)"
    }
    
} catch {
    Write-Host "ERROR: $_" -ForegroundColor Red
    $_.Exception.Response | Format-List | Out-String | Write-Host
}

# Also test a simpler query to see relationship count directly
Write-Host "`n=== Test: Get all relationships (no count) ===" -ForegroundColor Cyan
$query2 = "MATCH ()-[r]->() RETURN r LIMIT 5"

try {
    $body2 = @{ query = $query2 } | ConvertTo-Json
    $res2 = Invoke-RestMethod -Uri "$nexusUrl/cypher" -Method POST -Body $body2 -ContentType "application/json"
    
    Write-Host "Query: $query2" -ForegroundColor Yellow
    Write-Host "Rows returned: $($res2.rows.Count)" -ForegroundColor Green
    if ($res2.rows.Count -gt 0) {
        Write-Host "First relationship:"
        $res2.rows[0] | ConvertTo-Json -Depth 5 | Write-Host
    }
} catch {
    Write-Host "ERROR: $_" -ForegroundColor Red
}



