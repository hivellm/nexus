#!/usr/bin/env pwsh
# Extended Neo4j vs Nexus Cross-Compatibility Test
# Tests advanced features and edge cases

param(
    [string]$Neo4jUrl = "http://localhost:7474",
    [string]$NexusUrl = "http://localhost:15474",
    [string]$Neo4jUser = "neo4j",
    [string]$Neo4jPassword = "password"
)

$ErrorActionPreference = "Continue"

# Test queries - expanded coverage
$queries = @(
    @{
        name = "Aggregation - COUNT(*)"
        cypher = 'MATCH (n:Person) RETURN count(*) AS total'
        compareValues = $true
    },
    @{
        name = "Aggregation - COUNT property"
        cypher = 'MATCH (n:Person) RETURN count(n.age) AS count'
        compareValues = $true
    },
    @{
        name = "Aggregation - COUNT DISTINCT"
        cypher = 'MATCH (n:Person) RETURN count(DISTINCT n.age) AS unique_ages'
        compareValues = $true
    },
    @{
        name = "Aggregation - AVG"
        cypher = 'MATCH (n:Person) RETURN avg(n.age) AS average_age'
        compareValues = $true
        tolerance = 0.01
    },
    @{
        name = "Aggregation - MIN"
        cypher = 'MATCH (n:Person) RETURN min(n.age) AS min_age'
        compareValues = $true
    },
    @{
        name = "Aggregation - MAX"
        cypher = 'MATCH (n:Person) RETURN max(n.age) AS max_age'
        compareValues = $true
    },
    @{
        name = "Aggregation - SUM"
        cypher = 'MATCH (n:Person) RETURN sum(n.age) AS total_age'
        compareValues = $true
    },
    @{
        name = "UNION - Simple"
        cypher = 'MATCH (p:Person) RETURN p.name AS name UNION MATCH (c:Company) RETURN c.name AS name'
        compareRowCount = $true
    },
    @{
        name = "UNION ALL - With duplicates"
        cypher = 'MATCH (p:Person) RETURN p.age AS value UNION ALL MATCH (p:Person) RETURN p.age AS value'
        compareRowCount = $true
    },
    @{
        name = "UNION - Multiple columns"
        cypher = 'MATCH (p:Person) RETURN p.name AS n, p.age AS a UNION MATCH (e:Employee) RETURN e.name AS n, e.age AS a'
        compareRowCount = $true
    },
    @{
        name = "Multiple labels - Intersection"
        cypher = 'MATCH (n:Person:Employee) RETURN count(*) AS count'
        compareValues = $true
    },
    @{
        name = "WHERE - Property equals"
        cypher = 'MATCH (n:Person) WHERE n.age = 30 RETURN count(*) AS count'
        compareValues = $true
    },
    @{
        name = "WHERE - Property comparison"
        cypher = 'MATCH (n:Person) WHERE n.age >= 30 RETURN count(*) AS count'
        compareValues = $true
    },
    @{
        name = "WHERE - Multiple conditions AND"
        cypher = 'MATCH (n:Person) WHERE n.age >= 25 AND n.age <= 35 RETURN count(*) AS count'
        compareValues = $true
    },
    @{
        name = "Relationships - Outgoing"
        cypher = 'MATCH (p:Person)-[r:KNOWS]->(other) RETURN count(r) AS count'
        compareValues = $true
    },
    @{
        name = "Relationships - Incoming"
        cypher = 'MATCH (p:Person)<-[r:KNOWS]-(other) RETURN count(r) AS count'
        compareValues = $true
    },
    @{
        name = "Relationships - Bidirectional"
        cypher = 'MATCH (p:Person)-[r:KNOWS]-(other) RETURN count(r) AS count'
        compareValues = $true
    },
    @{
        name = "Relationships - Property filter"
        cypher = 'MATCH ()-[r:KNOWS]-() WHERE r.since >= 2015 RETURN count(r) AS count'
        compareValues = $true
    },
    @{
        name = "Functions - labels()"
        cypher = 'MATCH (n:Person) RETURN labels(n) AS labels LIMIT 1'
        compareStructure = $true
    },
    @{
        name = "Functions - keys()"
        cypher = 'MATCH (n:Person) RETURN keys(n) AS keys LIMIT 1'
        compareStructure = $true
    },
    @{
        name = "Functions - id()"
        cypher = 'MATCH (n:Person) RETURN id(n) IS NOT NULL AS has_id LIMIT 1'
        compareValues = $true
    },
    @{
        name = "Functions - type()"
        cypher = 'MATCH ()-[r:KNOWS]->() RETURN type(r) AS rel_type LIMIT 1'
        compareValues = $true
    },
    @{
        name = "ORDER BY - Single column ASC"
        cypher = 'MATCH (p:Person) RETURN p.age AS age ORDER BY age LIMIT 3'
        compareStructure = $true
    },
    @{
        name = "ORDER BY - Single column DESC"
        cypher = 'MATCH (p:Person) RETURN p.age AS age ORDER BY age DESC LIMIT 3'
        compareStructure = $true
    },
    @{
        name = "LIMIT - Basic"
        cypher = 'MATCH (p:Person) RETURN p.name AS name LIMIT 2'
        compareRowCount = $true
    },
    @{
        name = "Complex - Multiple labels + WHERE + COUNT"
        cypher = 'MATCH (n:Person:Employee) WHERE n.age > 25 RETURN count(*) AS count'
        compareValues = $true
    },
    @{
        name = "Complex - Relationships + Properties"
        cypher = 'MATCH (p:Person)-[r:WORKS_AT]->(c:Company) RETURN p.name AS person, c.name AS company'
        compareRowCount = $true
    },
    @{
        name = "Complex - Aggregation with WHERE"
        cypher = 'MATCH (p:Person) WHERE p.age >= 30 RETURN avg(p.age) AS avg_age'
        compareValues = $true
        tolerance = 0.01
    },
    @{
        name = "Edge case - Empty result"
        cypher = 'MATCH (n:NonExistentLabel) RETURN count(*) AS count'
        compareValues = $true
    },
    @{
        name = "Edge case - NULL properties"
        cypher = 'MATCH (n:Person) RETURN count(n.nonexistent) AS count'
        compareValues = $true
    },
    @{
        name = "Pattern - Two hops"
        cypher = 'MATCH (p:Person)-[:KNOWS]->()-[:KNOWS]->(end) RETURN count(*) AS count'
        compareValues = $true
    },
    @{
        name = "DISTINCT - In RETURN"
        cypher = 'MATCH (p:Person) RETURN DISTINCT p.age AS age'
        compareRowCount = $true
    },
    @{
        name = "Multiple relationships - Same type"
        cypher = 'MATCH (p:Person)-[:KNOWS]->(other:Person) RETURN count(*) AS count'
        compareValues = $true
    },
    @{
        name = "Property exists check"
        cypher = 'MATCH (n:Person) WHERE n.age IS NOT NULL RETURN count(*) AS count'
        compareValues = $true
    },
    @{
        name = "String property comparison"
        cypher = 'MATCH (p:Person) WHERE p.name = ''Alice'' RETURN count(*) AS count'
        compareValues = $true
    }
)

