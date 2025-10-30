# Test Relationship Queries ONE AT A TIME
# This script tests each query individually with delays to avoid overwhelming the server

$baseUrl = "http://localhost:15474"

Write-Host "=== Testing Relationship Queries (Slow Mode) ===" -ForegroundColor Cyan
Write-Host ""

function Invoke-CypherQuery {
    param([string]$query, [string]$testName)
    
    Write-Host "$testName" -ForegroundColor Yellow
    
    $body = @{ query = $query } | ConvertTo-Json
    
    try {
        $response = Invoke-RestMethod -Uri "$baseUrl/cypher" -Method Post -Body $body -ContentType "application/json" -TimeoutSec 10
        
        if ($response -and $response.rows) {
            return $response
        } else {
            Write-Host "  No data returned" -ForegroundColor Gray
            return $null
        }
    } catch {
        Write-Host "  Error: $($_.Exception.Message)" -ForegroundColor Red
        return $null
    }
}

# Test 1: Count all nodes
$result = Invoke-CypherQuery "MATCH (n) RETURN count(n) AS total" "Test 1: Count all nodes"
if ($result -and $result.rows) {
    Write-Host "  Result: $($result.rows[0][0]) nodes" -ForegroundColor Green
}
Write-Host ""
Start-Sleep -Seconds 1

# Test 2: Count all relationships
$result = Invoke-CypherQuery "MATCH ()-[r]->() RETURN count(r) AS total" "Test 2: Count all relationships"
if ($result -and $result.rows) {
    Write-Host "  Result: $($result.rows[0][0]) relationships" -ForegroundColor Green
}
Write-Host ""
Start-Sleep -Seconds 1

# Test 3: Get distinct relationship types
$result = Invoke-CypherQuery "MATCH ()-[r]->() RETURN DISTINCT type(r) AS relType LIMIT 10" "Test 3: Get distinct relationship types (LIMIT 10)"
if ($result -and $result.rows -and $result.rows.Count -gt 0) {
    Write-Host "  Found $($result.rows.Count) relationship types:" -ForegroundColor Green
    foreach ($row in $result.rows) {
        Write-Host "    - $($row[0])" -ForegroundColor Cyan
    }
} else {
    Write-Host "  No relationship types found" -ForegroundColor Red
}
Write-Host ""
Start-Sleep -Seconds 1

# Test 4: Count MENTIONS relationships
$result = Invoke-CypherQuery "MATCH ()-[r:MENTIONS]->() RETURN count(r) AS total" "Test 4: Count MENTIONS relationships"
if ($result -and $result.rows) {
    Write-Host "  Result: $($result.rows[0][0]) MENTIONS relationships" -ForegroundColor Green
}
Write-Host ""
Start-Sleep -Seconds 1

# Test 5: Get sample relationships
$result = Invoke-CypherQuery "MATCH (a)-[r]->(b) RETURN id(a), type(r), id(b) LIMIT 5" "Test 5: Sample relationships (LIMIT 5)"
if ($result -and $result.rows -and $result.rows.Count -gt 0) {
    Write-Host "  Found $($result.rows.Count) relationships:" -ForegroundColor Green
    foreach ($row in $result.rows) {
        Write-Host "    Node $($row[0]) -[:$($row[1])]-> Node $($row[2])" -ForegroundColor Cyan
    }
} else {
    Write-Host "  No relationships found" -ForegroundColor Red
}
Write-Host ""
Start-Sleep -Seconds 1

# Test 6: Stats endpoint (to verify data consistency)
Write-Host "Test 6: Database statistics" -ForegroundColor Yellow
try {
    $stats = Invoke-RestMethod -Uri "$baseUrl/stats" -Method Get -TimeoutSec 5
    Write-Host "  Nodes: $($stats.catalog.node_count)" -ForegroundColor Green
    Write-Host "  Relationships: $($stats.catalog.rel_count)" -ForegroundColor Green
    Write-Host "  Labels: $($stats.catalog.label_count)" -ForegroundColor Green
    Write-Host "  Rel Types: $($stats.catalog.rel_type_count)" -ForegroundColor Green
} catch {
    Write-Host "  Error: $($_.Exception.Message)" -ForegroundColor Red
}
Write-Host ""

Write-Host "=== Test Complete ===" -ForegroundColor Cyan
Write-Host ""
Write-Host "Summary:" -ForegroundColor Yellow
Write-Host "If relationship queries return 0 but stats shows data:"
Write-Host "  -> Expand operator or planner issue" -ForegroundColor Red
Write-Host "If both show data:"
Write-Host "  -> System working correctly! ✅" -ForegroundColor Green

