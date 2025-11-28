# Simplified test for Section 7 only
$ErrorActionPreference = "Continue"

# Configuration
$Neo4jUri = "http://localhost:7474"
$NexusUri = "http://localhost:15474"
$Neo4jUser = "neo4j"
$Neo4jPassword = "password"

# Functions based on original script
function Invoke-Neo4jQuery {
    param([string]$Cypher)
    $body = @{
        statements = @(
            @{
                statement = $Cypher
                parameters = @{}
            }
        )
    } | ConvertTo-Json -Depth 10
    
    $auth = [Convert]::ToBase64String([Text.Encoding]::ASCII.GetBytes("${Neo4jUser}:${Neo4jPassword}"))
    
    try {
        $response = Invoke-RestMethod -Uri "$Neo4jUri/db/neo4j/tx/commit" `
            -Method POST `
            -Headers @{
                "Authorization" = "Basic $auth"
                "Content-Type" = "application/json"
                "Accept" = "application/json"
            } `
            -Body $body `
            -ErrorAction Stop `
            -TimeoutSec 30
        
        if ($response.errors -and $response.errors.Count -gt 0) {
            return @{ error = $response.errors[0].message }
        }
        
        return $response.results[0]
    }
    catch {
        return @{ error = $_.Exception.Message }
    }
}

function Invoke-NexusQuery {
    param([string]$Cypher)
    $body = @{
        query = $Cypher
        parameters = @{}
    } | ConvertTo-Json -Depth 10
    
    try {
        $response = Invoke-RestMethod -Uri "$NexusUri/cypher" `
            -Method POST `
            -Headers @{
                "Content-Type" = "application/json"
                "Accept" = "application/json"
            } `
            -Body $body `
            -ErrorAction Stop `
            -TimeoutSec 30
        
        return $response
    }
    catch {
        return @{ error = $_.Exception.Message }
    }
}

# Clean and setup
Write-Host "Cleaning databases..." -ForegroundColor Cyan
Invoke-Neo4jQuery -Cypher "MATCH (n) DETACH DELETE n" | Out-Null
Invoke-NexusQuery -Cypher "MATCH (n) DETACH DELETE n" | Out-Null
Start-Sleep -Milliseconds 500

Write-Host "Setting up test data..." -ForegroundColor Cyan
Invoke-Neo4jQuery -Cypher "CREATE (a:Person {name: 'Alice', age: 30}), (b:Person {name: 'Bob', age: 25}), (c:Company {name: 'Acme'}), (d:Company {name: 'TechCorp'})" | Out-Null
Invoke-NexusQuery -Cypher "CREATE (a:Person {name: 'Alice', age: 30}), (b:Person {name: 'Bob', age: 25}), (c:Company {name: 'Acme'}), (d:Company {name: 'TechCorp'})" | Out-Null
Start-Sleep -Milliseconds 200

Invoke-Neo4jQuery -Cypher "MATCH (a:Person {name: 'Alice'}), (c:Company {name: 'Acme'}) CREATE (a)-[:WORKS_AT {since: 2020}]->(c)" | Out-Null
Invoke-NexusQuery -Cypher "MATCH (a:Person {name: 'Alice'}), (c:Company {name: 'Acme'}) CREATE (a)-[:WORKS_AT {since: 2020}]->(c)" | Out-Null

Invoke-Neo4jQuery -Cypher "MATCH (a:Person {name: 'Alice'}), (d:Company {name: 'TechCorp'}) CREATE (a)-[:WORKS_AT {since: 2022}]->(d)" | Out-Null
Invoke-NexusQuery -Cypher "MATCH (a:Person {name: 'Alice'}), (d:Company {name: 'TechCorp'}) CREATE (a)-[:WORKS_AT {since: 2022}]->(d)" | Out-Null