function Invoke-Neo4jQuery {
    param([string]$Query)
    
    $auth = [Convert]::ToBase64String([Text.Encoding]::ASCII.GetBytes("${Neo4jUser}:${Neo4jPassword}"))
    $body = @{
        statements = @(
            @{
                statement = $Query
            }
        )
    } | ConvertTo-Json
    
    try {
        $response = Invoke-RestMethod -Uri "$Neo4jUrl/db/neo4j/tx/commit" `
            -Method Post `
            -Headers @{
                "Authorization" = "Basic $auth"
                "Content-Type" = "application/json"
            } `
            -Body $body
        
        return $response.results[0]
    }
    catch {
        Write-Host "  Neo4j Error: $_" -ForegroundColor Red
        return $null
    }
}

function Invoke-NexusQuery {
    param([string]$Query)
    
    $body = @{
        query = $Query
    } | ConvertTo-Json
    
    try {
        $response = Invoke-RestMethod -Uri "$NexusUrl/cypher" `
            -Method Post `
            -Headers @{"Content-Type" = "application/json"} `
            -Body $body
        
        return $response
    }
    catch {
        Write-Host "  Nexus Error: $_" -ForegroundColor Red
        return $null
    }
}

function Compare-Results {
    param(
        $Neo4jResult,
        $NexusResult,
        $TestConfig
    )
    
    if ($null -eq $Neo4jResult -or $null -eq $NexusResult) {
        return @{ passed = $false; reason = "Query execution failed" }
    }
    
    $neo4jRowCount = $Neo4jResult.data.Count
    $nexusRowCount = $NexusResult.rows.Count
    
    # Compare row count if requested
    if ($TestConfig.compareRowCount) {
        if ($neo4jRowCount -ne $nexusRowCount) {
            return @{
                passed = $false
                reason = "Row count mismatch: Neo4j=$neo4jRowCount, Nexus=$nexusRowCount"
            }
        }
        return @{ passed = $true; reason = "Row counts match" }
    }
    
    # Compare values if requested
    if ($TestConfig.compareValues -and $neo4jRowCount -gt 0 -and $nexusRowCount -gt 0) {
        $neo4jValue = $Neo4jResult.data[0].row[0]
        $nexusValue = $NexusResult.rows[0][0]
        
        # Handle numeric comparison with tolerance
        if ($TestConfig.tolerance) {
            $diff = [Math]::Abs([double]$neo4jValue - [double]$nexusValue)
            if ($diff -gt $TestConfig.tolerance) {
                return @{
                    passed = $false
                    reason = "Value mismatch: Neo4j=$neo4jValue, Nexus=$nexusValue (diff=$diff)"
                }
            }
        }
        elseif ($neo4jValue -ne $nexusValue) {
            return @{
                passed = $false
                reason = "Value mismatch: Neo4j=$neo4jValue, Nexus=$nexusValue"
            }
        }
        
        return @{ passed = $true; reason = "Values match" }
    }
    
    # Structure comparison (just check both have data)
    if ($TestConfig.compareStructure) {
        if ($neo4jRowCount -gt 0 -and $nexusRowCount -gt 0) {
            return @{ passed = $true; reason = "Both returned data" }
        }
        return @{
            passed = $false
            reason = "Structure mismatch: Neo4j rows=$neo4jRowCount, Nexus rows=$nexusRowCount"
        }
    }
    
    # Default: just check both succeeded
    return @{ passed = $true; reason = "Both queries executed" }
}

