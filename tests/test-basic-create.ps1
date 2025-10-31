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

Write-Host "`n[CLEAN] Cleaning database..." -ForegroundColor Cyan
Invoke-NexusQuery -Cypher "MATCH (n) DETACH DELETE n" | Out-Null
Write-Host "Done" -ForegroundColor Green

Write-Host "`n[CREATE] Creating basic nodes..." -ForegroundColor Cyan

$queries = @(
    "CREATE (p:Person {name: 'Alice', age: 30})",
    "CREATE (p:Person {name: 'Bob', age: 25})",
    "CREATE (p:Person {name: 'Charlie', age: 35})"
)

foreach ($query in $queries) {
    Write-Host "  $query"
    Invoke-NexusQuery -Cypher $query | Out-Null
}

Write-Host "`n[COUNT] Counting nodes:" -ForegroundColor Cyan
$result = Invoke-NexusQuery -Cypher "MATCH (n) RETURN count(*) AS count"
$count = if ($result.rows[0] -is [array]) { $result.rows[0][0] } else { 0 }
Write-Host "  Total nodes: $count" -ForegroundColor Yellow

Write-Host "`n[LIST] Listing all nodes:" -ForegroundColor Cyan
$result = Invoke-NexusQuery -Cypher "MATCH (n) RETURN n.name, n.age, labels(n)"
foreach ($row in $result.rows) {
    if ($row -is [array]) {
        Write-Host "  Name: $($row[0]), Age: $($row[1]), Labels: $($row[2])"
    }
}
