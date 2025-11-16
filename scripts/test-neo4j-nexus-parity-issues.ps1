# Neo4j vs Nexus Parity Issues Test Suite
# Tests specific compatibility issues identified in tasks.md
# Performs deep comparison of results including error messages, data types, and edge cases
# 
# Usage: ./test-neo4j-nexus-parity-issues.ps1
# Requirements: Neo4j running on localhost:7474, Nexus running on localhost:15474

param(
    [string]$Neo4jUri = "http://localhost:7474",
    [string]$Neo4jUser = "neo4j",
    [string]$Neo4jPassword = "password",
    [string]$NexusUri = "http://localhost:15474",
    [switch]$Verbose
)

$ErrorActionPreference = "Continue"
$global:TestResults = @()

Write-Host "╔══════════════════════════════════════════════════════════════╗" -ForegroundColor Cyan
Write-Host "║   Neo4j vs Nexus Parity Issues Deep Comparison Test         ║" -ForegroundColor Cyan
Write-Host "╚══════════════════════════════════════════════════════════════╝" -ForegroundColor Cyan
Write-Host ""
Write-Host "Neo4j:  $Neo4jUri" -ForegroundColor Yellow
Write-Host "Nexus:  $NexusUri" -ForegroundColor Yellow
Write-Host ""

