# Comprehensive REST API Tests for UDF & All Features
# Tests CREATE FUNCTION, DROP FUNCTION, SHOW FUNCTIONS and other features

$ErrorActionPreference = "Stop"
$baseUrl = "http://localhost:15474"

Write-Host "=== Comprehensive REST API Tests ===" -ForegroundColor Cyan
Write-Host ""

# Helper function to test Cypher query
function Test-CypherQuery {
    param(
        [string]$Name,
        [string]$Query,
        [hashtable]$Params = @{},
        [switch]$ExpectError
    )
    
    Write-Host "Testing: $Name" -ForegroundColor Yellow
    Write-Host "  Query: $Query" -ForegroundColor Gray
    
    try {
        $body = @{
            query = $Query
            params = $Params
        } | ConvertTo-Json
        
        $response = Invoke-RestMethod -Uri "$baseUrl/cypher" -Method Post -Body $body -ContentType "application/json" -ErrorAction Stop
        
        if ($ExpectError) {
            Write-Host "  ❌ Expected error but got success" -ForegroundColor Red
            return $false
        }
        
        Write-Host "  ✅ Success" -ForegroundColor Green
        if ($response.columns) {
            Write-Host "    Columns: $($response.columns -join ', ')" -ForegroundColor Gray
            Write-Host "    Rows: $($response.rows.Count)" -ForegroundColor Gray
            if ($response.rows.Count -gt 0 -and $response.rows.Count -le 3) {
                Write-Host "    Sample: $($response.rows | ConvertTo-Json -Compress)" -ForegroundColor DarkGray
            }
        }
        if ($response.execution_time_ms) {
            Write-Host "    Time: $($response.execution_time_ms)ms" -ForegroundColor Gray
        }
        return $true
    } catch {
        if ($ExpectError) {
            Write-Host "  ✅ Expected error occurred" -ForegroundColor Green
            return $true
        } else {
            Write-Host "  ❌ Error: $($_.Exception.Message)" -ForegroundColor Red
            if ($_.ErrorDetails.Message) {
                Write-Host "    Details: $($_.ErrorDetails.Message)" -ForegroundColor DarkRed
            }
            return $false
        }
    }
}

# Test counter
$totalTests = 0
$passedTests = 0

Write-Host "=== 1. UDF Management Tests ===" -ForegroundColor Cyan
Write-Host ""

# SHOW FUNCTIONS (initially empty or with built-ins)
$totalTests++
if (Test-CypherQuery -Name "SHOW FUNCTIONS (initial)" -Query "SHOW FUNCTIONS") {
    $passedTests++
}

# CREATE FUNCTION
$totalTests++
if (Test-CypherQuery -Name "CREATE FUNCTION multiply" -Query "CREATE FUNCTION multiply(a: Integer, b: Integer) RETURNS Integer AS 'Multiply two integers'") {
    $passedTests++
}

# CREATE FUNCTION IF NOT EXISTS
$totalTests++
if (Test-CypherQuery -Name "CREATE FUNCTION IF NOT EXISTS add" -Query "CREATE FUNCTION IF NOT EXISTS add(a: Integer, b: Integer) RETURNS Integer") {
    $passedTests++
}

# CREATE FUNCTION with different types
$totalTests++
if (Test-CypherQuery -Name "CREATE FUNCTION with String params" -Query "CREATE FUNCTION concat(a: String, b: String) RETURNS String AS 'Concatenate strings'") {
    $passedTests++
}

# CREATE FUNCTION duplicate (should fail)
$totalTests++
if (Test-CypherQuery -Name "CREATE FUNCTION duplicate (should fail)" -Query "CREATE FUNCTION multiply(a: Integer, b: Integer) RETURNS Integer" -ExpectError) {
    $passedTests++
}

# CREATE FUNCTION IF NOT EXISTS (should succeed)
$totalTests++
if (Test-CypherQuery -Name "CREATE FUNCTION IF NOT EXISTS (duplicate)" -Query "CREATE FUNCTION IF NOT EXISTS multiply(a: Integer, b: Integer) RETURNS Integer") {
    $passedTests++
}

# SHOW FUNCTIONS (should show created functions)
$totalTests++
if (Test-CypherQuery -Name "SHOW FUNCTIONS (after creation)" -Query "SHOW FUNCTIONS") {
    $passedTests++
}

# DROP FUNCTION
$totalTests++
if (Test-CypherQuery -Name "DROP FUNCTION concat" -Query "DROP FUNCTION concat") {
    $passedTests++
}

# DROP FUNCTION IF EXISTS (non-existent)
$totalTests++
if (Test-CypherQuery -Name "DROP FUNCTION IF EXISTS (non-existent)" -Query "DROP FUNCTION IF EXISTS nonexistent") {
    $passedTests++
}

# DROP FUNCTION (non-existent, should fail)
$totalTests++
if (Test-CypherQuery -Name "DROP FUNCTION (non-existent, should fail)" -Query "DROP FUNCTION nonexistent" -ExpectError) {
    $passedTests++
}

Write-Host ""
Write-Host "=== 2. Basic Graph Operations ===" -ForegroundColor Cyan
Write-Host ""

# CREATE nodes
$totalTests++
if (Test-CypherQuery -Name "CREATE nodes" -Query "CREATE (a:Person {name: 'Alice', age: 30}), (b:Person {name: 'Bob', age: 25})") {
    $passedTests++
}

