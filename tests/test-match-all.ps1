# Test MATCH (n) to see what nodes it returns

$NexusUri = "http://localhost:15474"

function Invoke-NexusQuery {
    param([string]$Cypher)
    $body = @{ query = $Cypher } | ConvertTo-Json
    try {
        return Invoke-RestMethod -Uri "$NexusUri/cypher" -Method POST -Headers @{"Content-Type" = "application/json"} -Body $body
    } catch {
        Write-Host "[ERROR] $_" -ForegroundColor Red
        return $null
    }
}

Write-Host "`n[TEST] MATCH (n) returns which nodes?`n" -ForegroundColor Cyan

$result = Invoke-NexusQuery -Cypher "MATCH (n) RETURN id(n) AS id, labels(n) AS labels"

Write-Host "MATCH (n) returned: $($result.rows.Count) nodes" -ForegroundColor Yellow

foreach ($row in $result.rows) {
    $id = if ($row -is [array]) { $row[0] } else { $row.values[0] }
    $labels = if ($row -is [array]) { $row[1] } else { $row.values[1] }
    Write-Host "  ID: $id | Labels: $labels"
}

Write-Host "`nNow testing if DELETE receives these nodes..." -ForegroundColor Cyan
Invoke-NexusQuery -Cypher "MATCH (n) DETACH DELETE n" | Out-Null

$result2 = Invoke-NexusQuery -Cypher "MATCH (n) RETURN count(*) AS count"
$count = if ($result2.rows[0] -is [array]) { $result2.rows[0][0] } else { 0 }

Write-Host "`nAfter DELETE:" -ForegroundColor Yellow
Write-Host "  Remaining nodes: $count" -ForegroundColor $(if ($count -eq 0) { "Green" } else { "Red" })