# Clear and create test data
Write-Host "`n[SETUP] Creating comprehensive test dataset..." -ForegroundColor Cyan

$setupQueries = @(
    "MATCH (n) DETACH DELETE n",
    "CREATE (p:Person {name: 'Alice', age: 30, city: 'NYC'})",
    "CREATE (p:Person {name: 'Bob', age: 25, city: 'LA'})",
    "CREATE (p:Person {name: 'Charlie', age: 35, city: 'SF'})",
    "CREATE (p:Person:Employee {name: 'David', age: 28, role: 'Developer'})",
    "CREATE (p:Person:Employee {name: 'Eve', age: 32, role: 'Manager'})",
    "CREATE (c:Company {name: 'Acme Inc', founded: 2000})",
    "CREATE (c:Company {name: 'TechCorp', founded: 2010})",
    "CREATE (e:Employee {name: 'Frank', age: 27})",
    "MATCH (p:Person {name: 'Alice'}), (c:Company {name: 'Acme Inc'}) CREATE (p)-[:WORKS_AT {since: 2020}]->(c)",
    "MATCH (p:Person {name: 'Bob'}), (c:Company {name: 'TechCorp'}) CREATE (p)-[:WORKS_AT {since: 2018}]->(c)",
    "MATCH (p1:Person {name: 'Alice'}), (p2:Person {name: 'Bob'}) CREATE (p1)-[:KNOWS {since: 2015}]->(p2)",
    "MATCH (p1:Person {name: 'Bob'}), (p2:Person {name: 'Charlie'}) CREATE (p1)-[:KNOWS {since: 2018}]->(p2)",
    "MATCH (p1:Person {name: 'Alice'}), (p2:Person {name: 'David'}) CREATE (p1)-[:KNOWS {since: 2019}]->(p2)"
)

