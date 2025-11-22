# Test UNION, DISTINCT, and Relationship queries that were fixed
# Tests only against Nexus (no Neo4j required)

param(
    [string]$NexusUri = "http://127.0.0.1:15474"
)

$ErrorActionPreference = "Continue"
$global:PassedTests = 0
$global:FailedTests = 0

Write-Host "╔══════════════════════════════════════════════════════════════╗" -ForegroundColor Cyan
Write-Host "║  Testing UNION, DISTINCT, and Relationship Fixes           ║" -ForegroundColor Cyan
Write-Host "╚══════════════════════════════════════════════════════════════╝" -ForegroundColor Cyan
Write-Host ""

# Function to execute query on Nexus
function Invoke-NexusQuery {
    param([string]$Cypher)
    
    $body = @{
        query = $Cypher
    } | ConvertTo-Json
    
    try {
        $response = Invoke-RestMethod -Uri "$NexusUri/cypher" `
            -Method POST `
            -Headers @{
                "Content-Type" = "application/json"
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

# Function to run a test
function Run-Test {
    param(
        [string]$Name,
        [string]$Query,
        [int]$ExpectedRows,
        [string]$Description = ""
    )
    
    Write-Host "Testing: $Name" -ForegroundColor Yellow
    if ($Description) {
        Write-Host "  $Description" -ForegroundColor Gray
    }
    
    $result = Invoke-NexusQuery -Cypher $Query
    
    if ($result.error) {
        Write-Host "  ❌ FAIL: Error - $($result.error)" -ForegroundColor Red
        $global:FailedTests++
        return $false
    }
    
    $actualRows = if ($result.rows) { $result.rows.Count } else { 0 }
    
    if ($actualRows -eq $ExpectedRows) {
        Write-Host "  ✅ PASS: Expected $ExpectedRows rows, got $actualRows" -ForegroundColor Green
        if ($result.rows) {
            Write-Host "  Rows:" -ForegroundColor Gray
            $result.rows | ForEach-Object { 
                $rowStr = ($_ | ConvertTo-Json -Compress)
                Write-Host "    $rowStr" -ForegroundColor Gray
            }
        }
        $global:PassedTests++
        return $true
    } else {
        Write-Host "  ❌ FAIL: Expected $ExpectedRows rows, got $actualRows" -ForegroundColor Red
        if ($result.rows) {
            Write-Host "  Rows returned:" -ForegroundColor Gray
            $result.rows | ForEach-Object { 
                $rowStr = ($_ | ConvertTo-Json -Compress)
                Write-Host "    $rowStr" -ForegroundColor Gray
            }
        } else {
            Write-Host "  No rows returned" -ForegroundColor Gray
        }
        $global:FailedTests++
        return $false
    }
}

# Setup: Clean database and create test data
Write-Host "`n🔧 Setting up test environment..." -ForegroundColor Cyan
Invoke-NexusQuery -Cypher "MATCH (n) DETACH DELETE n" | Out-Null

# Create test data
Write-Host "Creating test data..." -ForegroundColor Gray
Invoke-NexusQuery -Cypher "CREATE (a:Person {name: 'Alice', age: 30, city: 'NYC'})" | Out-Null
Invoke-NexusQuery -Cypher "CREATE (b:Person {name: 'Bob', age: 25, city: 'LA'})" | Out-Null
Invoke-NexusQuery -Cypher "CREATE (c:Person {name: 'Charlie', age: 35, city: 'NYC'})" | Out-Null
Invoke-NexusQuery -Cypher "CREATE (d:Person {name: 'David', age: 28, city: 'LA'})" | Out-Null
Invoke-NexusQuery -Cypher "CREATE (acme:Company {name: 'Acme'})" | Out-Null
Invoke-NexusQuery -Cypher "CREATE (tech:Company {name: 'TechCorp'})" | Out-Null

# Create relationships
Invoke-NexusQuery -Cypher "MATCH (a:Person {name: 'Alice'}), (acme:Company {name: 'Acme'}) CREATE (a)-[:WORKS_AT {since: 2020}]->(acme)" | Out-Null
Invoke-NexusQuery -Cypher "MATCH (a:Person {name: 'Alice'}), (tech:Company {name: 'TechCorp'}) CREATE (a)-[:WORKS_AT {since: 2021}]->(tech)" | Out-Null
Invoke-NexusQuery -Cypher "MATCH (b:Person {name: 'Bob'}), (acme:Company {name: 'Acme'}) CREATE (b)-[:WORKS_AT {since: 2019}]->(acme)" | Out-Null

Write-Host "✓ Test data created`n" -ForegroundColor Green

#═══════════════════════════════════════════════════════════════════
# UNION Tests
#═══════════════════════════════════════════════════════════════════
Write-Host "`n┌─────────────────────────────────────────────────────┐" -ForegroundColor Yellow
Write-Host "│ Section: UNION Queries                              │" -ForegroundColor Yellow
Write-Host "└─────────────────────────────────────────────────────┘" -ForegroundColor Yellow

Run-Test -Name "10.01 UNION two queries" `
    -Query "MATCH (n:Person) RETURN n.name AS name UNION MATCH (n:Company) RETURN n.name AS name" `
    -ExpectedRows 6 `
    -Description "Should return 4 Person names + 2 Company names = 6 rows"

Run-Test -Name "10.05 UNION with WHERE" `
    -Query "MATCH (n:Person) WHERE n.age > 30 RETURN n.name AS name UNION MATCH (n:Company) RETURN n.name AS name" `
    -ExpectedRows 4 `
    -Description "Should return 2 Person names (Alice, Charlie) + 2 Company names = 4 rows"

Run-Test -Name "10.08 UNION empty results" `
    -Query "MATCH (n:NonExistent) RETURN n.name AS name UNION MATCH (n:Person) RETURN n.name AS name" `
    -ExpectedRows 4 `
    -Description "Should return 4 Person names (empty side ignored)"

#═══════════════════════════════════════════════════════════════════
# DISTINCT Tests
#═══════════════════════════════════════════════════════════════════
Write-Host "`n┌─────────────────────────────────────────────────────┐" -ForegroundColor Yellow
Write-Host "│ Section: DISTINCT Queries                          │" -ForegroundColor Yellow
Write-Host "└─────────────────────────────────────────────────────┘" -ForegroundColor Yellow

Run-Test -Name "2.20 MATCH with DISTINCT" `
    -Query "MATCH (n:Person) RETURN DISTINCT n.city AS city" `
    -ExpectedRows 2 `
    -Description "Should return 2 distinct cities: NYC, LA"

#═══════════════════════════════════════════════════════════════════
# Relationship Tests
#═══════════════════════════════════════════════════════════════════
Write-Host "`n┌─────────────────────────────────────────────────────┐" -ForegroundColor Yellow
Write-Host "│ Section: Relationship Queries                       │" -ForegroundColor Yellow
Write-Host "└─────────────────────────────────────────────────────┘" -ForegroundColor Yellow

Run-Test -Name "7.19 Relationship with aggregation" `
    -Query "MATCH (a:Person)-[r:WORKS_AT]->(b:Company) RETURN a.name AS person, count(r) AS jobs ORDER BY person" `
    -ExpectedRows 2 `
    -Description "Should return 2 rows: Alice (2 jobs), Bob (1 job)"

Run-Test -Name "7.25 MATCH all connected nodes" `
    -Query "MATCH (a:Person)-[r]-(b) RETURN DISTINCT a.name AS name ORDER BY name" `
    -ExpectedRows 2 `
    -Description "Should return 2 distinct Person names: Alice, Bob"

Run-Test -Name "7.30 Complex relationship query" `
    -Query "MATCH (a:Person)-[r:WORKS_AT]->(c:Company) RETURN a.name AS person, c.name AS company, r.since AS year ORDER BY year" `
    -ExpectedRows 3 `
    -Description "Should return 3 rows: Bob-Acme (2019), Alice-Acme (2020), Alice-TechCorp (2021)"

#═══════════════════════════════════════════════════════════════════
# Summary
#═══════════════════════════════════════════════════════════════════
Write-Host "`n═══════════════════════════════════════════════════════" -ForegroundColor Cyan
Write-Host "Test Summary" -ForegroundColor Cyan
Write-Host "═══════════════════════════════════════════════════════" -ForegroundColor Cyan
Write-Host "Passed: $global:PassedTests" -ForegroundColor Green
Write-Host "Failed: $global:FailedTests" -ForegroundColor $(if ($global:FailedTests -eq 0) { "Green" } else { "Red" })
Write-Host "Total:  $($global:PassedTests + $global:FailedTests)" -ForegroundColor Yellow
Write-Host ""

if ($global:FailedTests -eq 0) {
    Write-Host "✅ All tests passed!" -ForegroundColor Green
    exit 0
} else {
    Write-Host "❌ Some tests failed" -ForegroundColor Red
    exit 1
}

