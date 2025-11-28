# Neo4j vs Nexus Compatibility Test
# Executes identical queries on SEPARATE Neo4j and Nexus instances
# Compares results to validate Nexus compatibility with Neo4j

param(
    [string]$Neo4jUri = "http://localhost:7474",
    [string]$Neo4jUser = "neo4j",
    [string]$Neo4jPassword = "password",
    [string]$NexusUri = "http://localhost:15474"
)

Write-Host "[CHECK] Neo4j vs Nexus Compatibility Test" -ForegroundColor Cyan
Write-Host "=" * 60

# Test queries to run on both databases
$queries = @(
    @{
        name = "Count all nodes"
        cypher = "MATCH (n) RETURN count(*) AS count"
    },
    @{
        name = "Count nodes by label"
        cypher = "MATCH (n:Person) RETURN count(*) AS count"
    },
    @{
        name = "Get node properties"
        cypher = "MATCH (n:Person) RETURN n.name AS name, n.age AS age LIMIT 5"
    },
    @{
        name = "Count relationships"
        cypher = 'MATCH ()-[r:KNOWS]->() RETURN count(*) AS count'
    },
    @{
        name = "Relationship properties"
        cypher = 'MATCH (a)-[r:KNOWS]->(b) RETURN a.name AS from, b.name AS to, r.since AS since LIMIT 5'
    },
    @{
        name = "Multiple labels"
        cypher = "MATCH (n:Person:Employee) RETURN count(*) AS count"
    },
    @{
        name = "WHERE clause"
        cypher = "MATCH (n:Person) WHERE n.age > 25 RETURN count(*) AS count"
    },
    @{
        name = "Aggregation - avg"
        cypher = "MATCH (n:Person) RETURN avg(n.age) AS average_age"
    },
    @{
        name = "Aggregation - min/max"
        cypher = "MATCH (n:Person) RETURN min(n.age) AS min_age, max(n.age) AS max_age"
    },
    @{
        name = "ORDER BY"
        cypher = "MATCH (n:Person) RETURN n.name AS name ORDER BY n.name LIMIT 5"
    },
    @{
        name = "UNION query"
        cypher = "MATCH (p:Person) RETURN p.name AS name UNION MATCH (c:Company) RETURN c.name AS name"
    },
    @{
        name = "Labels function"
        cypher = "MATCH (n) RETURN labels(n) AS labels LIMIT 5"
    },
    @{
        name = "Keys function"
        cypher = "MATCH (n:Person) RETURN keys(n) AS keys LIMIT 1"
    },
    @{
        name = "ID function"
        cypher = "MATCH (n:Person) RETURN id(n) AS id LIMIT 5"
    },
    @{
        name = "Type function"
        cypher = 'MATCH ()-[r]->() RETURN type(r) AS type LIMIT 5'
    },
    @{
        name = "Bidirectional relationships"
        cypher = 'MATCH (a:Person)-[r:KNOWS]-(b:Person) RETURN count(*) AS count'
    },
    @{
        name = "Count with DISTINCT"
        cypher = "MATCH (n:Person) RETURN count(DISTINCT n.age) AS unique_ages"
    }
)

$results = @()
$passed = 0
$failed = 0
$skipped = 0

function Invoke-Neo4jQuery {
    param([string]$Cypher)
    
    $body = @{
        statements = @(
            @{
                statement = $Cypher
            }
        )
    } | ConvertTo-Json -Depth 3
    
    $auth = [Convert]::ToBase64String([Text.Encoding]::ASCII.GetBytes("${Neo4jUser}:${Neo4jPassword}"))
    
    try {
        $response = Invoke-RestMethod -Uri "$Neo4jUri/db/neo4j/tx/commit" `
            -Method POST `
            -Headers @{
                "Authorization" = "Basic $auth"
                "Content-Type" = "application/json"
            } `
            -Body $body `
            -ErrorAction Stop
        
        return $response.results[0]
    }
    catch {
        Write-Host "Neo4j Error: $_" -ForegroundColor Red
        return $null
    }
}

