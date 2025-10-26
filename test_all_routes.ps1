# Nexus API - Comprehensive Route Testing Script
# Tests all available REST API endpoints

$baseUrl = "http://localhost:15474"
$passed = 0
$failed = 0
$total = 0

function Test-Endpoint {
    param(
        [string]$Name,
        [string]$Method,
        [string]$Url,
        [string]$Body = $null,
        [bool]$ExpectSuccess = $true
    )
    
    $global:total++
    Write-Host "`n[$global:total] Testing: $Name" -ForegroundColor Cyan
    Write-Host "  Method: $Method $Url" -ForegroundColor Gray
    
    try {
        $headers = @{
            "Content-Type" = "application/json"
        }
        
        if ($Body) {
            Write-Host "  Body: $Body" -ForegroundColor Gray
            $response = Invoke-WebRequest -Uri $Url -Method $Method -Body $Body -Headers $headers -ErrorAction Stop
        } else {
            $response = Invoke-WebRequest -Uri $Url -Method $Method -Headers $headers -ErrorAction Stop
        }
        
        if ($ExpectSuccess) {
            Write-Host "  ✅ PASSED - Status: $($response.StatusCode)" -ForegroundColor Green
            Write-Host "  Response: $($response.Content.Substring(0, [Math]::Min(200, $response.Content.Length)))..." -ForegroundColor Gray
            $global:passed++
        } else {
            Write-Host "  ⚠️  UNEXPECTED SUCCESS - Expected failure but got: $($response.StatusCode)" -ForegroundColor Yellow
            $global:failed++
        }
    }
    catch {
        if (-not $ExpectSuccess) {
            Write-Host "  ✅ PASSED - Failed as expected: $($_.Exception.Message)" -ForegroundColor Green
            $global:passed++
        } else {
            Write-Host "  ❌ FAILED - $($_.Exception.Message)" -ForegroundColor Red
            $global:failed++
        }
    }
}

Write-Host "=======================================================" -ForegroundColor Magenta
Write-Host "  NEXUS API - COMPREHENSIVE ROUTE TESTING" -ForegroundColor Magenta
Write-Host "=======================================================" -ForegroundColor Magenta

# ============================================
# 1. HEALTH & STATUS ENDPOINTS
# ============================================
Write-Host "`n--- 1. HEALTH & STATUS ENDPOINTS ---" -ForegroundColor Yellow

Test-Endpoint -Name "Health Check" -Method "GET" -Url "$baseUrl/health"
Test-Endpoint -Name "Database Statistics" -Method "GET" -Url "$baseUrl/stats"

# ============================================
# 2. SCHEMA ENDPOINTS - Labels
# ============================================
Write-Host "`n━━━ 2. SCHEMA ENDPOINTS - LABELS ━━━" -ForegroundColor Yellow

Test-Endpoint -Name "Create Label - Person" -Method "POST" -Url "$baseUrl/schema/labels" `
    -Body '{"label": "Person"}'

Test-Endpoint -Name "Create Label - Company" -Method "POST" -Url "$baseUrl/schema/labels" `
    -Body '{"label": "Company"}'

Test-Endpoint -Name "Create Label - VIP" -Method "POST" -Url "$baseUrl/schema/labels" `
    -Body '{"label": "VIP"}'

Test-Endpoint -Name "List All Labels" -Method "GET" -Url "$baseUrl/schema/labels"

# ============================================
# 3. SCHEMA ENDPOINTS - Relationship Types
# ============================================
Write-Host "`n━━━ 3. SCHEMA ENDPOINTS - RELATIONSHIP TYPES ━━━" -ForegroundColor Yellow

Test-Endpoint -Name "Create RelType - KNOWS" -Method "POST" -Url "$baseUrl/schema/rel_types" `
    -Body '{"rel_type": "KNOWS"}'

Test-Endpoint -Name "Create RelType - WORKS_AT" -Method "POST" -Url "$baseUrl/schema/rel_types" `
    -Body '{"rel_type": "WORKS_AT"}'

Test-Endpoint -Name "List All Relationship Types" -Method "GET" -Url "$baseUrl/schema/rel_types"

# ============================================
# 4. DATA ENDPOINTS - Create Nodes
# ============================================
Write-Host "`n━━━ 4. DATA ENDPOINTS - CREATE NODES ━━━" -ForegroundColor Yellow

Test-Endpoint -Name "Create Node - Alice" -Method "POST" -Url "$baseUrl/data/nodes" `
    -Body '{"labels": ["Person"], "properties": {"name": "Alice", "age": 30, "city": "NYC"}}'

Test-Endpoint -Name "Create Node - Bob" -Method "POST" -Url "$baseUrl/data/nodes" `
    -Body '{"labels": ["Person"], "properties": {"name": "Bob", "age": 25}}'

