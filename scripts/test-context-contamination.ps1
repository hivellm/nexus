# Test script to reproduce context contamination issue
# This script tests the exact scenario where relationships contaminate node variables

$ErrorActionPreference = "Continue"
$NexusUri = "http://localhost:15474"

function Invoke-NexusQuery {
    param([string]$Cypher)
    $body = @{ query = $Cypher } | ConvertTo-Json
    try {
        $response = Invoke-RestMethod -Uri "$NexusUri/cypher" -Method POST -Headers @{"Content-Type"="application/json"} -Body $body -ErrorAction Stop -TimeoutSec 30
        return $response
    } catch {
        return @{ error = $_.Exception.Message }
    }
}

Write-Host "=== Test: Context Contamination ===" -ForegroundColor Cyan
Write-Host ""

# Step 1: Clean database
Write-Host "Step 1: Cleaning database..." -ForegroundColor Yellow
Invoke-NexusQuery -Cypher "MATCH (n) DETACH DELETE n" | Out-Null
Start-Sleep -Milliseconds 500

# Step 2: Create nodes
Write-Host "Step 2: Creating nodes (Alice, Bob, Acme, TechCorp)..." -ForegroundColor Yellow
Invoke-NexusQuery -Cypher "CREATE (a:Person {name: 'Alice', age: 30}), (b:Person {name: 'Bob', age: 25}), (c:Company {name: 'Acme'}), (d:Company {name: 'TechCorp'})" | Out-Null
Start-Sleep -Milliseconds 200

# Step 3: First MATCH...CREATE query
Write-Host "Step 3: Executing first MATCH...CREATE (Alice -> Acme)..." -ForegroundColor Yellow
$result1 = Invoke-NexusQuery -Cypher "MATCH (a:Person {name: 'Alice'}), (c:Company {name: 'Acme'}) CREATE (a)-[:WORKS_AT {since: 2020}]->(c) RETURN a, c"
Write-Host "  Result: $($result1.rows.Count) rows returned" -ForegroundColor Gray
if ($result1.rows) {
    Write-Host "  First row: $($result1.rows[0].values | ConvertTo-Json -Compress)" -ForegroundColor Gray
}
Start-Sleep -Milliseconds 200

# Step 4: Second MATCH...CREATE query (this is where the problem occurs)
Write-Host "Step 4: Executing second MATCH...CREATE (Alice -> TechCorp)..." -ForegroundColor Yellow
Write-Host "  This query should find Alice and TechCorp, but Filter may find relationships instead of nodes" -ForegroundColor Gray
$result2 = Invoke-NexusQuery -Cypher "MATCH (a:Person {name: 'Alice'}), (d:Company {name: 'TechCorp'}) CREATE (a)-[:WORKS_AT {since: 2022}]->(d) RETURN a, d"
Write-Host "  Result: $($result2.rows.Count) rows returned" -ForegroundColor Gray
if ($result2.rows) {
    Write-Host "  First row: $($result2.rows[0].values | ConvertTo-Json -Compress)" -ForegroundColor Gray
}
if ($result2.error) {
    Write-Host "  ERROR: $($result2.error)" -ForegroundColor Red
}

# Step 5: Verify relationships were created
Write-Host "Step 5: Verifying relationships..." -ForegroundColor Yellow
$verify = Invoke-NexusQuery -Cypher "MATCH (a:Person)-[r:WORKS_AT]->(c:Company) RETURN a.name AS person, c.name AS company, r.since AS year ORDER BY year"
Write-Host "  Found $($verify.rows.Count) relationships:" -ForegroundColor Gray
foreach ($row in $verify.rows) {
    Write-Host "    $($row.values[0]) -> $($row.values[1]) (since $($row.values[2]))" -ForegroundColor Gray
}

# Expected: 2 relationships (Alice->Acme, Alice->TechCorp)
# If only 1 is found, the second CREATE failed due to context contamination
if ($verify.rows.Count -eq 2) {
    Write-Host ""
    Write-Host "SUCCESS: Both relationships created correctly!" -ForegroundColor Green
} else {
    Write-Host ""
    Write-Host "FAILURE: Expected 2 relationships, found $($verify.rows.Count)" -ForegroundColor Red
    Write-Host "  This indicates context contamination - check server logs for [TRACE] messages" -ForegroundColor Yellow
}

Write-Host ""
Write-Host "=== Test Complete ===" -ForegroundColor Cyan
Write-Host "Check server console logs for [TRACE] messages to see where relationships are introduced" -ForegroundColor Gray

