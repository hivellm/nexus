# Comprehensive Neo4j Compatibility Test Suite
# Tests all implemented features with extensive edge cases and complex scenarios

param(
    [string]$Neo4jUri = "http://localhost:7474",
    [string]$Neo4jUser = "neo4j",
    [string]$Neo4jPassword = "password",
    [string]$NexusUri = "http://localhost:15474"
)

Write-Host "=== Comprehensive Neo4j Compatibility Test Suite ===" -ForegroundColor Cyan
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
    
    if ($Val1 -is [long] -and $Val2 -is [double]) {
        return [math]::Abs($Val1 - $Val2) -lt 0.0001
    }
    
    if ($Val1 -is [double] -and $Val2 -is [long]) {
        return [math]::Abs($Val1 - $Val2) -lt 0.0001
    }
    
    return $Val1 -eq $Val2
}

# Clear databases first
Write-Host "Clearing databases..." -ForegroundColor Yellow
Invoke-Neo4jQuery -Cypher "MATCH (n) DETACH DELETE n" | Out-Null
Invoke-NexusQuery -Cypher "MATCH (n) DETACH DELETE n" | Out-Null

# Setup comprehensive test data
Write-Host "Setting up comprehensive test data..." -ForegroundColor Yellow
$setupQueries = @(
    # Basic nodes
    "CREATE (p1:Person {name: 'Alice', age: 30, city: 'NYC', salary: 50000})",
    "CREATE (p2:Person {name: 'Bob', age: 25, city: 'LA', salary: 40000})",
    "CREATE (p3:Person {name: 'Charlie', age: 35, city: 'NYC', salary: 60000})",
    "CREATE (p4:Person {name: 'David', age: 28})",
    "CREATE (p5:Person:Employee {name: 'Eve', age: 32, city: 'SF', salary: 55000, role: 'Developer'})",
    "CREATE (p6:Person:Employee {name: 'Frank', age: 40, city: 'NYC', salary: 70000, role: 'Manager'})",
    
    # Companies
    "CREATE (c1:Company {name: 'Acme Inc', founded: 2000})",
    "CREATE (c2:Company {name: 'TechCorp', founded: 2010})",
    
    # Relationships
    "MATCH (p1:Person {name: 'Alice'}), (c1:Company {name: 'Acme Inc'}) CREATE (p1)-[:WORKS_AT {since: 2020, role: 'Engineer'}]->(c1)",
    "MATCH (p2:Person {name: 'Bob'}), (c1:Company {name: 'Acme Inc'}) CREATE (p2)-[:WORKS_AT {since: 2021, role: 'Designer'}]->(c1)",
    "MATCH (p3:Person {name: 'Charlie'}), (c2:Company {name: 'TechCorp'}) CREATE (p3)-[:WORKS_AT {since: 2019, role: 'Manager'}]->(c2)",
    "MATCH (p1:Person {name: 'Alice'}), (p2:Person {name: 'Bob'}) CREATE (p1)-[:KNOWS {since: 2015}]->(p2)",
    "MATCH (p2:Person {name: 'Bob'}), (p3:Person {name: 'Charlie'}) CREATE (p2)-[:KNOWS {since: 2018}]->(p3)",
    "MATCH (p5:Person {name: 'Eve'}), (p6:Person {name: 'Frank'}) CREATE (p5)-[:REPORTS_TO {since: 2022}]->(p6)"
)

foreach ($query in $setupQueries) {
    Invoke-Neo4jQuery -Cypher $query | Out-Null
    Invoke-NexusQuery -Cypher $query | Out-Null
}
Write-Host "Test data created" -ForegroundColor Green
Write-Host ""

# Comprehensive test queries organized by category
$testQueries = @()