Test-Endpoint -Name "Create Node - TechCorp" -Method "POST" -Url "$baseUrl/data/nodes" `
    -Body '{"labels": ["Company"], "properties": {"name": "TechCorp", "industry": "Technology"}}'

# ============================================
# 5. CYPHER QUERIES - READ OPERATIONS
# ============================================
Write-Host "`n━━━ 5. CYPHER QUERIES - READ OPERATIONS ━━━" -ForegroundColor Yellow

Test-Endpoint -Name "Simple MATCH" -Method "POST" -Url "$baseUrl/cypher" `
    -Body '{"query": "MATCH (n:Person) RETURN n"}'

Test-Endpoint -Name "MATCH with WHERE" -Method "POST" -Url "$baseUrl/cypher" `
    -Body '{"query": "MATCH (n:Person) WHERE n.age > 25 RETURN n"}'

Test-Endpoint -Name "MATCH with LIMIT" -Method "POST" -Url "$baseUrl/cypher" `
    -Body '{"query": "MATCH (n:Person) RETURN n LIMIT 5"}'

Test-Endpoint -Name "MATCH with ORDER BY" -Method "POST" -Url "$baseUrl/cypher" `
    -Body '{"query": "MATCH (n:Person) RETURN n ORDER BY n.age DESC"}'

Test-Endpoint -Name "Count Nodes" -Method "POST" -Url "$baseUrl/cypher" `
    -Body '{"query": "MATCH (n:Person) RETURN count(n)"}'

# ============================================
# 6. CYPHER QUERIES - WRITE OPERATIONS (NEW!)
# ============================================
Write-Host "`n━━━ 6. CYPHER QUERIES - WRITE OPERATIONS (NEW!) ━━━" -ForegroundColor Yellow

# MERGE Tests
Test-Endpoint -Name "MERGE - Basic" -Method "POST" -Url "$baseUrl/cypher" `
    -Body '{"query": "MERGE (n:Person {name: \"Charlie\"}) RETURN n"}' `
    -ExpectSuccess $false

Test-Endpoint -Name "MERGE - With ON CREATE" -Method "POST" -Url "$baseUrl/cypher" `
    -Body '{"query": "MERGE (n:Person {name: \"David\"}) ON CREATE SET n.created = true RETURN n"}' `
    -ExpectSuccess $false

Test-Endpoint -Name "MERGE - With ON MATCH" -Method "POST" -Url "$baseUrl/cypher" `
    -Body '{"query": "MERGE (n:Person {name: \"Alice\"}) ON MATCH SET n.updated = true RETURN n"}' `
    -ExpectSuccess $false

Test-Endpoint -Name "MERGE - With ON CREATE and ON MATCH" -Method "POST" -Url "$baseUrl/cypher" `
    -Body '{"query": "MERGE (n:Person {email: \"test@example.com\"}) ON CREATE SET n.created = true ON MATCH SET n.updated = true RETURN n"}' `
    -ExpectSuccess $false

# SET Tests
Test-Endpoint -Name "SET - Property" -Method "POST" -Url "$baseUrl/cypher" `
    -Body '{"query": "MATCH (n:Person {name: \"Alice\"}) SET n.age = 31 RETURN n"}' `
    -ExpectSuccess $false

Test-Endpoint -Name "SET - Multiple Properties" -Method "POST" -Url "$baseUrl/cypher" `
    -Body '{"query": "MATCH (n:Person) SET n.active = true, n.verified = true RETURN n"}' `
    -ExpectSuccess $false

Test-Endpoint -Name "SET - Label" -Method "POST" -Url "$baseUrl/cypher" `
    -Body '{"query": "MATCH (n:Person {name: \"Alice\"}) SET n:VIP RETURN n"}' `
    -ExpectSuccess $false

Test-Endpoint -Name "SET - With Expression" -Method "POST" -Url "$baseUrl/cypher" `
    -Body '{"query": "MATCH (n:Person) SET n.age = n.age + 1 RETURN n"}' `
    -ExpectSuccess $false

# DELETE Tests
Test-Endpoint -Name "DELETE - Node" -Method "POST" -Url "$baseUrl/cypher" `
    -Body '{"query": "MATCH (n:Person {name: \"TestUser\"}) DELETE n"}' `
    -ExpectSuccess $false

Test-Endpoint -Name "DETACH DELETE - Node" -Method "POST" -Url "$baseUrl/cypher" `
    -Body '{"query": "MATCH (n:Person {name: \"ToDelete\"}) DETACH DELETE n"}' `
    -ExpectSuccess $false

# REMOVE Tests
Test-Endpoint -Name "REMOVE - Property" -Method "POST" -Url "$baseUrl/cypher" `
    -Body '{"query": "MATCH (n:Person) REMOVE n.temp RETURN n"}' `
    -ExpectSuccess $false

Test-Endpoint -Name "REMOVE - Label" -Method "POST" -Url "$baseUrl/cypher" `
    -Body '{"query": "MATCH (n:Person:VIP) REMOVE n:VIP RETURN n"}' `
    -ExpectSuccess $false

