# Test Relationship Queries in Nexus (Updated for new API format)
$baseUrl = "http://localhost:15474"

Write-Host "=== Testing Relationship Queries in Nexus ===" -ForegroundColor Cyan
Write-Host ""

function Invoke-CypherQuery {
    param([string]$query)
    
    $body = @{ query = $query } | ConvertTo-Json
    
    try {
        $response = Invoke-RestMethod -Uri "$baseUrl/cypher" -Method Post -Body $body -ContentType "application/json"
        return $response
    } catch {
        Write-Host "Error: $_" -ForegroundColor Red
        return $null
    }
}

# Test 1: Count all nodes
Write-Host "Test 1: Count all nodes" -ForegroundColor Yellow
$result = Invoke-CypherQuery "MATCH (n) RETURN count(n) AS total"
if ($result -and $result.rows) {
    Write-Host "Total nodes: $($result.rows[0][0])" -ForegroundColor Green
}
Write-Host ""

# Test 2: Count all relationships (no type filter)
Write-Host "Test 2: Count all relationships (any type)" -ForegroundColor Yellow
$result = Invoke-CypherQuery "MATCH ()-[r]->() RETURN count(r) AS total"
if ($result -and $result.rows) {
    Write-Host "Total relationships: $($result.rows[0][0])" -ForegroundColor Green
}
Write-Host ""

# Test 3: Get distinct relationship types
Write-Host "Test 3: Get distinct relationship types" -ForegroundColor Yellow
$result = Invoke-CypherQuery "MATCH ()-[r]->() RETURN DISTINCT type(r) AS relType, count(r) AS count"
if ($result -and $result.rows -and $result.rows.Count -gt 0) {
    foreach ($row in $result.rows) {
        Write-Host "  - $($row[0]): $($row[1])" -ForegroundColor Cyan
    }
} else {
    Write-Host "  No relationships found" -ForegroundColor Red
}
Write-Host ""

# Test 4: Count MENTIONS relationships specifically
Write-Host "Test 4: Count MENTIONS relationships" -ForegroundColor Yellow
$result = Invoke-CypherQuery "MATCH ()-[r:MENTIONS]->() RETURN count(r) AS total"
if ($result -and $result.rows) {
    Write-Host "MENTIONS relationships: $($result.rows[0][0])" -ForegroundColor Green
}
Write-Host ""

# Test 5: Get sample relationships with node labels
Write-Host "Test 5: Sample relationships with node labels (LIMIT 5)" -ForegroundColor Yellow
$result = Invoke-CypherQuery "MATCH (a)-[r]->(b) RETURN labels(a) AS source_labels, type(r) AS rel_type, labels(b) AS target_labels LIMIT 5"
if ($result -and $result.rows -and $result.rows.Count -gt 0) {
    foreach ($row in $result.rows) {
        $sourceLabels = $row[0] -join ","
        $relType = $row[1]
        $targetLabels = $row[2] -join ","
        Write-Host "  [$sourceLabels] -[:$relType]-> [$targetLabels]" -ForegroundColor Cyan
    }
} else {
    Write-Host "  No relationships found" -ForegroundColor Red
}
Write-Host ""

# Test 6: Stats endpoint
Write-Host "Test 6: Database statistics (from /stats endpoint)" -ForegroundColor Yellow
try {
    $stats = Invoke-RestMethod -Uri "$baseUrl/stats" -Method Get
    Write-Host "  Nodes: $($stats.catalog.node_count)" -ForegroundColor Green
    Write-Host "  Relationships: $($stats.catalog.rel_count)" -ForegroundColor Green
    Write-Host "  Labels: $($stats.catalog.label_count)" -ForegroundColor Green
    Write-Host "  Types: $($stats.catalog.rel_type_count)" -ForegroundColor Green
} catch {
    Write-Host "  Stats endpoint unavailable" -ForegroundColor Red
}
Write-Host ""

Write-Host "=== Test Complete ===" -ForegroundColor Cyan