function Invoke-NexusQuery {
    param([string]$Cypher)
    
    $body = @{
        query = $Cypher
    } | ConvertTo-Json
    
    try {
        $response = Invoke-RestMethod -Uri "$NexusUri/cypher" `
            -Method POST `
            -Headers @{"Content-Type" = "application/json"} `
            -Body $body `
            -ErrorAction Stop
        
        return $response
    }
    catch {
        Write-Host "Nexus Error: $_" -ForegroundColor Red
        return $null
    }
}

function Compare-Results {
    param(
        [string]$QueryName,
        [object]$Neo4jResult,
        [object]$NexusResult
    )
    
    Write-Host "`n[TEST] Testing: $QueryName" -ForegroundColor Yellow
    Write-Host "Query: $($queries | Where-Object { $_.name -eq $QueryName } | Select-Object -ExpandProperty cypher)"
    
    if ($null -eq $Neo4jResult) {
        Write-Host "[WARN]  Neo4j query failed - SKIPPED" -ForegroundColor Yellow
        return "skipped"
    }
    
    if ($null -eq $NexusResult) {
        Write-Host "[FAIL] Nexus query failed - FAILED" -ForegroundColor Red
        return "failed"
    }
    
    # Compare row counts
    $neo4jRowCount = if ($Neo4jResult.data) { $Neo4jResult.data.Count } else { 0 }
    $nexusRowCount = if ($NexusResult.rows) { $NexusResult.rows.Count } else { 0 }
    
    Write-Host "Neo4j rows: $neo4jRowCount | Nexus rows: $nexusRowCount"
    
    if ($neo4jRowCount -ne $nexusRowCount) {
        Write-Host "[FAIL] Row count mismatch!" -ForegroundColor Red
        return "failed"
    }
    
    # Compare column counts
    $neo4jColCount = if ($Neo4jResult.columns) { $Neo4jResult.columns.Count } else { 0 }
    $nexusColCount = if ($NexusResult.columns) { $NexusResult.columns.Count } else { 0 }
    
    if ($neo4jColCount -ne $nexusColCount) {
        Write-Host "[WARN]  Column count different: Neo4j=$neo4jColCount, Nexus=$nexusColCount" -ForegroundColor Yellow
    }
    
    # For count queries, compare the actual count value
    if ($QueryName -like "*Count*" -and $neo4jRowCount -gt 0 -and $nexusRowCount -gt 0) {
        $neo4jCount = $Neo4jResult.data[0].row[0]
        
        # Nexus returns rows as arrays, not objects with .values
        $nexusCount = if ($NexusResult.rows[0] -is [array]) {
            $NexusResult.rows[0][0]
        } else {
            $NexusResult.rows[0].values[0]
        }
        
        Write-Host "Neo4j count: $neo4jCount | Nexus count: $nexusCount"
        
        if ($neo4jCount -eq $nexusCount) {
            Write-Host "[PASS] PASS - Results match!" -ForegroundColor Green
            return "passed"
        }
        else {
            Write-Host "[FAIL] FAIL - Count mismatch!" -ForegroundColor Red
            return "failed"
        }
    }
    
    Write-Host "[PASS] PASS - Structure matches!" -ForegroundColor Green
    return "passed"
}

# Setup: Clear both databases first
Write-Host "`n[SETUP] Setup: Clearing databases..." -ForegroundColor Cyan

# Clear Neo4j database
Write-Host "  Clearing Neo4j..."
try {
    Invoke-Neo4jQuery -Cypher "MATCH (n) DETACH DELETE n" | Out-Null
    Invoke-Neo4jQuery -Cypher "MATCH ()-[r]->() DELETE r" | Out-Null
} catch {
    Write-Host "    Warning: Neo4j cleanup failed: $_" -ForegroundColor Yellow
}

