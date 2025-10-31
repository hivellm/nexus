# Test DELETE and DETACH DELETE functionality

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

function Get-Count {
    param([string]$Query = "MATCH (n) RETURN count(*) AS count")
    
    $result = Invoke-NexusQuery -Cypher $Query
    if ($null -eq $result -or $null -eq $result.rows -or $result.rows.Count -eq 0) {
        return 0
    }
    if ($result.rows[0] -is [array]) {
        return $result.rows[0][0]
    } else {
        return $result.rows[0].values[0]
    }
}

Write-Host "`n[TEST] DELETE Operations Test`n" -ForegroundColor Cyan

# Test 1: Clean database
Write-Host "1. Cleaning database..." -ForegroundColor Yellow
Invoke-NexusQuery -Cypher "MATCH (n) DETACH DELETE n" | Out-Null
$count = Get-Count
Write-Host "   Count after DETACH DELETE: $count" -ForegroundColor $(if ($count -eq 0) { "Green" } else { "Red" })
if ($count -ne 0) { Write-Host "   [FAIL] Expected 0 nodes!" -ForegroundColor Red }

# Test 2: Create nodes
Write-Host "`n2. Creating 3 nodes..." -ForegroundColor Yellow
Invoke-NexusQuery -Cypher "CREATE (p:Person {name: 'Alice', age: 30})" | Out-Null
Invoke-NexusQuery -Cypher "CREATE (p:Person {name: 'Bob', age: 25})" | Out-Null
Invoke-NexusQuery -Cypher "CREATE (p:Person {name: 'Charlie', age: 35})" | Out-Null
$count = Get-Count
Write-Host "   Count after CREATE: $count" -ForegroundColor $(if ($count -eq 3) { "Green" } else { "Red" })
if ($count -ne 3) { Write-Host "   [FAIL] Expected 3 nodes, got $count!" -ForegroundColor Red }

# Test 3: Delete single node
Write-Host "`n3. Deleting Alice..." -ForegroundColor Yellow
Invoke-NexusQuery -Cypher "MATCH (n:Person {name: 'Alice'}) DELETE n" | Out-Null
$count = Get-Count
Write-Host "   Count after DELETE: $count" -ForegroundColor $(if ($count -eq 2) { "Green" } else { "Red" })
if ($count -ne 2) { Write-Host "   [FAIL] Expected 2 nodes, got $count!" -ForegroundColor Red }

# Test 4: Create relationship
Write-Host "`n4. Creating relationship Bob->Charlie..." -ForegroundColor Yellow
Invoke-NexusQuery -Cypher "MATCH (p1:Person {name: 'Bob'}), (p2:Person {name: 'Charlie'}) CREATE (p1)-[:KNOWS]->(p2)" | Out-Null
$relCount = Get-Count -Query "MATCH ()-[r]->() RETURN count(*) AS count"
Write-Host "   Relationship count: $relCount" -ForegroundColor $(if ($relCount -eq 1) { "Green" } else { "Red" })
if ($relCount -ne 1) { Write-Host "   [FAIL] Expected 1 relationship, got $relCount!" -ForegroundColor Red }

# Test 5: Try DELETE without DETACH (should fail or delete anyway)
Write-Host "`n5. Trying DELETE Bob (has relationship)..." -ForegroundColor Yellow
Invoke-NexusQuery -Cypher "MATCH (n:Person {name: 'Bob'}) DELETE n" | Out-Null
$count = Get-Count
Write-Host "   Count after DELETE: $count" -ForegroundColor Cyan
Write-Host "   Relationship count: $(Get-Count -Query 'MATCH ()-[r]->() RETURN count(*) AS count')" -ForegroundColor Cyan

# Test 6: DETACH DELETE remaining
Write-Host "`n6. DETACH DELETE all remaining nodes..." -ForegroundColor Yellow
Invoke-NexusQuery -Cypher "MATCH (n) DETACH DELETE n" | Out-Null
$count = Get-Count
$relCount = Get-Count -Query "MATCH ()-[r]->() RETURN count(*) AS count"
Write-Host "   Node count: $count" -ForegroundColor $(if ($count -eq 0) { "Green" } else { "Red" })
Write-Host "   Relationship count: $relCount" -ForegroundColor $(if ($relCount -eq 0) { "Green" } else { "Red" })

if ($count -eq 0 -and $relCount -eq 0) {
    Write-Host "`n[PASS] All DELETE tests passed!" -ForegroundColor Green
} else {
    Write-Host "`n[FAIL] DELETE tests failed!" -ForegroundColor Red
}