# Function to execute query on Neo4j
function Invoke-Neo4jQuery {
    param([string]$Cypher, [hashtable]$Parameters = @{})
    
    $body = @{
        statements = @(
            @{
                statement = $Cypher
                parameters = $Parameters
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
            return @{ 
                error = $response.errors[0].message
                errorCode = $response.errors[0].code
            }
        }
        
        return $response.results[0]
    }
    catch {
        return @{ error = $_.Exception.Message }
    }
}

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

# Deep comparison function
function Compare-Results {
    param(
        [string]$TestName,
        [string]$Query,
        [object]$Neo4jResult,
        [object]$NexusResult,
        [string]$Category,
        [string]$ExpectedBehavior,
        [switch]$ExpectError
    )
    
    $testEntry = @{
        Name = $TestName
        Query = $Query
        Category = $Category
        Status = "UNKNOWN"
        Neo4jRows = 0
        NexusRows = 0
        Neo4jError = $null
        NexusError = $null
        DataMatch = $false
        TypeMatch = $false
        Details = @()
    }
    
    # Extract errors
    if ($Neo4jResult.error) {
        $testEntry.Neo4jError = $Neo4jResult.error
        $testEntry.Details += "Neo4j Error: $($Neo4jResult.error)"
    }
    
    if ($NexusResult.error) {
        $testEntry.NexusError = $NexusResult.error
        $testEntry.Details += "Nexus Error: $($NexusResult.error)"
    }
    
    # If expecting error, check if both errored
    if ($ExpectError) {
        if ($Neo4jResult.error -and $NexusResult.error) {
            $testEntry.Status = "EXPECTED_ERROR_MATCH"
            Write-Host "⚠️  BOTH_ERROR: $TestName" -ForegroundColor Yellow
        }
        elseif (-not $Neo4jResult.error -and -not $NexusResult.error) {
            $testEntry.Status = "UNEXPECTED_SUCCESS"
            Write-Host "⚠️  NO_ERROR: $TestName (expected error but both succeeded)" -ForegroundColor Yellow
        }
        else {
            $testEntry.Status = "ERROR_MISMATCH"
            Write-Host "❌ ERROR_MISMATCH: $TestName" -ForegroundColor Red
        }
        $global:TestResults += $testEntry
        return
    }
    
    # Check for error mismatch (one errors, one doesn't)
    if ($Neo4jResult.error -and -not $NexusResult.error) {
        $testEntry.Status = "NEO4J_ERROR"
        $testEntry.Details += "Neo4j errored but Nexus succeeded"
        $global:TestResults += $testEntry
        Write-Host "⚠️  NEO4J_ERROR: $TestName" -ForegroundColor Yellow
        return
    }
    
    if ($NexusResult.error -and -not $Neo4jResult.error) {
        $testEntry.Status = "NEXUS_ERROR"
        $testEntry.Details += "Nexus errored but Neo4j succeeded"
        $testEntry.Details += "Expected: $ExpectedBehavior"
        $global:TestResults += $testEntry
        Write-Host "❌ NEXUS_ERROR: $TestName" -ForegroundColor Red
        return
    }
    
    # Both errored - skip comparison
    if ($Neo4jResult.error -and $NexusResult.error) {
        $testEntry.Status = "BOTH_ERROR"
        $global:TestResults += $testEntry
        Write-Host "⏭️  BOTH_ERROR: $TestName" -ForegroundColor Gray
        return
    }
    
    # Extract row counts
    $neo4jRows = if ($Neo4jResult.data) { $Neo4jResult.data.Count } else { 0 }
    $nexusRows = if ($NexusResult.rows) { $NexusResult.rows.Count } else { 0 }
    
    $testEntry.Neo4jRows = $neo4jRows
    $testEntry.NexusRows = $nexusRows
    
    # Compare row counts
    if ($neo4jRows -ne $nexusRows) {
        $testEntry.Status = "ROW_COUNT_MISMATCH"
        $testEntry.Details += "Row count: Neo4j=$neo4jRows, Nexus=$nexusRows"
        $testEntry.Details += "Expected: $ExpectedBehavior"
        $global:TestResults += $testEntry
        Write-Host "❌ ROW_MISMATCH: $TestName (Neo4j=$neo4jRows, Nexus=$nexusRows)" -ForegroundColor Red
        return
    }
    
    # If no rows, consider it a pass
    if ($neo4jRows -eq 0) {
        $testEntry.Status = "PASS_EMPTY"
        $testEntry.DataMatch = $true
        $global:TestResults += $testEntry
        Write-Host "✅ PASS: $TestName (both empty)" -ForegroundColor Green
        return
    }
    
    # Deep data comparison
    $dataMatches = $true
    $typeMatches = $true
    
    for ($i = 0; $i -lt $neo4jRows; $i++) {
        $neo4jRow = $Neo4jResult.data[$i].row
        $nexusRow = $NexusResult.rows[$i]
        
        if ($neo4jRow.Count -ne $nexusRow.Count) {
            $dataMatches = $false
            $testEntry.Details += "Row ${i}: Column count mismatch (Neo4j=$($neo4jRow.Count), Nexus=$($nexusRow.Count))"
        }
        
        # Compare each column
        for ($j = 0; $j -lt [Math]::Min($neo4jRow.Count, $nexusRow.Count); $j++) {
            $neo4jVal = $neo4jRow[$j]
            $nexusVal = $nexusRow[$j]
            
            # Type comparison
            $neo4jType = if ($null -eq $neo4jVal) { "null" } else { $neo4jVal.GetType().Name }
            $nexusType = if ($null -eq $nexusVal) { "null" } else { $nexusVal.GetType().Name }
            
            if ($neo4jType -ne $nexusType) {
                $typeMatches = $false
                $testEntry.Details += "Row ${i}, Col ${j}: Type mismatch (Neo4j=$neo4jType, Nexus=$nexusType)"
            }
            
            # Value comparison (handle arrays specially)
            if ($neo4jVal -is [Array] -and $nexusVal -is [Array]) {
                if ($neo4jVal.Count -ne $nexusVal.Count) {
                    $dataMatches = $false
                    $testEntry.Details += "Row ${i}, Col ${j}: Array length mismatch"
                }
                else {
                    for ($k = 0; $k -lt $neo4jVal.Count; $k++) {
                        if ($neo4jVal[$k] -ne $nexusVal[$k]) {
                            $dataMatches = $false
                            $testEntry.Details += "Row ${i}, Col ${j}, Elem ${k}: Value mismatch (Neo4j=$($neo4jVal[$k]), Nexus=$($nexusVal[$k]))"
                        }
                    }
                }
            }
            elseif ($neo4jVal -ne $nexusVal) {
                $dataMatches = $false
                $testEntry.Details += "Row ${i}, Col ${j}: Value mismatch (Neo4j=$neo4jVal, Nexus=$nexusVal)"
            }
        }
    }
    
    $testEntry.DataMatch = $dataMatches
    $testEntry.TypeMatch = $typeMatches
    
    if ($dataMatches -and $typeMatches) {
        $testEntry.Status = "PASS"
        Write-Host "✅ PASS: $TestName" -ForegroundColor Green
    }
    elseif ($dataMatches) {
        $testEntry.Status = "TYPE_MISMATCH"
        Write-Host "⚠️  TYPE_MISMATCH: $TestName" -ForegroundColor Yellow
    }
    else {
        $testEntry.Status = "DATA_MISMATCH"
        Write-Host "❌ DATA_MISMATCH: $TestName" -ForegroundColor Red
    }
    
    $global:TestResults += $testEntry
}

# Test runner
function Run-ParityTest {
    param(
        [string]$Name,
        [string]$Query,
        [string]$Category,
        [string]$ExpectedBehavior,
        [switch]$ExpectError
    )
    
    if ($Verbose) {
        Write-Host "`n--- $Name ---" -ForegroundColor Cyan
        Write-Host "Query: $Query" -ForegroundColor Gray
    }
    
    $neo4jResult = Invoke-Neo4jQuery -Cypher $Query
    $nexusResult = Invoke-NexusQuery -Cypher $Query
    
    Compare-Results -TestName $Name -Query $Query -Neo4jResult $neo4jResult -NexusResult $nexusResult `
        -Category $Category -ExpectedBehavior $ExpectedBehavior -ExpectError:$ExpectError
}

# Setup: Clean databases
Write-Host "`n🔧 Setting up test environment..." -ForegroundColor Cyan
Invoke-Neo4jQuery -Cypher "MATCH (n) DETACH DELETE n" | Out-Null
Invoke-NexusQuery -Cypher "MATCH (n) DETACH DELETE n" | Out-Null
Write-Host "✓ Databases cleaned`n" -ForegroundColor Green

#═══════════════════════════════════════════════════════════════════
# CATEGORY 1: CREATE WITH RETURN (HIGH PRIORITY - 5 tests)
#═══════════════════════════════════════════════════════════════════
Write-Host "`n┌─────────────────────────────────────────────────────┐" -ForegroundColor Yellow
Write-Host "│ Category 1: CREATE with RETURN (5 tests)           │" -ForegroundColor Yellow
Write-Host "└─────────────────────────────────────────────────────┘" -ForegroundColor Yellow

Run-ParityTest -Name "CREATE.01 - Single node with property return" `
    -Query "CREATE (n:Person {name: 'Alice', age: 30}) RETURN n.name AS name" `
    -Category "CREATE_WITH_RETURN" `
    -ExpectedBehavior "Should return 1 row with name='Alice'"

Run-ParityTest -Name "CREATE.02 - Create and return literal" `
    -Query "CREATE (n:Person {name: 'Bob'}) RETURN 'created' AS status" `
    -Category "CREATE_WITH_RETURN" `
    -ExpectedBehavior "Should return 1 row with status='created'"

Run-ParityTest -Name "CREATE.03 - Multiple properties return" `
    -Query "CREATE (n:Person {name: 'Charlie', age: 35, city: 'NYC'}) RETURN n.name, n.age, n.city" `
    -Category "CREATE_WITH_RETURN" `
    -ExpectedBehavior "Should return 1 row with all 3 properties"

Run-ParityTest -Name "CREATE.04 - Multiple labels return" `
    -Query "CREATE (n:Person:Employee {name: 'David'}) RETURN labels(n) AS lbls" `
    -Category "CREATE_WITH_RETURN" `
    -ExpectedBehavior "Should return 1 row with labels array ['Person', 'Employee']"

Run-ParityTest -Name "CREATE.05 - Return node object" `
    -Query "CREATE (n:Person {name: 'Eve'}) RETURN n" `
    -Category "CREATE_WITH_RETURN" `
    -ExpectedBehavior "Should return 1 row with complete node object"

Run-ParityTest -Name "CREATE.06 - Return id() function" `
    -Query "CREATE (n:Person {name: 'Frank'}) RETURN id(n) AS node_id" `
    -Category "CREATE_WITH_RETURN" `
    -ExpectedBehavior "Should return 1 row with numeric node ID"

Run-ParityTest -Name "CREATE.07 - Multiple creates with RETURN" `
    -Query "CREATE (a:Person {name: 'Alice'}), (b:Person {name: 'Bob'}) RETURN a.name, b.name" `
    -Category "CREATE_WITH_RETURN" `
    -ExpectedBehavior "Should return 1 row with both names"

#═══════════════════════════════════════════════════════════════════
# CATEGORY 2: STRING CONCATENATION (MEDIUM PRIORITY - 2 tests)
#═══════════════════════════════════════════════════════════════════
Write-Host "`n┌─────────────────────────────────────────────────────┐" -ForegroundColor Yellow
Write-Host "│ Category 2: String Concatenation (5 tests)         │" -ForegroundColor Yellow
Write-Host "└─────────────────────────────────────────────────────┘" -ForegroundColor Yellow

Run-ParityTest -Name "STRING.01 - Basic concatenation" `
    -Query "RETURN 'Hello' + ' ' + 'World' AS text" `
    -Category "STRING_CONCAT" `
    -ExpectedBehavior "Should return 1 row with text='Hello World'"

Run-ParityTest -Name "STRING.02 - Concatenation with property" `
    -Query "MATCH (n:Person {name: 'Alice'}) RETURN 'Name: ' + n.name AS result" `
    -Category "STRING_CONCAT" `
    -ExpectedBehavior "Should return 'Name: Alice'"

Run-ParityTest -Name "STRING.03 - Multiple concatenations" `
    -Query "RETURN 'A' + 'B' + 'C' + 'D' AS result" `
    -Category "STRING_CONCAT" `
    -ExpectedBehavior "Should return 'ABCD'"

Run-ParityTest -Name "STRING.04 - Concatenation with NULL" `
    -Query "RETURN 'Hello' + null AS result" `
    -Category "STRING_CONCAT" `
    -ExpectedBehavior "Should return NULL (string + null = null)"

Run-ParityTest -Name "STRING.05 - Concatenation in WHERE" `
    -Query "MATCH (n:Person) WHERE n.name + ' Test' = 'Alice Test' RETURN n.name" `
    -Category "STRING_CONCAT" `
    -ExpectedBehavior "Should return Alice if concatenation works in WHERE"

#═══════════════════════════════════════════════════════════════════
# CATEGORY 3: ARRAY SLICING (MEDIUM PRIORITY - 3 tests)
#═══════════════════════════════════════════════════════════════════
Write-Host "`n┌─────────────────────────────────────────────────────┐" -ForegroundColor Yellow
Write-Host "│ Category 3: Array Slicing (5 tests)                │" -ForegroundColor Yellow
Write-Host "└─────────────────────────────────────────────────────┘" -ForegroundColor Yellow

Run-ParityTest -Name "ARRAY.01 - Basic slicing [1..3]" `
    -Query "RETURN [1, 2, 3, 4, 5][1..3] AS slice" `
    -Category "ARRAY_SLICE" `
    -ExpectedBehavior "Should return [2, 3, 4] (indices 1 to 3 inclusive)"

Run-ParityTest -Name "ARRAY.02 - Slicing from start [..3]" `
    -Query "RETURN [1, 2, 3, 4, 5][..3] AS slice" `
    -Category "ARRAY_SLICE" `
    -ExpectedBehavior "Should return [1, 2, 3, 4] (start to index 3)"

Run-ParityTest -Name "ARRAY.03 - Slicing to end [2..]" `
    -Query "RETURN [1, 2, 3, 4, 5][2..] AS slice" `
    -Category "ARRAY_SLICE" `
    -ExpectedBehavior "Should return [3, 4, 5] (index 2 to end)"

Run-ParityTest -Name "ARRAY.04 - Negative index slicing [-3..-1]" `
    -Query "RETURN [1, 2, 3, 4, 5][-3..-1] AS slice" `
    -Category "ARRAY_SLICE" `
    -ExpectedBehavior "Should return [3, 4, 5] (last 3 elements)"

Run-ParityTest -Name "ARRAY.05 - Slicing with property" `
    -Query "CREATE (n:Person {tags: ['dev', 'ops', 'admin']}) RETURN n.tags[0..1] AS slice" `
    -Category "ARRAY_SLICE" `
    -ExpectedBehavior "Should return ['dev', 'ops']"

#═══════════════════════════════════════════════════════════════════
# CATEGORY 4: ARRAY CONCATENATION (MEDIUM PRIORITY - 3 tests)
#═══════════════════════════════════════════════════════════════════
Write-Host "`n┌─────────────────────────────────────────────────────┐" -ForegroundColor Yellow
Write-Host "│ Category 4: Array Concatenation (5 tests)          │" -ForegroundColor Yellow
Write-Host "└─────────────────────────────────────────────────────┘" -ForegroundColor Yellow

Run-ParityTest -Name "CONCAT.01 - Basic array concatenation" `
    -Query "RETURN [1, 2] + [3, 4] AS combined" `
    -Category "ARRAY_CONCAT" `
    -ExpectedBehavior "Should return [1, 2, 3, 4]"

Run-ParityTest -Name "CONCAT.02 - String array concatenation" `
    -Query "RETURN ['a', 'b'] + ['c', 'd'] AS combined" `
    -Category "ARRAY_CONCAT" `
    -ExpectedBehavior "Should return ['a', 'b', 'c', 'd']"

Run-ParityTest -Name "CONCAT.03 - Multiple concatenations" `
    -Query "RETURN [1] + [2] + [3] + [4] AS combined" `
    -Category "ARRAY_CONCAT" `
    -ExpectedBehavior "Should return [1, 2, 3, 4]"

Run-ParityTest -Name "CONCAT.04 - Empty array concatenation" `
    -Query "RETURN [] + [1, 2] AS combined" `
    -Category "ARRAY_CONCAT" `
    -ExpectedBehavior "Should return [1, 2]"

Run-ParityTest -Name "CONCAT.05 - Mixed type concatenation" `
    -Query "RETURN [1, 'a'] + [true, null] AS combined" `
    -Category "ARRAY_CONCAT" `
    -ExpectedBehavior "Should return [1, 'a', true, null]"

#═══════════════════════════════════════════════════════════════════
# CATEGORY 5: MULTIPLE RELATIONSHIP TYPES (LOW PRIORITY - 4 tests)
#═══════════════════════════════════════════════════════════════════
Write-Host "`n┌─────────────────────────────────────────────────────┐" -ForegroundColor Yellow
Write-Host "│ Category 5: Multiple Rel Types (4 tests)           │" -ForegroundColor Yellow
Write-Host "└─────────────────────────────────────────────────────┘" -ForegroundColor Yellow

# Setup relationships
Invoke-Neo4jQuery -Cypher "MATCH (a:Person {name: 'Alice'}), (b:Person {name: 'Bob'}) CREATE (a)-[:KNOWS]->(b)" | Out-Null
Invoke-NexusQuery -Cypher "MATCH (a:Person {name: 'Alice'}), (b:Person {name: 'Bob'}) CREATE (a)-[:KNOWS]->(b)" | Out-Null

Invoke-Neo4jQuery -Cypher "MATCH (a:Person {name: 'Alice'}), (c:Person {name: 'Charlie'}) CREATE (a)-[:WORKS_WITH]->(c)" | Out-Null
Invoke-NexusQuery -Cypher "MATCH (a:Person {name: 'Alice'}), (c:Person {name: 'Charlie'}) CREATE (a)-[:WORKS_WITH]->(c)" | Out-Null

Run-ParityTest -Name "RELTYPE.01 - Multiple types with pipe" `
    -Query "MATCH (a)-[r:KNOWS|WORKS_WITH]->(b) RETURN count(r) AS cnt" `
    -Category "MULTIPLE_REL_TYPES" `
    -ExpectedBehavior "Should match both KNOWS and WORKS_WITH relationships"

Run-ParityTest -Name "RELTYPE.02 - Three types with pipe" `
    -Query "MATCH (a)-[r:KNOWS|WORKS_WITH|MANAGES]->(b) RETURN count(r) AS cnt" `
    -Category "MULTIPLE_REL_TYPES" `
    -ExpectedBehavior "Should match all three relationship types"

Run-ParityTest -Name "RELTYPE.03 - Return type with multiple" `
    -Query "MATCH (a)-[r:KNOWS|WORKS_WITH]->(b) RETURN type(r) AS rel_type" `
    -Category "MULTIPLE_REL_TYPES" `
    -ExpectedBehavior "Should return relationship types"

Run-ParityTest -Name "RELTYPE.04 - Bidirectional with multiple types" `
    -Query "MATCH (a)-[r:KNOWS|WORKS_WITH]-(b) RETURN count(r) AS cnt" `
    -Category "MULTIPLE_REL_TYPES" `
    -ExpectedBehavior "Should match in both directions"

#═══════════════════════════════════════════════════════════════════
# FINAL REPORT
#═══════════════════════════════════════════════════════════════════
Write-Host "`n╔══════════════════════════════════════════════════════════════╗" -ForegroundColor Cyan
Write-Host "║                  DETAILED TEST SUMMARY                       ║" -ForegroundColor Cyan
Write-Host "╚══════════════════════════════════════════════════════════════╝" -ForegroundColor Cyan

# Group by category
$categories = $global:TestResults | Group-Object Category

foreach ($category in $categories) {
    Write-Host "`n━━━ $($category.Name) ━━━" -ForegroundColor Yellow
    
    $passed = ($category.Group | Where-Object { $_.Status -eq "PASS" }).Count
    $total = $category.Group.Count
    $passRate = if ($total -gt 0) { [math]::Round(($passed / $total) * 100, 2) } else { 0 }
    
    Write-Host "  Pass Rate: $passRate% ($passed/$total)" -ForegroundColor $(if ($passRate -ge 80) { "Green" } else { "Red" })
    
    foreach ($test in $category.Group) {
        $icon = switch ($test.Status) {
            "PASS" { "✅" }
            "PASS_EMPTY" { "✅" }
            "NEXUS_ERROR" { "❌" }
            "ROW_COUNT_MISMATCH" { "❌" }
            "DATA_MISMATCH" { "❌" }
            "TYPE_MISMATCH" { "⚠️ " }
            default { "⏭️ " }
        }
        
        Write-Host "    $icon $($test.Name) - $($test.Status)" -ForegroundColor $(
            if ($test.Status -eq "PASS" -or $test.Status -eq "PASS_EMPTY") { "Green" }
            elseif ($test.Status -like "*ERROR*" -or $test.Status -like "*MISMATCH*") { "Red" }
            else { "Yellow" }
        )
        
        if ($test.Details.Count -gt 0 -and $Verbose) {
            foreach ($detail in $test.Details) {
                Write-Host "       └─ $detail" -ForegroundColor Gray
            }
        }
    }
}

# Overall statistics
Write-Host "`n╔══════════════════════════════════════════════════════════════╗" -ForegroundColor Cyan
Write-Host "║                    OVERALL STATISTICS                        ║" -ForegroundColor Cyan
Write-Host "╚══════════════════════════════════════════════════════════════╝" -ForegroundColor Cyan

$totalTests = $global:TestResults.Count
$passedTests = ($global:TestResults | Where-Object { $_.Status -eq "PASS" -or $_.Status -eq "PASS_EMPTY" }).Count
$nexusErrors = ($global:TestResults | Where-Object { $_.Status -eq "NEXUS_ERROR" }).Count
$rowMismatches = ($global:TestResults | Where-Object { $_.Status -eq "ROW_COUNT_MISMATCH" }).Count
$dataMismatches = ($global:TestResults | Where-Object { $_.Status -eq "DATA_MISMATCH" }).Count
$typeMismatches = ($global:TestResults | Where-Object { $_.Status -eq "TYPE_MISMATCH" }).Count

Write-Host ""
Write-Host "Total Tests:       $totalTests" -ForegroundColor White
Write-Host "Passed:            $passedTests" -ForegroundColor Green
Write-Host "Nexus Errors:      $nexusErrors" -ForegroundColor Red
Write-Host "Row Mismatches:    $rowMismatches" -ForegroundColor Red
Write-Host "Data Mismatches:   $dataMismatches" -ForegroundColor Red
Write-Host "Type Mismatches:   $typeMismatches" -ForegroundColor Yellow
Write-Host ""

$overallPassRate = if ($totalTests -gt 0) { [math]::Round(($passedTests / $totalTests) * 100, 2) } else { 0 }
Write-Host "Overall Pass Rate: $overallPassRate%" -ForegroundColor $(
    if ($overallPassRate -ge 95) { "Green" }
    elseif ($overallPassRate -ge 80) { "Yellow" }
    else { "Red" }
)

# Priority action items
Write-Host "`n╔══════════════════════════════════════════════════════════════╗" -ForegroundColor Cyan
Write-Host "║                   PRIORITY ACTION ITEMS                      ║" -ForegroundColor Cyan
Write-Host "╚══════════════════════════════════════════════════════════════╝" -ForegroundColor Cyan
Write-Host ""

$failedByCategory = $global:TestResults | Where-Object { 
    $_.Status -ne "PASS" -and $_.Status -ne "PASS_EMPTY" -and $_.Status -notlike "*BOTH_ERROR*"
} | Group-Object Category | Sort-Object Count -Descending

foreach ($category in $failedByCategory) {
    $priority = switch ($category.Name) {
        "CREATE_WITH_RETURN" { "🔴 HIGH" }
        "STRING_CONCAT" { "🟡 MEDIUM" }
        "ARRAY_SLICE" { "🟡 MEDIUM" }
        "ARRAY_CONCAT" { "🟡 MEDIUM" }
        "MULTIPLE_REL_TYPES" { "🟢 LOW" }
        default { "🔵 INFO" }
    }
    
    Write-Host "$priority - $($category.Name): $($category.Count) issues" -ForegroundColor Yellow
}

Write-Host ""

