# Advanced Neo4j Compatibility Test Suite
# Additional edge cases, advanced functions, and complex scenarios

param(
    [string]$Neo4jUri = "http://localhost:7474",
    [string]$Neo4jUser = "neo4j",
    [string]$Neo4jPassword = "password",
    [string]$NexusUri = "http://localhost:15474"
)

Write-Host "=== Advanced Neo4j Compatibility Test Suite ===" -ForegroundColor Cyan
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

# Setup test data
Write-Host "Setting up test data..." -ForegroundColor Yellow
$setupQueries = @(
    "CREATE (p1:Person {name: 'Alice', age: 30, city: 'NYC', salary: 50000, tags: ['dev', 'senior']})",
    "CREATE (p2:Person {name: 'Bob', age: 25, city: 'LA', salary: 40000, tags: ['design']})",
    "CREATE (p3:Person {name: 'Charlie', age: 35, city: 'NYC', salary: 60000, tags: ['dev', 'lead']})",
    "CREATE (p4:Person {name: 'David', age: 28})",
    "CREATE (p5:Person:Employee {name: 'Eve', age: 32, city: 'SF', salary: 55000})",
    "CREATE (c1:Company {name: 'Acme Inc', founded: 2000})",
    "CREATE (c2:Company {name: 'TechCorp', founded: 2010})",
    "MATCH (p1:Person {name: 'Alice'}), (c1:Company {name: 'Acme Inc'}) CREATE (p1)-[:WORKS_AT {since: 2020}]->(c1)",
    "MATCH (p2:Person {name: 'Bob'}), (c1:Company {name: 'Acme Inc'}) CREATE (p2)-[:WORKS_AT {since: 2021}]->(c1)",
    "MATCH (p1:Person {name: 'Alice'}), (p2:Person {name: 'Bob'}) CREATE (p1)-[:KNOWS {since: 2015}]->(p2)"
)

foreach ($query in $setupQueries) {
    Invoke-Neo4jQuery -Cypher $query | Out-Null
    Invoke-NexusQuery -Cypher $query | Out-Null
}
Write-Host "Test data created" -ForegroundColor Green
Write-Host ""

# Advanced test queries
$testQueries = @()

# ===== ADDITIONAL STRING FUNCTIONS =====
$testQueries += @(
    @{ name = "toLower function"; cypher = "RETURN toLower('HELLO WORLD') AS lower" },
    @{ name = "toUpper function"; cypher = "RETURN toUpper('hello world') AS upper" },
    @{ name = "toLower with MATCH"; cypher = "MATCH (n:Person {name: 'Alice'}) RETURN toLower(n.name) AS lower" },
    @{ name = "toUpper with MATCH"; cypher = "MATCH (n:Person {name: 'Alice'}) RETURN toUpper(n.name) AS upper" },
    @{ name = "ltrim function"; cypher = "RETURN ltrim('  hello  ') AS trimmed" },
    @{ name = "rtrim function"; cypher = "RETURN rtrim('  hello  ') AS trimmed" },
    @{ name = "split function"; cypher = "RETURN split('a,b,c', ',') AS parts" },
    @{ name = "split with MATCH"; cypher = "MATCH (n:Person {name: 'Alice'}) RETURN split(n.name, 'l') AS parts" },
    @{ name = "substring with negative start"; cypher = "RETURN substring('hello', -2, 2) AS substr" },
    @{ name = "substring out of bounds"; cypher = "RETURN substring('hello', 10, 5) AS substr" }
)

# ===== LIST FUNCTIONS =====
$testQueries += @(
    @{ name = "head function"; cypher = "RETURN head([1, 2, 3]) AS first" },
    @{ name = "head empty list"; cypher = "RETURN head([]) AS first" },
    @{ name = "last function"; cypher = "RETURN last([1, 2, 3]) AS last" },
    @{ name = "last empty list"; cypher = "RETURN last([]) AS last" },
    @{ name = "size function with list"; cypher = "RETURN size([1, 2, 3, 4]) AS size" },
    @{ name = "size empty list"; cypher = "RETURN size([]) AS size" },
    @{ name = "size with MATCH"; cypher = "MATCH (n:Person {name: 'Alice'}) RETURN size(n.tags) AS size" },
    @{ name = "range function basic"; cypher = "RETURN range(1, 5) AS numbers" },
    @{ name = "range with step"; cypher = "RETURN range(0, 10, 2) AS numbers" },
    @{ name = "range negative step"; cypher = "RETURN range(10, 0, -2) AS numbers" },
    @{ name = "head with collect"; cypher = "MATCH (n:Person) RETURN head(collect(n.name)) AS first_name" },
    @{ name = "last with collect"; cypher = "MATCH (n:Person) RETURN last(collect(n.name)) AS last_name" }
)

