$nexusUrl = "http://127.0.0.1:15474"

Write-Host "=== Simple COUNT Test ===" -ForegroundColor Cyan

$query = "MATCH (d:Document) RETURN count(d) AS total"
Write-Host "Query: $query`n" -ForegroundColor Yellow

try {
    $body = @{ query = $query } | ConvertTo-Json
    $res = Invoke-RestMethod -Uri "$nexusUrl/cypher" -Method POST -Body $body -ContentType "application/json"
    
    Write-Host "Response:" -ForegroundColor Green
    Write-Host "  Columns: $($res.columns -join ', ')"
    Write-Host "  Rows: $($res.rows.Count)"
    
    if ($res.rows.Count -gt 0) {
        $firstRow = $res.rows[0]
        Write-Host "  First row type: $($firstRow.GetType().Name)"
        Write-Host "  First row value: $firstRow"
        
        if ($firstRow -is [Array] -and $firstRow.Count -gt 0) {
            Write-Host "  First row[0]: $($firstRow[0])"
            Write-Host "  First row[0] type: $($firstRow[0].GetType().Name)"
        }
    }
    
    # Also test simple MATCH to verify data exists
    Write-Host "`nVerifying data exists:" -ForegroundColor Yellow
    $body2 = @{ query = "MATCH (d:Document) RETURN d LIMIT 1" } | ConvertTo-Json
    $res2 = Invoke-RestMethod -Uri "$nexusUrl/cypher" -Method POST -Body $body2 -ContentType "application/json"
    Write-Host "  MATCH returned: $($res2.rows.Count) rows"
    
} catch {
    Write-Host "ERROR: $_" -ForegroundColor Red
    Write-Host $_.Exception | Format-List | Out-String
}

Write-Host ""






