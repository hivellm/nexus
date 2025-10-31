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

Write-Host "`n[TEST] Testing DETACH DELETE without MATCH..." -ForegroundColor Cyan

# Create test data first
Write-Host "`nCreating test data..."
Invoke-NexusQuery -Cypher "CREATE (n:Test {name: 'test'})" | Out-Null

Write-Host "`nTesting DETACH DELETE n (without MATCH):"
try {
    $result = Invoke-NexusQuery -Cypher "DETACH DELETE n"
    Write-Host "Result: $($result | ConvertTo-Json -Depth 2)"
} catch {
    Write-Host "Error: $_" -ForegroundColor Red
}

Write-Host "`nChecking if node was deleted:"
$result = Invoke-NexusQuery -Cypher "MATCH (n:Test) RETURN count(*) AS count"
$count = if ($result.rows[0] -is [array]) { $result.rows[0][0] } else { 0 }
Write-Host "  Remaining nodes: $count"
