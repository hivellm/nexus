# Extended Neo4j Compatibility Test
# Tests all implemented features including WHERE clauses, null handling, etc.

param(
    [string]$Neo4jUri = "http://localhost:7474",
    [string]$Neo4jUser = "neo4j",
    [string]$Neo4jPassword = "password",
    [string]$NexusUri = "http://localhost:15474"
)

Write-Host "=== Extended Neo4j Compatibility Test ===" -ForegroundColor Cyan
Write-Host ""

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
        return $null
    }
}

function Compare-Values {
    param($Val1, $Val2)
    
    if ($null -eq $Val1 -and $null -eq $Val2) { return $true }
    if ($null -eq $Val1 -or $null -eq $Val2) { return $false }
    
    if ($Val1 -is [array] -and $Val2 -is [array]) {
        if ($Val1.Count -ne $Val2.Count) { return $false }
        for ($i = 0; $i -lt $Val1.Count; $i++) {
            if (-not (Compare-Values $Val1[$i] $Val2[$i])) { return $false }
        }
        return $true
    }
    
    if ($Val1 -is [double] -and $Val2 -is [double]) {
        return [math]::Abs($Val1 - $Val2) -lt 0.0001
    }
    
    return $Val1 -eq $Val2
}

# Setup test data
Write-Host "Setting up test data..." -ForegroundColor Yellow
$setupQueries = @(
    "CREATE (p:Person {name: 'Alice', age: 30, city: 'NYC'})",
    "CREATE (p:Person {name: 'Bob', age: 25, city: 'LA'})",
    "CREATE (p:Person {name: 'Charlie', age: 35})",
    "MATCH (p1:Person {name: 'Alice'}), (p2:Person {name: 'Bob'}) CREATE (p1)-[:KNOWS]->(p2)"
)

foreach ($query in $setupQueries) {
    Invoke-Neo4jQuery -Cypher $query | Out-Null
    Invoke-NexusQuery -Cypher $query | Out-Null
}
Write-Host "Test data created" -ForegroundColor Green
Write-Host ""

# Test queries
$testQueries = @(
    @{
        name = "count(*) with MATCH"
        cypher = "MATCH (n:Person) RETURN count(*) AS count"
    },
    @{
        name = "count(variable) with MATCH"
        cypher = "MATCH (n:Person) RETURN count(n) AS count"
    },
    @{
        name = "sum() with MATCH"
        cypher = "MATCH (n:Person) RETURN sum(n.age) AS total_age"
    },
    @{
        name = "avg() with MATCH"
        cypher = "MATCH (n:Person) RETURN avg(n.age) AS avg_age"
    },
    @{
        name = "min() with MATCH"
        cypher = "MATCH (n:Person) RETURN min(n.age) AS min_age"
    },
    @{
        name = "max() with MATCH"
        cypher = "MATCH (n:Person) RETURN max(n.age) AS max_age"
    },
    @{
        name = "WHERE with IN operator"
        cypher = "MATCH (n:Person) WHERE n.name IN ['Alice', 'Bob'] RETURN count(n) AS count"
    },
    @{
        name = "WHERE with AND operator"
        cypher = "MATCH (n:Person) WHERE n.age > 25 AND n.city = 'NYC' RETURN count(n) AS count"
    },
    @{
        name = "WHERE with OR operator"
        cypher = "MATCH (n:Person) WHERE n.age > 30 OR n.city = 'LA' RETURN count(n) AS count"
    },
    @{
        name = "WHERE with IS NULL"
        cypher = "MATCH (n:Person) WHERE n.city IS NULL RETURN count(n) AS count"
    },
    @{
        name = "WHERE with IS NOT NULL"
        cypher = "MATCH (n:Person) WHERE n.city IS NOT NULL RETURN count(n) AS count"
    },
    @{
        name = "null arithmetic"
        cypher = "RETURN null + 5 AS result"
    },
    @{
        name = "coalesce function"
        cypher = "RETURN coalesce(null, 42) AS result"
    },
    @{
        name = "reverse function"
        cypher = "RETURN reverse([1, 2, 3]) AS reversed"
    },
    @{
        name = "replace function"
        cypher = "RETURN replace('hello', 'l', 'L') AS replaced"
    },
    @{
        name = "trim function"
        cypher = "RETURN trim('  hello  ') AS trimmed"
    }
)

$passed = 0
$failed = 0
$skipped = 0

Write-Host "Running compatibility tests..." -ForegroundColor Yellow
Write-Host ""

foreach ($test in $testQueries) {
    Write-Host "Testing: $($test.name)" -ForegroundColor Cyan
    Write-Host "  Query: $($test.cypher)"
    
    $neo4jResult = Invoke-Neo4jQuery -Cypher $test.cypher
    $nexusResult = Invoke-NexusQuery -Cypher $test.cypher
    
    if ($null -eq $neo4jResult) {
        Write-Host "  [SKIP] Neo4j query failed" -ForegroundColor Yellow
        $skipped++
        Write-Host ""
        continue
    }
    
    if ($null -eq $nexusResult) {
        Write-Host "  [FAIL] Nexus query failed" -ForegroundColor Red
        $failed++
        Write-Host ""
        continue
    }
    
    # Extract values
    $neo4jValue = $null
    $nexusValue = $null
    
    if ($neo4jResult.data -and $neo4jResult.data.Count -gt 0) {
        $neo4jValue = $neo4jResult.data[0].row[0]
    }
    
    if ($nexusResult.rows -and $nexusResult.rows.Count -gt 0) {
        if ($nexusResult.rows[0] -is [array]) {
            $nexusValue = $nexusResult.rows[0][0]
        } else {
            $nexusValue = $nexusResult.rows[0].values[0]
        }
    }
    
    Write-Host "  Neo4j: $neo4jValue"
    Write-Host "  Nexus: $nexusValue"
    
    if (Compare-Values $neo4jValue $nexusValue) {
        Write-Host "  [PASS] Results match!" -ForegroundColor Green
        $passed++
    } else {
        Write-Host "  [FAIL] Results don't match!" -ForegroundColor Red
        $failed++
    }
    Write-Host ""
}

Write-Host "========================================" -ForegroundColor Cyan
Write-Host "EXTENDED COMPATIBILITY TEST SUMMARY" -ForegroundColor Cyan
Write-Host "========================================" -ForegroundColor Cyan
Write-Host "Total Tests: $($testQueries.Count)"
Write-Host "Passed: $passed" -ForegroundColor Green
Write-Host "Failed: $failed" -ForegroundColor Red
Write-Host "Skipped: $skipped" -ForegroundColor Yellow

$passRate = if ($testQueries.Count -gt 0) { 
    [math]::Round(($passed / $testQueries.Count) * 100, 2) 
} else { 
    0 
}
Write-Host ""
Write-Host "Pass Rate: $passRate%" -ForegroundColor $(if ($passRate -ge 90) { "Green" } elseif ($passRate -ge 70) { "Yellow" } else { "Red" })

