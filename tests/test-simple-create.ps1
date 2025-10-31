$NexusUri = "http://localhost:15474"

function Invoke-NexusQuery {
    param([string]$Cypher)
    $body = @{ query = $Cypher } | ConvertTo-Json
    try {
        $result = Invoke-RestMethod -Uri "$NexusUri/cypher" -Method POST -Headers @{"Content-Type" = "application/json"} -Body $body
        return $result
    } catch {
        Write-Host "Error: $_" -ForegroundColor Red
        return $null
    }
}

Write-Host "`n[TEST] Testing simple CREATE..." -ForegroundColor Cyan

Invoke-NexusQuery -Cypher 'MATCH (n) DETACH DELETE n' | Out-Null

$result = Invoke-NexusQuery -Cypher "CREATE (p:Person {name: 'Test'})"
if ($result.error) {
    Write-Host "  Error: $($result.error)" -ForegroundColor Red
} else {
    Write-Host "  Success!" -ForegroundColor Green
}

$count = Invoke-NexusQuery -Cypher 'MATCH (n) RETURN count(*) AS c'
$c = if ($count.rows[0] -is [array]) { $count.rows[0][0] } else { 0 }
Write-Host "  Node count: $c" -ForegroundColor Yellow