# ===== AGGREGATION FUNCTIONS =====
$testQueries += @(
    @{ name = "count(*) without MATCH"; cypher = "RETURN count(*) AS count" },
    @{ name = "count(*) with MATCH"; cypher = "MATCH (n:Person) RETURN count(*) AS count" },
    @{ name = "count(variable) with MATCH"; cypher = "MATCH (n:Person) RETURN count(n) AS count" },
    @{ name = "count(*) with empty MATCH"; cypher = "MATCH (n:NonExistent) RETURN count(*) AS count" },
    @{ name = "sum() without MATCH"; cypher = "RETURN sum(1) AS sum_val" },
    @{ name = "sum() with MATCH"; cypher = "MATCH (n:Person) RETURN sum(n.age) AS total_age" },
    @{ name = "sum() with null values"; cypher = "MATCH (n:Person) RETURN sum(n.salary) AS total_salary" },
    @{ name = "avg() without MATCH"; cypher = "RETURN avg(10) AS avg_val" },
    @{ name = "avg() with MATCH"; cypher = "MATCH (n:Person) RETURN avg(n.age) AS avg_age" },
    @{ name = "avg() with null values"; cypher = "MATCH (n:Person) RETURN avg(n.salary) AS avg_salary" },
    @{ name = "min() without MATCH"; cypher = "RETURN min(5) AS min_val" },
    @{ name = "min() with MATCH"; cypher = "MATCH (n:Person) RETURN min(n.age) AS min_age" },
    @{ name = "min() with null values"; cypher = "MATCH (n:Person) RETURN min(n.salary) AS min_salary" },
    @{ name = "max() without MATCH"; cypher = "RETURN max(15) AS max_val" },
    @{ name = "max() with MATCH"; cypher = "MATCH (n:Person) RETURN max(n.age) AS max_age" },
    @{ name = "max() with null values"; cypher = "MATCH (n:Person) RETURN max(n.salary) AS max_salary" },
    @{ name = "collect() without MATCH"; cypher = "RETURN collect(1) AS collected" },
    @{ name = "collect() with MATCH"; cypher = "MATCH (n:Person) RETURN collect(n.name) AS names" },
    @{ name = "collect() with null values"; cypher = "MATCH (n:Person) RETURN collect(n.city) AS cities" }
)

# ===== WHERE CLAUSE OPERATORS =====
$testQueries += @(
    @{ name = "WHERE with IN operator (strings)"; cypher = "MATCH (n:Person) WHERE n.name IN ['Alice', 'Bob'] RETURN count(n) AS count" },
    @{ name = "WHERE with IN operator (numbers)"; cypher = "MATCH (n:Person) WHERE n.age IN [25, 30, 35] RETURN count(n) AS count" },
    @{ name = "WHERE with IN operator (empty list)"; cypher = "MATCH (n:Person) WHERE n.name IN [] RETURN count(n) AS count" },
    @{ name = "WHERE with NOT IN operator"; cypher = "MATCH (n:Person) WHERE n.name NOT IN ['Alice'] RETURN count(n) AS count" },
    @{ name = "WHERE with IS NULL"; cypher = "MATCH (n:Person) WHERE n.city IS NULL RETURN count(n) AS count" },
    @{ name = "WHERE with IS NOT NULL"; cypher = "MATCH (n:Person) WHERE n.city IS NOT NULL RETURN count(n) AS count" },
    @{ name = "WHERE with AND operator"; cypher = "MATCH (n:Person) WHERE n.age > 25 AND n.city = 'NYC' RETURN count(n) AS count" },
    @{ name = "WHERE with OR operator"; cypher = "MATCH (n:Person) WHERE n.age > 30 OR n.city = 'LA' RETURN count(n) AS count" },
    @{ name = "WHERE with NOT operator"; cypher = "MATCH (n:Person) WHERE NOT (n.age < 30) RETURN count(n) AS count" },
    @{ name = "WHERE with complex AND/OR"; cypher = "MATCH (n:Person) WHERE (n.age > 30 AND n.city = 'NYC') OR n.name = 'Bob' RETURN count(n) AS count" },
    @{ name = "WHERE with comparison operators"; cypher = "MATCH (n:Person) WHERE n.age >= 30 RETURN count(n) AS count" },
    @{ name = "WHERE with <> operator"; cypher = "MATCH (n:Person) WHERE n.age <> 25 RETURN count(n) AS count" }
)

