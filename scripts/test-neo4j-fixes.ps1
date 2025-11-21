# Test Neo4j Compatibility Fixes
# Tests the fixes for MATCH property filters, GROUP BY, UNION, and DISTINCT

param(
    [string]$NexusUri = "http://localhost:15474"
)

$ErrorActionPreference = "Continue"
$global:PassedTests = 0
$global:FailedTests = 0

Write-Host "╔══════════════════════════════════════════════════════════════╗" -ForegroundColor Cyan
Write-Host "║  Testing Neo4j Compatibility Fixes                         ║" -ForegroundColor Cyan
Write-Host "╚══════════════════════════════════════════════════════════════╝" -ForegroundColor Cyan
Write-Host ""

# Function to execute query on Nexus
function Invoke-NexusQuery {
    param([string]$Cypher, [hashtable]$Parameters = @{})
    
    $body = @{
        query = $Cypher
        parameters = $Parameters
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

# Function to run a test
function Run-Test {
    param(
        [string]$Name,
        [string]$Query,
        [int]$ExpectedRows,
        [string]$Description
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
        $global:PassedTests++
        return $true
    } else {
        Write-Host "  ❌ FAIL: Expected $ExpectedRows rows, got $actualRows" -ForegroundColor Red
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
Invoke-NexusQuery -Cypher "CREATE (b:Person {name: 'Bob', age: 30, city: 'LA'})" | Out-Null
Invoke-NexusQuery -Cypher "CREATE (c:Person {name: 'Charlie', age: 25, city: 'NYC'})" | Out-Null
Invoke-NexusQuery -Cypher "CREATE (d:Person {name: 'David', age: 35, city: 'LA'})" | Out-Null
Invoke-NexusQuery -Cypher "CREATE (acme:Company {name: 'Acme Corp'})" | Out-Null
Invoke-NexusQuery -Cypher "CREATE (tech:Company {name: 'Tech Inc'})" | Out-Null

# Create relationships
Invoke-NexusQuery -Cypher "MATCH (a:Person {name: 'Alice'}), (acme:Company {name: 'Acme Corp'}) CREATE (a)-[:WORKS_AT {since: 2020}]->(acme)" | Out-Null
Invoke-NexusQuery -Cypher "MATCH (b:Person {name: 'Bob'}), (acme:Company {name: 'Acme Corp'}) CREATE (b)-[:WORKS_AT {since: 2021}]->(acme)" | Out-Null

Write-Host "✓ Test data created`n" -ForegroundColor Green

#═══════════════════════════════════════════════════════════════════
# Phase 1: MATCH Property Filter Issues
#═══════════════════════════════════════════════════════════════════
Write-Host "`n┌─────────────────────────────────────────────────────┐" -ForegroundColor Yellow
Write-Host "│ Phase 1: MATCH Property Filter Issues (4 tests)     │" -ForegroundColor Yellow
Write-Host "└─────────────────────────────────────────────────────┘" -ForegroundColor Yellow

Run-Test -Name "1.1 MATCH Person with property" `
    -Query "MATCH (n:Person {name: 'Alice'}) RETURN n.name AS name" `
    -ExpectedRows 1 `
    -Description "Query: MATCH (n:Person {name: 'Alice'}) RETURN n.name AS name"

Run-Test -Name "1.2 MATCH and return multiple properties" `
    -Query "MATCH (n:Person {name: 'Alice'}) RETURN n.name AS name, n.age AS age" `
    -ExpectedRows 1 `
    -Description "Query: MATCH (n:Person {name: 'Alice'}) RETURN n.name AS name, n.age AS age"

Run-Test -Name "1.3 MATCH with WHERE equality" `
    -Query "MATCH (n:Person) WHERE n.name = 'Bob' RETURN n.name" `
    -ExpectedRows 1 `
    -Description "Query: MATCH (n:Person) WHERE n.name = 'Bob' RETURN n.name"

Run-Test -Name "1.4 MATCH with property access" `
    -Query "MATCH (n:Person) WHERE n.age = 30 RETURN n.name" `
    -ExpectedRows 2 `
    -Description "Query: MATCH (n:Person) WHERE n.age = 30 RETURN n.name (Alice and Bob both have age 30)"

#═══════════════════════════════════════════════════════════════════
# Phase 2: GROUP BY Aggregation Issues
#═══════════════════════════════════════════════════════════════════
Write-Host "`n┌─────────────────────────────────────────────────────┐" -ForegroundColor Yellow
Write-Host "│ Phase 2: GROUP BY Aggregation Issues (5 tests)      │" -ForegroundColor Yellow
Write-Host "└─────────────────────────────────────────────────────┘" -ForegroundColor Yellow

Run-Test -Name "2.1 COUNT with GROUP BY" `
    -Query "MATCH (n:Person) RETURN n.city AS city, count(n) AS cnt ORDER BY city" `
    -ExpectedRows 2 `
    -Description "Query: MATCH (n:Person) RETURN n.city AS city, count(n) AS cnt ORDER BY city (2 cities: LA and NYC)"

Run-Test -Name "2.2 SUM with GROUP BY" `
    -Query "MATCH (n:Person) RETURN n.city AS city, sum(n.age) AS total ORDER BY city" `
    -ExpectedRows 2 `
    -Description "Query: MATCH (n:Person) RETURN n.city AS city, sum(n.age) AS total ORDER BY city"

Run-Test -Name "2.3 AVG with GROUP BY" `
    -Query "MATCH (n:Person) RETURN n.city AS city, avg(n.age) AS avg_age ORDER BY city" `
    -ExpectedRows 2 `
    -Description "Query: MATCH (n:Person) RETURN n.city AS city, avg(n.age) AS avg_age ORDER BY city"

Run-Test -Name "2.4 Aggregation with ORDER BY" `
    -Query "MATCH (n:Person) RETURN n.city AS city, count(n) AS cnt ORDER BY cnt DESC" `
    -ExpectedRows 2 `
    -Description "Query: MATCH (n:Person) RETURN n.city AS city, count(n) AS cnt ORDER BY cnt DESC"

Run-Test -Name "2.5 Aggregation with LIMIT" `
    -Query "MATCH (n:Person) RETURN n.city AS city, count(n) AS cnt ORDER BY cnt DESC LIMIT 2" `
    -ExpectedRows 2 `
    -Description "Query: MATCH (n:Person) RETURN n.city AS city, count(n) AS cnt ORDER BY cnt DESC LIMIT 2"

#═══════════════════════════════════════════════════════════════════
# Phase 3: UNION Query Issues
#═══════════════════════════════════════════════════════════════════
Write-Host "`n┌─────────────────────────────────────────────────────┐" -ForegroundColor Yellow
Write-Host "│ Phase 3: UNION Query Issues (4 tests)               │" -ForegroundColor Yellow
Write-Host "└─────────────────────────────────────────────────────┘" -ForegroundColor Yellow

Run-Test -Name "3.1 UNION two queries" `
    -Query "MATCH (n:Person) RETURN n.name AS name UNION MATCH (n:Company) RETURN n.name AS name" `
    -ExpectedRows 6 `
    -Description "Query: MATCH (n:Person) RETURN n.name AS name UNION MATCH (n:Company) RETURN n.name AS name (4 Person + 2 Company = 6 unique names)"

Run-Test -Name "3.2 UNION ALL" `
    -Query "MATCH (n:Person) RETURN n.name AS name UNION ALL MATCH (n:Company) RETURN n.name AS name" `
    -ExpectedRows 6 `
    -Description "Query: MATCH (n:Person) RETURN n.name AS name UNION ALL MATCH (n:Company) RETURN n.name AS name (4 Person + 2 Company = 6 total)"

Run-Test -Name "3.3 UNION with WHERE" `
    -Query "MATCH (n:Person) WHERE n.age > 30 RETURN n.name AS name UNION MATCH (n:Company) RETURN n.name AS name" `
    -ExpectedRows 3 `
    -Description "Query: MATCH (n:Person) WHERE n.age > 30 RETURN n.name AS name UNION MATCH (n:Company) RETURN n.name AS name (1 Person + 2 Company = 3)"

Run-Test -Name "3.4 UNION empty results" `
    -Query "MATCH (n:NonExistent) RETURN n.name AS name UNION MATCH (n:Person) RETURN n.name AS name" `
    -ExpectedRows 4 `
    -Description "Query: MATCH (n:NonExistent) RETURN n.name AS name UNION MATCH (n:Person) RETURN n.name AS name (0 + 4 = 4)"

#═══════════════════════════════════════════════════════════════════
# Phase 4: DISTINCT Operation Issues
#═══════════════════════════════════════════════════════════════════
Write-Host "`n┌─────────────────────────────────────────────────────┐" -ForegroundColor Yellow
Write-Host "│ Phase 4: DISTINCT Operation Issues (1 test)          │" -ForegroundColor Yellow
Write-Host "└─────────────────────────────────────────────────────┘" -ForegroundColor Yellow

Run-Test -Name "4.1 MATCH with DISTINCT" `
    -Query "MATCH (n:Person) RETURN DISTINCT n.city AS city" `
    -ExpectedRows 2 `
    -Description "Query: MATCH (n:Person) RETURN DISTINCT n.city AS city (2 unique cities: LA and NYC)"

#═══════════════════════════════════════════════════════════════════
# Summary
#═══════════════════════════════════════════════════════════════════
Write-Host "`n╔══════════════════════════════════════════════════════════════╗" -ForegroundColor Cyan
Write-Host "║  Test Summary                                                ║" -ForegroundColor Cyan
Write-Host "╚══════════════════════════════════════════════════════════════╝" -ForegroundColor Cyan
Write-Host ""
Write-Host "Total Tests: $($global:PassedTests + $global:FailedTests)" -ForegroundColor White
Write-Host "✅ Passed: $global:PassedTests" -ForegroundColor Green
Write-Host "❌ Failed: $global:FailedTests" -ForegroundColor Red

if ($global:FailedTests -eq 0) {
    Write-Host "`n🎉 All tests passed!" -ForegroundColor Green
    exit 0
} else {
    Write-Host "`n⚠️  Some tests failed. Review the output above." -ForegroundColor Yellow
    exit 1
}

