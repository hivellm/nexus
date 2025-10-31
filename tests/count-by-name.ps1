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

Write-Host "`n[COUNT] Counting nodes by name:`n" -ForegroundColor Cyan

$names = @("Alice", "Bob", "Charlie", "David", "Acme Inc")

foreach ($name in $names) {
    $result = Invoke-NexusQuery -Cypher "MATCH (n {name: '$name'}) RETURN count(*) AS count"
    $count = if ($result.rows[0] -is [array]) { $result.rows[0][0] } else { 0 }
    Write-Host "  $name : $count" -ForegroundColor $(if ($count -eq 1) { "Green" } else { "Red" })
}

Write-Host "`nTotal:" -ForegroundColor Cyan
$total = Invoke-NexusQuery -Cypher "MATCH (n) RETURN count(*) AS count"
$count = if ($total.rows[0] -is [array]) { $total.rows[0][0] } else { 0 }
Write-Host "  All nodes: $count" -ForegroundColor Yellow

