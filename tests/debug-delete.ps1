$NexusUri = "http://localhost:15474"

function Invoke-NexusQuery {
    param([string]$Cypher)
    $body = @{ query = $Cypher } | ConvertTo-Json
    try {
        return Invoke-RestMethod -Uri "$NexusUri/cypher" -Method POST -Headers @{"Content-Type" = "application/json"} -Body $body
    } catch {
        return $null
    }
}

Write-Host "`n[BEFORE] Nodes before DELETE:" -ForegroundColor Yellow
$result = Invoke-NexusQuery -Cypher "MATCH (n) RETURN count(*) AS count"
$count = if ($result.rows[0] -is [array]) { $result.rows[0][0] } else { 0 }
Write-Host "  Total nodes: $count"

Write-Host "`n[DELETE] Executing MATCH (n) DETACH DELETE n..." -ForegroundColor Cyan
$result = Invoke-NexusQuery -Cypher "MATCH (n) DETACH DELETE n"
Write-Host "  Result: $($result | ConvertTo-Json -Depth 1)"

Write-Host "`n[AFTER] Nodes after DELETE:" -ForegroundColor Green
$result = Invoke-NexusQuery -Cypher "MATCH (n) RETURN count(*) AS count"
$count = if ($result.rows[0] -is [array]) { $result.rows[0][0] } else { 0 }
Write-Host "  Total nodes: $count"

Write-Host "`n[DETAIL] Checking if any nodes remain:" -ForegroundColor Cyan
$result = Invoke-NexusQuery -Cypher "MATCH (n) RETURN n.name, n.age, labels(n) LIMIT 5"
if ($result.rows.Count -gt 0) {
    foreach ($row in $result.rows) {
        if ($row -is [array]) {
            Write-Host "  REMAINING: Name='$($row[0])', Age='$($row[1])', Labels='$($row[2])'"
        }
    }
} else {
    Write-Host "  No nodes remaining"
}