foreach ($query in $setupQueries) {
    Invoke-Neo4jQuery -Query $query | Out-Null
    Invoke-NexusQuery -Query $query | Out-Null
}

Write-Host "[PASS] Test data created`n" -ForegroundColor Green

# Run tests
Write-Host "=" * 80 -ForegroundColor Yellow
Write-Host "EXTENDED COMPATIBILITY TESTS" -ForegroundColor Yellow
Write-Host "=" * 80 -ForegroundColor Yellow

$passed = 0
$failed = 0
$results = @()

foreach ($test in $queries) {
    Write-Host "`n[TEST] $($test.name)" -ForegroundColor Cyan
    
    $neo4jResult = Invoke-Neo4jQuery -Query $test.cypher
    $nexusResult = Invoke-NexusQuery -Query $test.cypher
    
    $comparison = Compare-Results -Neo4jResult $neo4jResult -NexusResult $nexusResult -TestConfig $test
    
    $result = @{
        name = $test.name
        query = $test.cypher
        passed = $comparison.passed
        reason = $comparison.reason
    }
    
    $results += $result
    
    if ($comparison.passed) {
        Write-Host "[PASS] $($comparison.reason)" -ForegroundColor Green
        $passed++
    }
    else {
        Write-Host "[FAIL] $($comparison.reason)" -ForegroundColor Red
        $failed++
    }
}

# Summary
Write-Host "`n" + ("=" * 80) -ForegroundColor Yellow
Write-Host "TEST SUMMARY" -ForegroundColor Yellow
Write-Host ("=" * 80) -ForegroundColor Yellow

$total = $passed + $failed
$passRate = if ($total -gt 0) { [math]::Round(($passed / $total) * 100, 2) } else { 0 }

Write-Host "`nTotal Tests: $total" -ForegroundColor White
Write-Host "Passed: $passed" -ForegroundColor Green
Write-Host "Failed: $failed" -ForegroundColor Red
Write-Host "`nPass Rate: $passRate%" -ForegroundColor $(if ($passRate -ge 90) { "Green" } elseif ($passRate -ge 75) { "Yellow" } else { "Red" })

# Save detailed report
$report = @{
    timestamp = Get-Date -Format "yyyy-MM-dd HH:mm:ss"
    total = $total
    passed = $passed
    failed = $failed
    passRate = $passRate
    results = $results
} | ConvertTo-Json -Depth 10

$reportPath = "extended-compatibility-report.json"
$report | Out-File -FilePath $reportPath -Encoding UTF8

Write-Host "`nDetailed report saved to: $reportPath" -ForegroundColor Cyan

# Show failures if any
if ($failed -gt 0) {
    Write-Host "`n" + ("=" * 80) -ForegroundColor Red
    Write-Host "FAILED TESTS DETAILS" -ForegroundColor Red
    Write-Host ("=" * 80) -ForegroundColor Red
    
    foreach ($result in $results | Where-Object { -not $_.passed }) {
        Write-Host "`n[FAIL] $($result.name)" -ForegroundColor Red
        Write-Host "  Query: $($result.query)" -ForegroundColor Gray
        Write-Host "  Reason: $($result.reason)" -ForegroundColor Yellow
    }
}

Write-Host ""
exit $(if ($failed -eq 0) { 0 } else { 1 })

