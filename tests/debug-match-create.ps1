# Debug MATCH ... CREATE behavior

$NexusUri = "http://localhost:15474"

function Invoke-NexusQuery {
    param([string]$Cypher)
    
    $body = @{ query = $Cypher } | ConvertTo-Json
    
    try {
        $response = Invoke-RestMethod -Uri "$NexusUri/cypher" `
            -Method POST `
            -Headers @{"Content-Type" = "application/json"} `
            -Body $body `
            -ErrorAction Stop
        
        return $response
    }
    catch {
        Write-Host "[ERROR] $_" -ForegroundColor Red
        return $null
    }
}

Write-Host "[TEST] Testing MATCH ... CREATE behavior`n" -ForegroundColor Cyan

# Step 1: Create two nodes
Write-Host "1. Creating Alice..." -ForegroundColor Yellow
Invoke-NexusQuery -Cypher "CREATE (p:Person {name: 'Alice', age: 30})" | Out-Null

Write-Host "2. Creating Bob..." -ForegroundColor Yellow
Invoke-NexusQuery -Cypher "CREATE (p:Person {name: 'Bob', age: 25})" | Out-Null

# Step 3: Count nodes
Write-Host "`n3. Counting nodes..." -ForegroundColor Yellow
$countResult = Invoke-NexusQuery -Cypher "MATCH (n) RETURN count(*) AS count"
$nodeCount = if ($countResult.rows[0] -is [array]) { $countResult.rows[0][0] } else { $countResult.rows[0].values[0] }
Write-Host "   Nodes: $nodeCount (expected: 2)" -ForegroundColor $(if ($nodeCount -eq 2) { "Green" } else { "Red" })

# Step 4: Test MATCH query
Write-Host "`n4. Testing MATCH query..." -ForegroundColor Yellow
$matchResult = Invoke-NexusQuery -Cypher "MATCH (p1:Person {name: 'Alice'}), (p2:Person {name: 'Bob'}) RETURN p1.name, p2.name"
Write-Host "   MATCH returned $($matchResult.rows.Count) row(s)" -ForegroundColor $(if ($matchResult.rows.Count -eq 1) { "Green" } else { "Red" })

# Step 5: Create relationship
Write-Host "`n5. Creating relationship with MATCH ... CREATE..." -ForegroundColor Yellow
Invoke-NexusQuery -Cypher "MATCH (p1:Person {name: 'Alice'}), (p2:Person {name: 'Bob'}) CREATE (p1)-[:KNOWS {since: 2015}]->(p2)" | Out-Null

# Step 6: Count nodes again
Write-Host "`n6. Counting nodes AFTER relationship creation..." -ForegroundColor Yellow
$countResult2 = Invoke-NexusQuery -Cypher "MATCH (n) RETURN count(*) AS count"
$nodeCount2 = if ($countResult2.rows[0] -is [array]) { $countResult2.rows[0][0] } else { $countResult2.rows[0].values[0] }
Write-Host "   Nodes: $nodeCount2 (expected: 2)" -ForegroundColor $(if ($nodeCount2 -eq 2) { "Green" } else { "Red" })

if ($nodeCount2 -ne 2) {
    Write-Host "   [BUG] Extra $($nodeCount2 - 2) nodes created!" -ForegroundColor Red
}

# Step 7: Count relationships
Write-Host "`n7. Counting relationships..." -ForegroundColor Yellow
$relResult = Invoke-NexusQuery -Cypher "MATCH ()-[r:KNOWS]->() RETURN count(*) AS count"
$relCount = if ($relResult.rows[0] -is [array]) { $relResult.rows[0][0] } else { $relResult.rows[0].values[0] }
Write-Host "   Relationships: $relCount (expected: 1)" -ForegroundColor $(if ($relCount -eq 1) { "Green" } else { "Red" })

# Step 8: List all nodes
Write-Host "`n8. Listing all nodes..." -ForegroundColor Yellow
$allNodes = Invoke-NexusQuery -Cypher "MATCH (n) RETURN labels(n) AS labels, n.name AS name"
foreach ($row in $allNodes.rows) {
    $labels = if ($row -is [array]) { $row[0] } else { $row.values[0] }
    $name = if ($row -is [array]) { $row[1] } else { $row.values[1] }
    Write-Host "   - Labels: $labels, Name: $name"
}

Write-Host "`n[DONE] Debug complete" -ForegroundColor Cyan