# ===== MATH FUNCTIONS =====
$testQueries += @(
    @{ name = "abs function positive"; cypher = "RETURN abs(5) AS abs_val" },
    @{ name = "abs function negative"; cypher = "RETURN abs(-5) AS abs_val" },
    @{ name = "abs function zero"; cypher = "RETURN abs(0) AS abs_val" },
    @{ name = "abs with MATCH"; cypher = "MATCH (n:Person {name: 'Alice'}) RETURN abs(n.age - 35) AS diff" },
    @{ name = "round function"; cypher = "RETURN round(3.7) AS rounded" },
    @{ name = "round negative"; cypher = "RETURN round(-3.7) AS rounded" },
    @{ name = "ceil function"; cypher = "RETURN ceil(3.2) AS ceiling" },
    @{ name = "floor function"; cypher = "RETURN floor(3.8) AS floor_val" },
    @{ name = "round with MATCH"; cypher = "MATCH (n:Person {name: 'Alice'}) RETURN round(n.salary / 1000.0) AS salary_k" }
)

# ===== ORDER BY AND LIMIT =====
$testQueries += @(
    @{ name = "ORDER BY ascending"; cypher = "MATCH (n:Person) RETURN n.name ORDER BY n.name LIMIT 3" },
    @{ name = "ORDER BY descending"; cypher = "MATCH (n:Person) RETURN n.age ORDER BY n.age DESC LIMIT 3" },
    @{ name = "ORDER BY multiple columns"; cypher = "MATCH (n:Person) RETURN n.name, n.age ORDER BY n.age, n.name LIMIT 3" },
    @{ name = "LIMIT without ORDER BY"; cypher = "MATCH (n:Person) RETURN n.name LIMIT 2" },
    @{ name = "ORDER BY with WHERE"; cypher = "MATCH (n:Person) WHERE n.age > 25 RETURN n.name ORDER BY n.age DESC LIMIT 2" },
    @{ name = "ORDER BY with aggregation"; cypher = "MATCH (n:Person) RETURN n.city, count(n) AS count ORDER BY count DESC LIMIT 2" }
)

# ===== MULTIPLE COLUMNS IN RETURN =====
$testQueries += @(
    @{ name = "RETURN multiple columns"; cypher = "MATCH (n:Person {name: 'Alice'}) RETURN n.name, n.age, n.city" },
    @{ name = "RETURN with aliases"; cypher = "MATCH (n:Person {name: 'Alice'}) RETURN n.name AS name, n.age AS age" },
    @{ name = "RETURN with expressions"; cypher = "MATCH (n:Person {name: 'Alice'}) RETURN n.name, n.age + 5 AS future_age" },
    @{ name = "RETURN with functions"; cypher = "MATCH (n:Person {name: 'Alice'}) RETURN n.name, toUpper(n.city) AS city_upper" },
    @{ name = "RETURN multiple aggregations"; cypher = "MATCH (n:Person) RETURN count(n) AS total, avg(n.age) AS avg_age, min(n.age) AS min_age" }
)

# ===== NESTED EXPRESSIONS =====
$testQueries += @(
    @{ name = "Nested arithmetic"; cypher = "RETURN (10 + 5) * 2 AS result" },
    @{ name = "Nested with functions"; cypher = "RETURN substring('hello world', 0, length('hello')) AS substr" },
    @{ name = "Nested coalesce"; cypher = "RETURN coalesce(null, coalesce(null, 42)) AS result" },
    @{ name = "Nested logical"; cypher = "RETURN (5 > 3 AND 2 < 4) OR (1 > 2) AS result" },
    @{ name = "Complex nested expression"; cypher = "RETURN (10 + 5) * (2 ^ 2) AS result" }
)

# ===== EDGE CASES FOR AGGREGATION =====
$testQueries += @(
    @{ name = "count with DISTINCT"; cypher = "MATCH (n:Person) RETURN count(DISTINCT n.city) AS unique_cities" },
    @{ name = "sum with all nulls"; cypher = "MATCH (n:Person {name: 'David'}) RETURN sum(n.salary) AS total" },
    @{ name = "avg with single value"; cypher = "MATCH (n:Person {name: 'Alice'}) RETURN avg(n.age) AS avg_age" },
    @{ name = "min with strings"; cypher = "MATCH (n:Person) RETURN min(n.name) AS min_name" },
    @{ name = "max with strings"; cypher = "MATCH (n:Person) RETURN max(n.name) AS max_name" },
    @{ name = "collect with DISTINCT"; cypher = "MATCH (n:Person) RETURN collect(DISTINCT n.city) AS cities" },
    @{ name = "collect empty result"; cypher = "MATCH (n:NonExistent) RETURN collect(n.name) AS names" }
)

