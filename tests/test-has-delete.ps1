$NexusUri = "http://localhost:15474"

function Invoke-NexusQuery {
    param([string]$Cypher)
    $body = @{ query = $Cypher } | ConvertTo-Json
    try {
        return Invoke-RestMethod -Uri "$NexusUri/cypher" -Method POST -Headers @{"Content-Type" = "application/json"} -Body $body
    } catch {
        Write-Host "Error: $_" -ForegroundColor Red
        return $null
    }
}

Write-Host "`n[TEST] Checking if DETACH DELETE is detected as DELETE..." -ForegroundColor Cyan

# Create test data first
Write-Host "`nCreating test data..."
Invoke-NexusQuery -Cypher "CREATE (n:Test {name: 'test'})" | Out-Null

Write-Host "`nTesting DETACH DELETE detection:"
$result = Invoke-NexusQuery -Cypher "MATCH (n:Test) DETACH DELETE n"

Write-Host "Result type check:"
Write-Host "  Columns: $($result.columns.Count)"
Write-Host "  Rows: $($result.rows.Count)"

if ($result.columns.Count -eq 0 -and $result.rows.Count -eq 0) {
    Write-Host "  ✅ DETACH DELETE correctly detected and returns empty result" -ForegroundColor Green
} else {
    Write-Host "  ❌ DETACH DELETE not detected properly - returns data" -ForegroundColor Red
    Write-Host "     Expected: 0 columns, 0 rows"
    Write-Host "     Actual: $($result.columns.Count) columns, $($result.rows.Count) rows"
}
