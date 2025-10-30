# Test Relationship Queries in Nexus
# Comprehensive test suite for relationship handling

$baseUrl = "http://localhost:15474"
$queries = @()

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
if ($result) {
    Write-Host "Total nodes: $($result.results[0].data[0].row[0])" -ForegroundColor Green
}
Write-Host ""

# Test 2: Count all relationships (no type filter)
Write-Host "Test 2: Count all relationships (any type)" -ForegroundColor Yellow
$result = Invoke-CypherQuery "MATCH ()-[r]->() RETURN count(r) AS total"
if ($result) {
    Write-Host "Total relationships: $($result.results[0].data[0].row[0])" -ForegroundColor Green
}
Write-Host ""

# Test 3: Get distinct relationship types
Write-Host "Test 3: Get distinct relationship types" -ForegroundColor Yellow
$result = Invoke-CypherQuery "MATCH ()-[r]->() RETURN DISTINCT type(r) AS relType, count(r) AS count"
if ($result -and $result.results[0].data) {
    foreach ($row in $result.results[0].data) {
        Write-Host "  - $($row.row[0]): $($row.row[1])" -ForegroundColor Cyan
    }
} else {
    Write-Host "  No relationships found or query failed" -ForegroundColor Red
}
Write-Host ""

# Test 4: Count MENTIONS relationships specifically
Write-Host "Test 4: Count MENTIONS relationships" -ForegroundColor Yellow
$result = Invoke-CypherQuery "MATCH ()-[r:MENTIONS]->() RETURN count(r) AS total"
if ($result) {
    Write-Host "MENTIONS relationships: $($result.results[0].data[0].row[0])" -ForegroundColor Green
}
Write-Host ""

# Test 5: Get sample relationships with node labels
Write-Host "Test 5: Sample relationships with node labels (LIMIT 5)" -ForegroundColor Yellow
$result = Invoke-CypherQuery "MATCH (a)-[r]->(b) RETURN labels(a) AS source_labels, type(r) AS rel_type, labels(b) AS target_labels LIMIT 5"
if ($result -and $result.results[0].data) {
    foreach ($row in $result.results[0].data) {
        $sourceLabels = $row.row[0] -join ","
        $relType = $row.row[1]
        $targetLabels = $row.row[2] -join ","
        Write-Host "  [$sourceLabels] -[:$relType]-> [$targetLabels]" -ForegroundColor Cyan
    }
} else {
    Write-Host "  No relationships found" -ForegroundColor Red
}
Write-Host ""

# Test 6: Bidirectional query (undirected)
Write-Host "Test 6: Bidirectional relationships (undirected)" -ForegroundColor Yellow
$result = Invoke-CypherQuery "MATCH (a)-[r]-(b) RETURN count(r) AS total"
if ($result) {
    Write-Host "Bidirectional count: $($result.results[0].data[0].row[0])" -ForegroundColor Green
}
Write-Host ""

# Test 7: Relationship properties
Write-Host "Test 7: Relationship properties (if any exist)" -ForegroundColor Yellow
$result = Invoke-CypherQuery "MATCH ()-[r]->() RETURN r LIMIT 5"
if ($result -and $result.results[0].data) {
    Write-Host "Sample relationships with properties:" -ForegroundColor Cyan
    foreach ($row in $result.results[0].data) {
        Write-Host "  Relationship: $($row.row[0] | ConvertTo-Json -Compress)" -ForegroundColor Gray
    }
} else {
    Write-Host "  No relationships found" -ForegroundColor Red
}
Write-Host ""

# Test 8: Specific pattern - Document MENTIONS Entity
Write-Host "Test 8: Document MENTIONS pattern" -ForegroundColor Yellow
$result = Invoke-CypherQuery "MATCH (d:Document)-[r:MENTIONS]->(e) RETURN count(r) AS total"
if ($result) {
    Write-Host "Document->MENTIONS->* : $($result.results[0].data[0].row[0])" -ForegroundColor Green
}
Write-Host ""

# Test 9: Get actual relationship data (not just count)
Write-Host "Test 9: Get actual relationship data (LIMIT 3)" -ForegroundColor Yellow
$result = Invoke-CypherQuery "MATCH (a)-[r:MENTIONS]->(b) RETURN a.name AS source, type(r) AS rel, b.name AS target LIMIT 3"
if ($result -and $result.results[0].data) {
    foreach ($row in $result.results[0].data) {
        Write-Host "  $($row.row[0]) -[:$($row.row[1])]-> $($row.row[2])" -ForegroundColor Cyan
    }
} else {
    Write-Host "  No relationship data found" -ForegroundColor Red
}
Write-Host ""

# Test 10: Check stats endpoint
Write-Host "Test 10: Database statistics (from /stats endpoint)" -ForegroundColor Yellow
try {
    $stats = Invoke-RestMethod -Uri "$baseUrl/stats" -Method Get
    Write-Host "  Nodes: $($stats.node_count)" -ForegroundColor Green
    Write-Host "  Relationships: $($stats.relationship_count)" -ForegroundColor Green
    Write-Host "  Labels: $($stats.label_count)" -ForegroundColor Green
    Write-Host "  Types: $($stats.type_count)" -ForegroundColor Green
} catch {
    Write-Host "  Stats endpoint unavailable" -ForegroundColor Red
}
Write-Host ""

Write-Host "=== Test Complete ===" -ForegroundColor Cyan
Write-Host ""
Write-Host "Summary:" -ForegroundColor Yellow
Write-Host "If counts show relationships in stats but MATCH queries return 0:"
Write-Host "  -> Issue is in query execution (Expand operator or planner)" -ForegroundColor Red
Write-Host "If stats also shows 0 relationships:"
Write-Host "  -> Issue is in data import or CREATE/MERGE handling" -ForegroundColor Red
Write-Host "If both show matching non-zero counts:"
Write-Host "  -> System is working correctly! ✅" -ForegroundColor Green