# ===== COMPLEX WHERE CONDITIONS =====
$testQueries += @(
    @{ name = "WHERE with nested AND"; cypher = "MATCH (n:Person) WHERE (n.age > 25 AND n.city = 'NYC') AND n.name <> 'David' RETURN count(n) AS count" },
    @{ name = "WHERE with nested OR"; cypher = "MATCH (n:Person) WHERE (n.age < 30 OR n.city = 'SF') OR n.name = 'Eve' RETURN count(n) AS count" },
    @{ name = "WHERE with NOT and AND"; cypher = "MATCH (n:Person) WHERE NOT (n.age < 30) AND n.city IS NOT NULL RETURN count(n) AS count" },
    @{ name = "WHERE with string comparison"; cypher = "MATCH (n:Person) WHERE n.name > 'Bob' RETURN count(n) AS count" },
    @{ name = "WHERE with list contains"; cypher = "MATCH (n:Person) WHERE 'dev' IN n.tags RETURN count(n) AS count" },
    @{ name = "WHERE with multiple IN"; cypher = "MATCH (n:Person) WHERE n.name IN ['Alice', 'Bob'] AND n.city IN ['NYC', 'LA'] RETURN count(n) AS count" }
)

# ===== PROPERTY ACCESS EDGE CASES =====
$testQueries += @(
    @{ name = "Access nested property"; cypher = "MATCH (n:Person {name: 'Alice'}) RETURN n.tags[0] AS first_tag" },
    @{ name = "Access non-existent property"; cypher = "MATCH (n:Person {name: 'Alice'}) RETURN n.non_existent AS prop" },
    @{ name = "Property with coalesce"; cypher = "MATCH (n:Person) RETURN coalesce(n.city, 'Unknown') AS city" },
    @{ name = "Property in WHERE with null"; cypher = "MATCH (n:Person) WHERE n.city = null RETURN count(n) AS count" },
    @{ name = "Property comparison with null"; cypher = "MATCH (n:Person) WHERE n.city <> null RETURN count(n) AS count" }
)

# ===== TYPE CONVERSION FUNCTIONS =====
$testQueries += @(
    @{ name = "toString function"; cypher = "RETURN toString(42) AS str" },
    @{ name = "toString with MATCH"; cypher = "MATCH (n:Person {name: 'Alice'}) RETURN toString(n.age) AS age_str" },
    @{ name = "toInteger function"; cypher = "RETURN toInteger('42') AS num" },
    @{ name = "toFloat function"; cypher = "RETURN toFloat('3.14') AS num" },
    @{ name = "toBoolean function"; cypher = "RETURN toBoolean('true') AS bool" }
)

# ===== RELATIONSHIP EDGE CASES =====
$testQueries += @(
    @{ name = "Relationship property access"; cypher = "MATCH (a)-[r:WORKS_AT]->(b) RETURN r.since AS since LIMIT 1" },
    @{ name = "Relationship with WHERE on property"; cypher = "MATCH (a)-[r:WORKS_AT]->(b) WHERE r.since > 2020 RETURN count(r) AS count" },
    @{ name = "Multiple relationship types"; cypher = "MATCH ()-[r]->() WHERE type(r) IN ['KNOWS', 'WORKS_AT'] RETURN count(r) AS count" },
    @{ name = "Relationship direction matters"; cypher = "MATCH (a)-[r:KNOWS]->(b) RETURN count(r) AS count" },
    @{ name = "Undirected relationship count"; cypher = "MATCH (a)-[r:KNOWS]-(b) RETURN count(r) AS count" }
)

# ===== MULTIPLE LABELS =====
$testQueries += @(
    @{ name = "Multiple labels intersection"; cypher = "MATCH (n:Person:Employee) RETURN count(n) AS count" },
    @{ name = "Multiple labels with WHERE"; cypher = "MATCH (n:Person:Employee) WHERE n.age > 30 RETURN count(n) AS count" },
    @{ name = "Multiple labels with aggregation"; cypher = "MATCH (n:Person:Employee) RETURN avg(n.age) AS avg_age" }
)

# ===== EMPTY RESULTS =====
$testQueries += @(
    @{ name = "Empty MATCH with count"; cypher = "MATCH (n:NonExistent) RETURN count(n) AS count" },
    @{ name = "Empty MATCH with sum"; cypher = "MATCH (n:NonExistent) RETURN sum(n.age) AS total" },
    @{ name = "Empty MATCH with avg"; cypher = "MATCH (n:NonExistent) RETURN avg(n.age) AS avg_age" },
    @{ name = "Empty MATCH with min"; cypher = "MATCH (n:NonExistent) RETURN min(n.age) AS min_age" },
    @{ name = "Empty MATCH with max"; cypher = "MATCH (n:NonExistent) RETURN max(n.age) AS max_age" },
    @{ name = "Empty MATCH with collect"; cypher = "MATCH (n:NonExistent) RETURN collect(n.name) AS names" }
)

Write-Host "Running $($testQueries.Count) advanced compatibility tests..." -ForegroundColor Yellow
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
    
    # Extract values
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
    
    # Compare row counts
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
Write-Host "ADVANCED COMPATIBILITY TEST SUMMARY" -ForegroundColor Cyan
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
$reportPath = Join-Path $reportDir "neo4j-advanced-compatibility-report.json"
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