Invoke-Neo4jQuery -Cypher "MATCH (b:Person {name: 'Bob'}), (c:Company {name: 'Acme'}) CREATE (b)-[:WORKS_AT {since: 2021}]->(c)" | Out-Null
Invoke-NexusQuery -Cypher "MATCH (b:Person {name: 'Bob'}), (c:Company {name: 'Acme'}) CREATE (b)-[:WORKS_AT {since: 2021}]->(c)" | Out-Null

Start-Sleep -Milliseconds 300

Write-Host "`nTesting Section 7 (3 critical tests):" -ForegroundColor Yellow

# Test 7.19
Write-Host "Test 7.19..." -NoNewline
$q = 'MATCH (a:Person)-[r:WORKS_AT]->(b:Company) RETURN a.name AS person, count(r) AS jobs ORDER BY person'
$neo = Invoke-Neo4jQuery -Cypher $q
$nex = Invoke-NexusQuery -Cypher $q
$neoRows = if ($neo -and $neo.data) { $neo.data.Count } else { 0 }
$nexRows = if ($nex -and $nex.rows) { $nex.rows.Count } else { 0 }
if ($neo.error) {
    Write-Host " FAIL (Neo4j Error: $($neo.error))" -ForegroundColor Red
} elseif ($nex.error) {
    Write-Host " FAIL (Nexus Error: $($nex.error))" -ForegroundColor Red
} elseif ($neoRows -eq $nexRows -and $neoRows -eq 2) {
    Write-Host " PASS (Both: $neoRows rows)" -ForegroundColor Green
} else {
    Write-Host " FAIL (Neo4j: $neoRows, Nexus: $nexRows, Expected: 2)" -ForegroundColor Red
}

# Test 7.25
Write-Host "Test 7.25..." -NoNewline
$q = 'MATCH (a:Person)-[r]-(b) RETURN DISTINCT a.name AS name ORDER BY name'
$neo = Invoke-Neo4jQuery -Cypher $q
$nex = Invoke-NexusQuery -Cypher $q
$neoRows = if ($neo -and $neo.data) { $neo.data.Count } else { 0 }
$nexRows = if ($nex -and $nex.rows) { $nex.rows.Count } else { 0 }
if ($neo.error) {
    Write-Host " FAIL (Neo4j Error: $($neo.error))" -ForegroundColor Red
} elseif ($nex.error) {
    Write-Host " FAIL (Nexus Error: $($nex.error))" -ForegroundColor Red
} elseif ($neoRows -eq $nexRows -and $neoRows -eq 2) {
    Write-Host " PASS (Both: $neoRows rows)" -ForegroundColor Green
} else {
    Write-Host " FAIL (Neo4j: $neoRows, Nexus: $nexRows, Expected: 2)" -ForegroundColor Red
}

# Test 7.30
Write-Host "Test 7.30..." -NoNewline
$q = 'MATCH (a:Person)-[r:WORKS_AT]->(c:Company) RETURN a.name AS person, c.name AS company, r.since AS year ORDER BY year'
$neo = Invoke-Neo4jQuery -Cypher $q
$nex = Invoke-NexusQuery -Cypher $q
$neoRows = if ($neo -and $neo.data) { $neo.data.Count } else { 0 }
$nexRows = if ($nex -and $nex.rows) { $nex.rows.Count } else { 0 }
if ($neo.error) {
    Write-Host " FAIL (Neo4j Error: $($neo.error))" -ForegroundColor Red
} elseif ($nex.error) {
    Write-Host " FAIL (Nexus Error: $($nex.error))" -ForegroundColor Red
} elseif ($neoRows -eq $nexRows -and $neoRows -eq 3) {
    Write-Host " PASS (Both: $neoRows rows)" -ForegroundColor Green
} else {
    Write-Host " FAIL (Neo4j: $neoRows, Nexus: $nexRows, Expected: 3)" -ForegroundColor Red
}

Write-Host "`nDone!" -ForegroundColor Cyan
