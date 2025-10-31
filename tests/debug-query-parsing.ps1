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

Write-Host "`n[TEST] Testing different DELETE queries..." -ForegroundColor Cyan

$queries = @(
    "MATCH (n) RETURN n",
    "MATCH (n) DETACH DELETE n"
)

foreach ($query in $queries) {
    Write-Host "`nTesting: $query" -ForegroundColor Yellow
    $result = Invoke-NexusQuery -Cypher $query

    if ($result) {
        Write-Host "  Columns: $($result.columns -join ', ')"
        Write-Host "  Rows: $($result.rows.Count)"
        if ($result.rows.Count -gt 0) {
            Write-Host "  First row: $($result.rows[0] | ConvertTo-Json -Compress)"
        }
    } else {
        Write-Host "  No result returned"
    }
}