# ===== MATHEMATICAL OPERATORS =====
$testQueries += @(
    @{ name = "Power operator (2^3)"; cypher = "RETURN 2 ^ 3 AS power" },
    @{ name = "Power operator (10^2)"; cypher = "RETURN 10 ^ 2 AS power" },
    @{ name = "Power operator with null"; cypher = "RETURN null ^ 2 AS power" },
    @{ name = "Modulo operator (10%3)"; cypher = "RETURN 10 % 3 AS mod" },
    @{ name = "Modulo operator (15%4)"; cypher = "RETURN 15 % 4 AS mod" },
    @{ name = "Modulo operator with null"; cypher = "RETURN null % 3 AS mod" },
    @{ name = "Power in WHERE clause"; cypher = "MATCH (n:Person) WHERE n.age = 2.0 ^ 5.0 RETURN count(n) AS count" },
    @{ name = "Modulo in WHERE clause"; cypher = "MATCH (n:Person) WHERE n.age % 5 = 0 RETURN count(n) AS count" },
    @{ name = "Complex arithmetic expression"; cypher = "RETURN (10 + 5) * 2 ^ 2 AS result" },
    @{ name = "Arithmetic with null"; cypher = "RETURN null + 5 AS result" },
    @{ name = "Arithmetic null + null"; cypher = "RETURN null + null AS result" }
)

# ===== STRING FUNCTIONS =====
$testQueries += @(
    @{ name = "substring basic"; cypher = "RETURN substring('hello', 1, 3) AS substr" },
    @{ name = "substring full length"; cypher = "RETURN substring('hello', 0, 5) AS substr" },
    @{ name = "substring with MATCH"; cypher = "MATCH (n:Person {name: 'Alice'}) RETURN substring(n.name, 0, 3) AS substr" },
    @{ name = "replace basic"; cypher = "RETURN replace('hello', 'l', 'L') AS replaced" },
    @{ name = "replace multiple occurrences"; cypher = "RETURN replace('hello world', 'o', 'O') AS replaced" },
    @{ name = "replace with MATCH"; cypher = "MATCH (n:Person {name: 'Alice'}) RETURN replace(n.name, 'A', 'a') AS replaced" },
    @{ name = "trim basic"; cypher = "RETURN trim('  hello  ') AS trimmed" },
    @{ name = "trim with tabs"; cypher = "RETURN trim('	hello	') AS trimmed" },
    @{ name = "trim empty string"; cypher = "RETURN trim('') AS trimmed" },
    @{ name = "trim with MATCH"; cypher = "MATCH (n:Person {name: 'Alice'}) RETURN trim(concat('  ', n.name, '  ')) AS trimmed" }
)

# ===== LIST OPERATIONS =====
$testQueries += @(
    @{ name = "tail basic"; cypher = "RETURN tail([1, 2, 3, 4]) AS tail" },
    @{ name = "tail single element"; cypher = "RETURN tail([1]) AS tail" },
    @{ name = "tail empty list"; cypher = "RETURN tail([]) AS tail" },
    @{ name = "reverse basic"; cypher = "RETURN reverse([1, 2, 3]) AS reversed" },
    @{ name = "reverse strings"; cypher = "RETURN reverse(['a', 'b', 'c']) AS reversed" },
    @{ name = "reverse empty list"; cypher = "RETURN reverse([]) AS reversed" },
    @{ name = "collect with tail"; cypher = "MATCH (n:Person) RETURN tail(collect(n.name)) AS names" },
    @{ name = "collect with reverse"; cypher = "MATCH (n:Person) RETURN reverse(collect(n.age)) AS ages" }
)