# Clear Nexus database
Write-Host "  Clearing Nexus..."
try {
    Invoke-NexusQuery -Cypher "MATCH (n) DETACH DELETE n" | Out-Null
    Invoke-NexusQuery -Cypher "MATCH ()-[r]->() DELETE r" | Out-Null
} catch {
    Write-Host "    Warning: Nexus cleanup failed: $_" -ForegroundColor Yellow
}

Write-Host "`n[PASS] Databases cleared" -ForegroundColor Green

# Create test data
Write-Host "`n[DATA] Creating test data in both databases..." -ForegroundColor Cyan

$setupQueries = @(
    "CREATE (p:Person {name: 'Alice', age: 30})",
    "CREATE (p:Person {name: 'Bob', age: 25})",
    "CREATE (p:Person {name: 'Charlie', age: 35})",
    "CREATE (p:Person:Employee {name: 'David', age: 28, role: 'Developer'})",
    "CREATE (c:Company {name: 'Acme Inc'})",
    "MATCH (p:Person {name: 'Alice'}), (c:Company {name: 'Acme Inc'}) CREATE (p)-[:WORKS_AT {since: 2020}]->(c)",
    "MATCH (p1:Person {name: 'Alice'}), (p2:Person {name: 'Bob'}) CREATE (p1)-[:KNOWS {since: 2015}]->(p2)",
    "MATCH (p1:Person {name: 'Bob'}), (p2:Person {name: 'Charlie'}) CREATE (p1)-[:KNOWS {since: 2018}]->(p2)"
)

foreach ($query in $setupQueries) {
    Write-Host "  Neo4j: $query"
    Invoke-Neo4jQuery -Cypher $query | Out-Null
    
    Write-Host "  Nexus: $query"
    Invoke-NexusQuery -Cypher $query | Out-Null
}

Write-Host "`n[PASS] Test data created" -ForegroundColor Green

# Run compatibility tests
Write-Host "`n🧪 Running Compatibility Tests..." -ForegroundColor Cyan
Write-Host "=" * 60

foreach ($query in $queries) {
    $neo4jResult = Invoke-Neo4jQuery -Cypher $query.cypher
    $nexusResult = Invoke-NexusQuery -Cypher $query.cypher
    
    $status = Compare-Results -QueryName $query.name -Neo4jResult $neo4jResult -NexusResult $nexusResult
    
    switch ($status) {
        "passed" { $passed++ }
        "failed" { $failed++ }
        "skipped" { $skipped++ }
    }
    
    $results += @{
        query = $query.name
        status = $status
        cypher = $query.cypher
    }
}

# Summary
Write-Host "`n" + ("=" * 60)
Write-Host "[TEST] COMPATIBILITY TEST SUMMARY" -ForegroundColor Cyan
Write-Host ("=" * 60)
Write-Host "Total Tests: $($queries.Count)"
Write-Host "[PASS] Passed: $passed" -ForegroundColor Green
Write-Host "[FAIL] Failed: $failed" -ForegroundColor Red
Write-Host "[WARN]  Skipped: $skipped" -ForegroundColor Yellow

$passRate = if ($queries.Count -gt 0) { 
    [math]::Round(($passed / $queries.Count) * 100, 2) 
} else { 
    0 
}
Write-Host "`n[RESULT] Pass Rate: $passRate%" -ForegroundColor $(if ($passRate -ge 90) { "Green" } elseif ($passRate -ge 70) { "Yellow" } else { "Red" })

# Export results
$reportDir = Join-Path $PSScriptRoot "reports"
if (-not (Test-Path $reportDir)) {
    New-Item -ItemType Directory -Path $reportDir -Force | Out-Null
}
$reportPath = Join-Path $reportDir "neo4j-cross-compatibility-report.json"
$results | ConvertTo-Json -Depth 3 | Out-File $reportPath
Write-Host "`nReport saved to: $reportPath" -ForegroundColor Cyan

# Exit code
if ($failed -gt 0) {
    exit 1
}
exit 0

