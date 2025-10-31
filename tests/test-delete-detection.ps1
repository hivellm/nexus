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

Write-Host "`n[TEST] Testing if DELETE is detected..." -ForegroundColor Cyan

# Test different DELETE variations
$queries = @(
    "MATCH (n) RETURN n",
    "MATCH (n) DELETE n",
    "MATCH (n) DETACH DELETE n",
    "DELETE n"  # This should fail
)

foreach ($query in $queries) {
    Write-Host "`nTesting: $query" -ForegroundColor Yellow
    $result = Invoke-NexusQuery -Cypher $query

    if ($result) {
        Write-Host "  Success: $($result.columns.Count) columns, $($result.rows.Count) rows"
        if ($result.rows.Count -gt 0) {
            Write-Host "  First row type: $($result.rows[0].GetType().Name)"
        }
    } else {
        Write-Host "  Failed to execute"
    }
}