# ===== NULL HANDLING =====
$testQueries += @(
    @{ name = "null = null in RETURN"; cypher = "RETURN null = null AS null_eq" },
    @{ name = "null <> null in RETURN"; cypher = "RETURN null <> null AS null_ne" },
    @{ name = "null = value in RETURN"; cypher = "RETURN null = 5 AS null_eq_val" },
    @{ name = "coalesce with null first"; cypher = "RETURN coalesce(null, 42) AS result" },
    @{ name = "coalesce with value first"; cypher = "RETURN coalesce(10, 42) AS result" },
    @{ name = "coalesce multiple nulls"; cypher = "RETURN coalesce(null, null, 42) AS result" },
    @{ name = "coalesce with MATCH"; cypher = "MATCH (n:Person {name: 'David'}) RETURN coalesce(n.city, 'Unknown') AS city" },
    @{ name = "null arithmetic addition"; cypher = "RETURN null + 5 AS result" },
    @{ name = "null arithmetic multiplication"; cypher = "RETURN null * 5 AS result" },
    @{ name = "null arithmetic division"; cypher = "RETURN null / 5 AS result" }
)

# ===== LOGICAL OPERATORS =====
$testQueries += @(
    @{ name = "AND operator true AND true"; cypher = "RETURN (5 > 3 AND 2 < 4) AS and_result" },
    @{ name = "AND operator true AND false"; cypher = "RETURN (5 > 3 AND 2 > 4) AS and_result" },
    @{ name = "OR operator true OR false"; cypher = "RETURN (5 > 3 OR 2 > 4) AS or_result" },
    @{ name = "OR operator false OR false"; cypher = "RETURN (5 < 3 OR 2 > 4) AS or_result" },
    @{ name = "NOT operator"; cypher = "RETURN NOT (5 < 3) AS not_result" },
    @{ name = "NOT with parentheses"; cypher = "RETURN NOT (5 > 3) AS not_result" },
    @{ name = "Complex logical expression"; cypher = "RETURN (5 > 3 AND 2 < 4) OR (1 > 2) AS complex" }
)

# ===== COMPLEX QUERIES =====
$testQueries += @(
    @{ name = "Multiple aggregations"; cypher = "MATCH (n:Person) RETURN count(n) AS count, sum(n.age) AS total_age, avg(n.age) AS avg_age" },
    @{ name = "Aggregation with WHERE"; cypher = "MATCH (n:Person) WHERE n.age > 28 RETURN avg(n.age) AS avg_age" },
    @{ name = "Aggregation with multiple labels"; cypher = "MATCH (n:Person:Employee) RETURN count(n) AS count" },
    @{ name = "WHERE with string functions"; cypher = "MATCH (n:Person) WHERE substring(n.name, 0, 1) = 'A' RETURN count(n) AS count" },
    @{ name = "WHERE with mathematical operators"; cypher = "MATCH (n:Person) WHERE n.age % 5 = 0 RETURN count(n) AS count" },
    @{ name = "RETURN with multiple expressions"; cypher = "MATCH (n:Person {name: 'Alice'}) RETURN n.name, n.age, n.city, coalesce(n.salary, 0) AS salary" },
    @{ name = "Complex WHERE with AND/OR/NOT"; cypher = "MATCH (n:Person) WHERE (n.age > 30 AND n.city = 'NYC') OR (NOT n.city IS NULL AND n.age < 25) RETURN count(n) AS count" }
)

# ===== RELATIONSHIP QUERIES =====
$testQueries += @(
    @{ name = "Count relationships"; cypher = "MATCH ()-[r:KNOWS]->() RETURN count(r) AS count" },
    @{ name = "Count all relationships"; cypher = "MATCH ()-[r]->() RETURN count(r) AS count" },
    @{ name = "Relationship properties"; cypher = "MATCH (a)-[r:KNOWS]->(b) RETURN count(r) AS count" },
    @{ name = "Relationship with WHERE"; cypher = "MATCH (a)-[r:WORKS_AT]->(b) WHERE r.since > 2020 RETURN count(r) AS count" },
    @{ name = "Bidirectional relationships"; cypher = "MATCH (a)-[r:KNOWS]-(b) RETURN count(r) AS count" }
)

Write-Host "Running $($testQueries.Count) comprehensive compatibility tests..." -ForegroundColor Yellow
Write-Host ""

$passed = 0
$failed = 0
$skipped = 0
$results = @()