# MATCH and RETURN
$totalTests++
if (Test-CypherQuery -Name "MATCH nodes" -Query "MATCH (n:Person) RETURN n.name, n.age ORDER BY n.age DESC LIMIT 10") {
    $passedTests++
}

# CREATE relationships
$totalTests++
if (Test-CypherQuery -Name "CREATE relationships" -Query "MATCH (a:Person {name: 'Alice'}), (b:Person {name: 'Bob'}) CREATE (a)-[:KNOWS {since: 2020}]->(b)") {
    $passedTests++
}

# MATCH with relationships
$totalTests++
if (Test-CypherQuery -Name "MATCH with relationships" -Query "MATCH (a:Person)-[r:KNOWS]->(b:Person) RETURN a.name, b.name, r.since") {
    $passedTests++
}

Write-Host ""
Write-Host "=== 3. Advanced Query Features ===" -ForegroundColor Cyan
Write-Host ""

# MERGE
$totalTests++
if (Test-CypherQuery -Name "MERGE with ON CREATE" -Query "MERGE (n:Person {name: 'Charlie'}) ON CREATE SET n.age = 28 RETURN n") {
    $passedTests++
}

# SET
$totalTests++
if (Test-CypherQuery -Name "SET properties" -Query "MATCH (n:Person {name: 'Alice'}) SET n.city = 'NYC', n:Employee RETURN n") {
    $passedTests++
}

# WHERE with conditions
$totalTests++
if (Test-CypherQuery -Name "WHERE with conditions" -Query "MATCH (n:Person) WHERE n.age > 25 RETURN n.name, n.age") {
    $passedTests++
}

# Aggregation
$totalTests++
if (Test-CypherQuery -Name "COUNT aggregation" -Query "MATCH (n:Person) RETURN count(n) AS total") {
    $passedTests++
}

# WITH clause
$totalTests++
if (Test-CypherQuery -Name "WITH clause" -Query "MATCH (n:Person) WITH n WHERE n.age > 25 RETURN n.name ORDER BY n.age") {
    $passedTests++
}

Write-Host ""
Write-Host "=== 4. Schema Management ===" -ForegroundColor Cyan
Write-Host ""

# CREATE INDEX
$totalTests++
if (Test-CypherQuery -Name "CREATE INDEX" -Query "CREATE INDEX ON :Person(name)") {
    $passedTests++
}

# CREATE INDEX IF NOT EXISTS
$totalTests++
if (Test-CypherQuery -Name "CREATE INDEX IF NOT EXISTS" -Query "CREATE INDEX IF NOT EXISTS ON :Person(age)") {
    $passedTests++
}

# SHOW FUNCTIONS again (to verify persistence)
$totalTests++
if (Test-CypherQuery -Name "SHOW FUNCTIONS (verify persistence)" -Query "SHOW FUNCTIONS") {
    $passedTests++
}

Write-Host ""
Write-Host "=== 5. String Operations ===" -ForegroundColor Cyan
Write-Host ""

# STARTS WITH
$totalTests++
if (Test-CypherQuery -Name "STARTS WITH" -Query "MATCH (n:Person) WHERE n.name STARTS WITH 'A' RETURN n.name") {
    $passedTests++
}

# CONTAINS
$totalTests++
if (Test-CypherQuery -Name "CONTAINS" -Query "MATCH (n:Person) WHERE n.name CONTAINS 'ice' RETURN n.name") {
    $passedTests++
}

# ENDS WITH
$totalTests++
if (Test-CypherQuery -Name "ENDS WITH" -Query "MATCH (n:Person) WHERE n.name ENDS WITH 'e' RETURN n.name") {
    $passedTests++
}

Write-Host ""
Write-Host "=== 6. Variable Length Paths ===" -ForegroundColor Cyan
Write-Host ""

# Variable length path
$totalTests++
if (Test-CypherQuery -Name "Variable length path" -Query "MATCH (a:Person {name: 'Alice'})-[*1..2]->(b) RETURN a.name, b.name LIMIT 10") {
    $passedTests++
}

Write-Host ""
Write-Host "=== 7. Built-in Functions ===" -ForegroundColor Cyan
Write-Host ""

# String functions
$totalTests++
if (Test-CypherQuery -Name "String functions" -Query "MATCH (n:Person) RETURN n.name, upper(n.name) AS upper_name, length(n.name) AS name_length LIMIT 5") {
    $passedTests++
}

# Math functions
$totalTests++
if (Test-CypherQuery -Name "Math functions" -Query "MATCH (n:Person) RETURN n.age, abs(n.age - 30) AS diff LIMIT 5") {
    $passedTests++
}

Write-Host ""
Write-Host "=== Summary ===" -ForegroundColor Cyan
Write-Host "Total Tests: $totalTests" -ForegroundColor White
Write-Host "Passed: $passedTests" -ForegroundColor Green
Write-Host "Failed: $($totalTests - $passedTests)" -ForegroundColor $(if ($totalTests -eq $passedTests) { "Green" } else { "Red" })
Write-Host "Success Rate: $([math]::Round(($passedTests / $totalTests) * 100, 2))%" -ForegroundColor $(if ($totalTests -eq $passedTests) { "Green" } else { "Yellow" })
Write-Host ""