# ============================================
# 7. CYPHER QUERIES - CREATE OPERATIONS
# ============================================
Write-Host "`n━━━ 7. CYPHER QUERIES - CREATE OPERATIONS ━━━" -ForegroundColor Yellow

Test-Endpoint -Name "CREATE - Single Node" -Method "POST" -Url "$baseUrl/cypher" `
    -Body '{"query": "CREATE (n:Person {name: \"Emma\", age: 28}) RETURN n"}'

Test-Endpoint -Name "CREATE - Multiple Nodes" -Method "POST" -Url "$baseUrl/cypher" `
    -Body '{"query": "CREATE (a:Person {name: \"Frank\"}), (b:Person {name: \"Grace\"}) RETURN a, b"}'

# ============================================
# 8. DATA ENDPOINTS - Create Relationships
# ============================================
Write-Host "`n━━━ 8. DATA ENDPOINTS - CREATE RELATIONSHIPS ━━━" -ForegroundColor Yellow

Test-Endpoint -Name "Create Relationship - KNOWS" -Method "POST" -Url "$baseUrl/data/relationships" `
    -Body '{"type": "KNOWS", "from_node_id": 0, "to_node_id": 1, "properties": {"since": "2020"}}'

# ============================================
# 9. DATA ENDPOINTS - Update Nodes
# ============================================
Write-Host "`n━━━ 9. DATA ENDPOINTS - UPDATE NODES ━━━" -ForegroundColor Yellow

Test-Endpoint -Name "Update Node Properties" -Method "PUT" -Url "$baseUrl/data/nodes" `
    -Body '{"node_id": 0, "properties": {"age": 31, "city": "SF"}}'

# ============================================
# 10. DATA ENDPOINTS - Delete Nodes
# ============================================
Write-Host "`n━━━ 10. DATA ENDPOINTS - DELETE NODES ━━━" -ForegroundColor Yellow

Test-Endpoint -Name "Delete Node by ID" -Method "DELETE" -Url "$baseUrl/data/nodes/999" `
    -ExpectSuccess $false

# ============================================
# 11. BULK INGEST
# ============================================
Write-Host "`n━━━ 11. BULK INGEST OPERATIONS ━━━" -ForegroundColor Yellow

$bulkData = @'
{
  "nodes": [
    {"labels": ["Person"], "properties": {"name": "Helen", "age": 35}},
    {"labels": ["Person"], "properties": {"name": "Ivan", "age": 40}}
  ]
}
'@

Test-Endpoint -Name "Bulk Ingest Nodes" -Method "POST" -Url "$baseUrl/ingest" `
    -Body $bulkData

# ============================================
# 12. KNN VECTOR SEARCH
# ============================================
Write-Host "`n━━━ 12. KNN VECTOR SEARCH ━━━" -ForegroundColor Yellow

$knnQuery = @'
{
  "label": "Person",
  "vector": [0.1, 0.2, 0.3, 0.4, 0.5],
  "k": 5
}
'@

Test-Endpoint -Name "KNN Search" -Method "POST" -Url "$baseUrl/knn_traverse" `
    -Body $knnQuery -ExpectSuccess $false

# ============================================
# 13. ERROR CASES
# ============================================
Write-Host "`n━━━ 13. ERROR CASES (Should Fail) ━━━" -ForegroundColor Yellow

Test-Endpoint -Name "Invalid Cypher Syntax" -Method "POST" -Url "$baseUrl/cypher" `
    -Body '{"query": "INVALID CYPHER QUERY"}' -ExpectSuccess $false

Test-Endpoint -Name "Missing Query Field" -Method "POST" -Url "$baseUrl/cypher" `
    -Body '{"invalid": "field"}' -ExpectSuccess $false

Test-Endpoint -Name "Invalid JSON" -Method "POST" -Url "$baseUrl/cypher" `
    -Body '{invalid json}' -ExpectSuccess $false

Test-Endpoint -Name "Non-existent Endpoint" -Method "GET" -Url "$baseUrl/nonexistent" `
    -ExpectSuccess $false

# ============================================
# FINAL SUMMARY
# ============================================
Write-Host "`n═══════════════════════════════════════════════════════" -ForegroundColor Magenta
Write-Host "  TEST SUMMARY" -ForegroundColor Magenta
Write-Host "═══════════════════════════════════════════════════════" -ForegroundColor Magenta
Write-Host "  Total Tests: $total" -ForegroundColor White
Write-Host "  ✅ Passed: $passed" -ForegroundColor Green
Write-Host "  ❌ Failed: $failed" -ForegroundColor Red
Write-Host "  Success Rate: $([math]::Round(($passed/$total)*100, 2))%" -ForegroundColor Cyan
Write-Host "═══════════════════════════════════════════════════════" -ForegroundColor Magenta

# Stop the server
Write-Host "`n🛑 Stopping Nexus Server..." -ForegroundColor Yellow
Get-Process | Where-Object { $_.ProcessName -eq "nexus-server" } | Stop-Process -Force
Write-Host "✅ Server stopped" -ForegroundColor Green