foreach ($test in $testQueries) {
    Write-Host "Testing: $($test.name)" -ForegroundColor Cyan
    Write-Host "  Query: $($test.cypher)"
    
    $neo4jResult = Invoke-Neo4jQuery -Cypher $test.cypher
    $nexusResult = Invoke-NexusQuery -Cypher $test.cypher
    
    if ($null -eq $neo4jResult) {
        Write-Host "  [SKIP] Neo4j query failed" -ForegroundColor Yellow
        $skipped++
        $results += @{ name = $test.name; status = "skipped"; query = $test.cypher }
        Write-Host ""
        continue
    }
    
    if ($null -eq $nexusResult) {
        Write-Host "  [FAIL] Nexus query failed" -ForegroundColor Red
        $failed++
        $results += @{ name = $test.name; status = "failed"; query = $test.cypher; reason = "Nexus query failed" }
        Write-Host ""
        continue
    }
    
    # Extract values - handle multiple columns
    $neo4jValues = @()
    $nexusValues = @()
    
    if ($neo4jResult.data -and $neo4jResult.data.Count -gt 0) {
        $neo4jValues = $neo4jResult.data[0].row
    }
    
    if ($nexusResult.rows -and $nexusResult.rows.Count -gt 0) {
        if ($nexusResult.rows[0] -is [array]) {
            $nexusValues = $nexusResult.rows[0]
        } else {
            $nexusValues = $nexusResult.rows[0].values
        }
    }
    
    # Compare row counts first
    if ($neo4jResult.data.Count -ne $nexusResult.rows.Count) {
        Write-Host "  Neo4j rows: $($neo4jResult.data.Count) | Nexus rows: $($nexusResult.rows.Count)" -ForegroundColor Yellow
        Write-Host "  [FAIL] Row count mismatch!" -ForegroundColor Red
        $failed++
        $results += @{ name = $test.name; status = "failed"; query = $test.cypher; reason = "Row count mismatch" }
        Write-Host ""
        continue
    }
    
    # Compare values
    $match = $true
    if ($neo4jValues.Count -eq $nexusValues.Count) {
        for ($i = 0; $i -lt $neo4jValues.Count; $i++) {
            if (-not (Compare-Values $neo4jValues[$i] $nexusValues[$i])) {
                $match = $false
                break
            }
        }
    } else {
        $match = $false
    }
    
    if ($match) {
        Write-Host "  [PASS] Results match!" -ForegroundColor Green
        $passed++
        $results += @{ name = $test.name; status = "passed"; query = $test.cypher }
    } else {
        Write-Host "  Neo4j: $($neo4jValues -join ', ')" -ForegroundColor Yellow
        Write-Host "  Nexus: $($nexusValues -join ', ')" -ForegroundColor Yellow
        Write-Host "  [FAIL] Results don't match!" -ForegroundColor Red
        $failed++
        $results += @{ name = $test.name; status = "failed"; query = $test.cypher; reason = "Value mismatch" }
    }
    Write-Host ""
}

Write-Host "========================================" -ForegroundColor Cyan
Write-Host "COMPREHENSIVE COMPATIBILITY TEST SUMMARY" -ForegroundColor Cyan
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

# Export detailed results
$reportDir = Join-Path $PSScriptRoot ".." "tests" "cross-compatibility" "reports"
if (-not (Test-Path $reportDir)) {
    New-Item -ItemType Directory -Path $reportDir -Force | Out-Null
}
$reportPath = Join-Path $reportDir "neo4j-comprehensive-compatibility-report.json"
$results | ConvertTo-Json -Depth 3 | Out-File $reportPath
Write-Host ""
Write-Host "Detailed report saved to: $reportPath" -ForegroundColor Cyan

# Show failed tests summary
if ($failed -gt 0) {
    Write-Host ""
    Write-Host "Failed Tests:" -ForegroundColor Red
    $results | Where-Object { $_.status -eq "failed" } | ForEach-Object {
        Write-Host "  - $($_.name): $($_.reason)" -ForegroundColor Red
    }
}

