$NexusUri = "http://localhost:15474"
$Neo4jUri = "http://localhost:7474"
$Neo4jAuth = @{
    Username = "neo4j"
    Password = "password"
}

function Invoke-NexusQuery {
    param([string]$Cypher)
    $body = @{ query = $Cypher } | ConvertTo-Json
    try {
        return Invoke-RestMethod -Uri "$NexusUri/cypher" -Method POST -Headers @{"Content-Type" = "application/json"} -Body $body
    } catch {
        return $null
    }
}

function Invoke-Neo4jQuery {
    param([string]$Cypher)
    $body = @{ statements = @(@{ statement = $Cypher }) } | ConvertTo-Json -Depth 3
    $auth = [Convert]::ToBase64String([Text.Encoding]::ASCII.GetBytes("$($Neo4jAuth.Username):$($Neo4jAuth.Password)"))
    try {
        $result = Invoke-RestMethod -Uri "$Neo4jUri/db/neo4j/tx/commit" -Method POST -Headers @{
            "Content-Type" = "application/json"
            "Authorization" = "Basic $auth"
        } -Body $body
        return $result.results[0]
    } catch {
        return $null
    }
}

function Get-Count {
    param($Result, $Source)
    if (-not $Result) { return 0 }
    
    if ($Source -eq "Nexus") {
        if ($Result.rows -and $Result.rows.Count -gt 0) {
            if ($Result.rows[0] -is [array]) {
                return [int]$Result.rows[0][0]
            }
        }
    } else {
        if ($Result.data -and $Result.data.Count -gt 0) {
            if ($Result.data[0].row -and $Result.data[0].row.Count -gt 0) {
                return [int]$Result.data[0].row[0]
            }
        }
    }
    return 0
}

function Compare-Results {
    param($TestName, $Neo4jResult, $NexusResult, $Expected)
    
    $neo4jCount = Get-Count -Result $Neo4jResult -Source "Neo4j"
    $nexusCount = Get-Count -Result $NexusResult -Source "Nexus"
    
    if ($Expected) {
        if ($neo4jCount -eq $Expected -and $nexusCount -eq $Expected) {
            Write-Host "  ✅ PASS - Both: $Expected" -ForegroundColor Green
            return $true
        } else {
            Write-Host "  ❌ FAIL - Expected: $Expected, Neo4j: $neo4jCount, Nexus: $nexusCount" -ForegroundColor Red
            return $false
        }
    } else {
        if ($neo4jCount -eq $nexusCount) {
            Write-Host "  ✅ PASS - Both: $neo4jCount" -ForegroundColor Green
            return $true
        } else {
            Write-Host "  ❌ FAIL - Neo4j: $neo4jCount, Nexus: $nexusCount" -ForegroundColor Red
            return $false
        }
    }
}

Write-Host "`n╔════════════════════════════════════════════════════════════╗" -ForegroundColor Cyan
Write-Host "║        EXTENDED NEO4J COMPATIBILITY TEST SUITE          ║" -ForegroundColor Cyan
Write-Host "╚════════════════════════════════════════════════════════════╝`n" -ForegroundColor Cyan

$passed = 0
$failed = 0

# === SETUP ===
Write-Host "[SETUP] Cleaning databases..." -ForegroundColor Yellow
Invoke-Neo4jQuery -Cypher 'MATCH (n) DETACH DELETE n' | Out-Null
Invoke-NexusQuery -Cypher 'MATCH (n) DETACH DELETE n' | Out-Null
Write-Host "  ✅ Databases cleaned`n" -ForegroundColor Green

# === TEST SUITE 1: CREATE OPERATIONS ===
Write-Host "=== TEST SUITE 1: CREATE OPERATIONS - 10 tests ===`n" -ForegroundColor Cyan

Write-Host "[1] Single node with label"
Invoke-Neo4jQuery -Cypher "CREATE (n:Person)" | Out-Null
Invoke-NexusQuery -Cypher "CREATE (n:Person)" | Out-Null
$neo = Invoke-Neo4jQuery -Cypher 'MATCH (n:Person) RETURN count(*) AS c'
$nex = Invoke-NexusQuery -Cypher 'MATCH (n:Person) RETURN count(*) AS c'
if (Compare-Results -Neo4jResult $neo -NexusResult $nex -Expected 1) { $passed++ } else { $failed++ }

Write-Host "`n[2] Node with multiple labels"
Invoke-Neo4jQuery -Cypher "CREATE (n:Person:Employee:Manager)" | Out-Null
Invoke-NexusQuery -Cypher "CREATE (n:Person:Employee:Manager)" | Out-Null
$neo = Invoke-Neo4jQuery -Cypher 'MATCH (n:Person:Employee:Manager) RETURN count(*) AS c'
$nex = Invoke-NexusQuery -Cypher 'MATCH (n:Person:Employee:Manager) RETURN count(*) AS c'
if (Compare-Results -Neo4jResult $neo -NexusResult $nex -Expected 1) { $passed++ } else { $failed++ }

Write-Host "`n[3] Node with properties"
Invoke-Neo4jQuery -Cypher "CREATE (n:Product {name: 'Laptop', price: 999, inStock: true})" | Out-Null
Invoke-NexusQuery -Cypher "CREATE (n:Product {name: 'Laptop', price: 999, inStock: true})" | Out-Null
$neo = Invoke-Neo4jQuery -Cypher 'MATCH (n:Product {name: ''Laptop''}) RETURN count(*) AS c'
$nex = Invoke-NexusQuery -Cypher 'MATCH (n:Product {name: ''Laptop''}) RETURN count(*) AS c'
if (Compare-Results -Neo4jResult $neo -NexusResult $nex -Expected 1) { $passed++ } else { $failed++ }

Write-Host "`n[4] Multiple nodes in sequence"
Invoke-Neo4jQuery -Cypher "CREATE (n:City {name: 'NYC'})" | Out-Null
Invoke-Neo4jQuery -Cypher "CREATE (n:City {name: 'LA'})" | Out-Null
Invoke-Neo4jQuery -Cypher "CREATE (n:City {name: 'Chicago'})" | Out-Null
Invoke-NexusQuery -Cypher "CREATE (n:City {name: 'NYC'})" | Out-Null
Invoke-NexusQuery -Cypher "CREATE (n:City {name: 'LA'})" | Out-Null
Invoke-NexusQuery -Cypher "CREATE (n:City {name: 'Chicago'})" | Out-Null
$neo = Invoke-Neo4jQuery -Cypher 'MATCH (n:City) RETURN count(*) AS c'
$nex = Invoke-NexusQuery -Cypher 'MATCH (n:City) RETURN count(*) AS c'
if (Compare-Results -Neo4jResult $neo -NexusResult $nex -Expected 3) { $passed++ } else { $failed++ }

Write-Host "`n[5] Node without label (anonymous)"
Invoke-Neo4jQuery -Cypher "CREATE (n {id: 1})" | Out-Null
Invoke-NexusQuery -Cypher "CREATE (n {id: 1})" | Out-Null
$neo = Invoke-Neo4jQuery -Cypher 'MATCH (n {id: 1}) RETURN count(*) AS c'
$nex = Invoke-NexusQuery -Cypher 'MATCH (n {id: 1}) RETURN count(*) AS c'
if (Compare-Results -Neo4jResult $neo -NexusResult $nex -Expected 1) { $passed++ } else { $failed++ }

Write-Host "`n[6] CREATE with RETURN"
$neo = Invoke-Neo4jQuery -Cypher "CREATE (n:Test {value: 42}) RETURN n.value AS v"
$nex = Invoke-NexusQuery -Cypher "CREATE (n:Test {value: 42}) RETURN n.value AS v"
if ($neo.data.Count -eq $nex.rows.Count) {
    Write-Host "  ✅ PASS - Both returned data" -ForegroundColor Green
    $passed++
} else {
    Write-Host "  ❌ FAIL - Return count mismatch" -ForegroundColor Red
    $failed++
}

Write-Host "`n[7] Total node count after CREATE operations"
$neo = Invoke-Neo4jQuery -Cypher 'MATCH (n) RETURN count(*) AS c'
$nex = Invoke-NexusQuery -Cypher 'MATCH (n) RETURN count(*) AS c'
if (Compare-Results -Neo4jResult $neo -NexusResult $nex) { $passed++ } else { $failed++ }

Write-Host "`n[8] CREATE with NULL property"
Invoke-Neo4jQuery -Cypher "CREATE (n:NullTest {name: 'Test'})" | Out-Null
Invoke-NexusQuery -Cypher "CREATE (n:NullTest {name: 'Test'})" | Out-Null
$neo = Invoke-Neo4jQuery -Cypher 'MATCH (n:NullTest) RETURN count(*) AS c'
$nex = Invoke-NexusQuery -Cypher 'MATCH (n:NullTest) RETURN count(*) AS c'
if (Compare-Results -Neo4jResult $neo -NexusResult $nex -Expected 1) { $passed++ } else { $failed++ }

Write-Host "`n[9] CREATE with numeric properties"
Invoke-Neo4jQuery -Cypher "CREATE (n:Numbers {int: 42, float: 3.14})" | Out-Null
Invoke-NexusQuery -Cypher "CREATE (n:Numbers {int: 42, float: 3.14})" | Out-Null
$neo = Invoke-Neo4jQuery -Cypher 'MATCH (n:Numbers) RETURN count(*) AS c'
$nex = Invoke-NexusQuery -Cypher 'MATCH (n:Numbers) RETURN count(*) AS c'
if (Compare-Results -Neo4jResult $neo -NexusResult $nex -Expected 1) { $passed++ } else { $failed++ }

Write-Host "`n[10] CREATE with string properties"
Invoke-Neo4jQuery -Cypher "CREATE (n:Strings {text: 'Hello World', empty: ''})" | Out-Null
Invoke-NexusQuery -Cypher "CREATE (n:Strings {text: 'Hello World', empty: ''})" | Out-Null
$neo = Invoke-Neo4jQuery -Cypher 'MATCH (n:Strings) RETURN count(*) AS c'
$nex = Invoke-NexusQuery -Cypher 'MATCH (n:Strings) RETURN count(*) AS c'
if (Compare-Results -Neo4jResult $neo -NexusResult $nex -Expected 1) { $passed++ } else { $failed++ }

# === TEST SUITE 2: MATCH OPERATIONS ===
Write-Host "`n`n=== TEST SUITE 2: MATCH OPERATIONS - 10 tests ===`n" -ForegroundColor Cyan

Write-Host "[11] MATCH all nodes"
$neo = Invoke-Neo4jQuery -Cypher 'MATCH (n) RETURN count(*) AS c'
$nex = Invoke-NexusQuery -Cypher 'MATCH (n) RETURN count(*) AS c'
if (Compare-Results -Neo4jResult $neo -NexusResult $nex) { $passed++ } else { $failed++ }

Write-Host "`n[12] MATCH by single label"
$neo = Invoke-Neo4jQuery -Cypher 'MATCH (n:Person) RETURN count(*) AS c'
$nex = Invoke-NexusQuery -Cypher 'MATCH (n:Person) RETURN count(*) AS c'
if (Compare-Results -Neo4jResult $neo -NexusResult $nex) { $passed++ } else { $failed++ }

Write-Host "`n[13] MATCH by multiple labels"
$neo = Invoke-Neo4jQuery -Cypher 'MATCH (n:Person:Employee) RETURN count(*) AS c'
$nex = Invoke-NexusQuery -Cypher 'MATCH (n:Person:Employee) RETURN count(*) AS c'
if (Compare-Results -Neo4jResult $neo -NexusResult $nex) { $passed++ } else { $failed++ }

Write-Host "`n[14] MATCH with property filter (string)"
$neo = Invoke-Neo4jQuery -Cypher 'MATCH (n:Product {name: ''Laptop''}) RETURN count(*) AS c'
$nex = Invoke-NexusQuery -Cypher 'MATCH (n:Product {name: ''Laptop''}) RETURN count(*) AS c'
if (Compare-Results -Neo4jResult $neo -NexusResult $nex -Expected 1) { $passed++ } else { $failed++ }

Write-Host "`n[15] MATCH with property filter (number)"
$neo = Invoke-Neo4jQuery -Cypher 'MATCH (n:Product {price: 999}) RETURN count(*) AS c'
$nex = Invoke-NexusQuery -Cypher 'MATCH (n:Product {price: 999}) RETURN count(*) AS c'
if (Compare-Results -Neo4jResult $neo -NexusResult $nex -Expected 1) { $passed++ } else { $failed++ }

Write-Host "`n[16] MATCH with multiple property filters"
$neo = Invoke-Neo4jQuery -Cypher 'MATCH (n:Product {name: ''Laptop'', price: 999}) RETURN count(*) AS c'
$nex = Invoke-NexusQuery -Cypher 'MATCH (n:Product {name: ''Laptop'', price: 999}) RETURN count(*) AS c'
if (Compare-Results -Neo4jResult $neo -NexusResult $nex -Expected 1) { $passed++ } else { $failed++ }

Write-Host "`n[17] MATCH with WHERE clause (greater than)"
$neo = Invoke-Neo4jQuery -Cypher 'MATCH (n:Product) WHERE n.price > 500 RETURN count(*) AS c'
$nex = Invoke-NexusQuery -Cypher 'MATCH (n:Product) WHERE n.price > 500 RETURN count(*) AS c'
if (Compare-Results -Neo4jResult $neo -NexusResult $nex) { $passed++ } else { $failed++ }

Write-Host "`n[18] MATCH with WHERE clause (equals)"
$neo = Invoke-Neo4jQuery -Cypher 'MATCH (n:City) WHERE n.name = ''NYC'' RETURN count(*) AS c'
$nex = Invoke-NexusQuery -Cypher 'MATCH (n:City) WHERE n.name = ''NYC'' RETURN count(*) AS c'
if (Compare-Results -Neo4jResult $neo -NexusResult $nex -Expected 1) { $passed++ } else { $failed++ }

Write-Host "`n[19] MATCH non-existent label"
$neo = Invoke-Neo4jQuery -Cypher 'MATCH (n:NonExistent) RETURN count(*) AS c'
$nex = Invoke-NexusQuery -Cypher 'MATCH (n:NonExistent) RETURN count(*) AS c'
if (Compare-Results -Neo4jResult $neo -NexusResult $nex -Expected 0) { $passed++ } else { $failed++ }

Write-Host "`n[20] MATCH non-existent property"
$neo = Invoke-Neo4jQuery -Cypher 'MATCH (n:Product {nonexistent: ''value''}) RETURN count(*) AS c'
$nex = Invoke-NexusQuery -Cypher 'MATCH (n:Product {nonexistent: ''value''}) RETURN count(*) AS c'
if (Compare-Results -Neo4jResult $neo -NexusResult $nex -Expected 0) { $passed++ } else { $failed++ }

# === TEST SUITE 3: AGGREGATION FUNCTIONS ===
Write-Host "`n`n=== TEST SUITE 3: AGGREGATION FUNCTIONS - 10 tests ===`n" -ForegroundColor Cyan

Write-Host "[21] COUNT(*) all nodes"
$neo = Invoke-Neo4jQuery -Cypher 'MATCH (n) RETURN count(*) AS c'
$nex = Invoke-NexusQuery -Cypher 'MATCH (n) RETURN count(*) AS c'
if (Compare-Results -Neo4jResult $neo -NexusResult $nex) { $passed++ } else { $failed++ }

Write-Host "`n[22] COUNT(n) nodes"
$neo = Invoke-Neo4jQuery -Cypher 'MATCH (n:Person) RETURN count(n) AS c'
$nex = Invoke-NexusQuery -Cypher 'MATCH (n:Person) RETURN count(n) AS c'
if (Compare-Results -Neo4jResult $neo -NexusResult $nex) { $passed++ } else { $failed++ }

Write-Host "`n[23] COUNT DISTINCT"
$neo = Invoke-Neo4jQuery -Cypher 'MATCH (n:City) RETURN count(DISTINCT n.name) AS c'
$nex = Invoke-NexusQuery -Cypher 'MATCH (n:City) RETURN count(DISTINCT n.name) AS c'
if (Compare-Results -Neo4jResult $neo -NexusResult $nex) { $passed++ } else { $failed++ }

Write-Host "`n[24] SUM aggregation"
$neo = Invoke-Neo4jQuery -Cypher 'MATCH (n:Product) RETURN sum(n.price) AS s'
$nex = Invoke-NexusQuery -Cypher 'MATCH (n:Product) RETURN sum(n.price) AS s'
$neoSum = if ($neo.data[0].row[0]) { [int]$neo.data[0].row[0] } else { 0 }
$nexSum = if ($nex.rows[0] -is [array]) { [int]$nex.rows[0][0] } else { 0 }
if ($neoSum -eq $nexSum) {
    Write-Host "  ✅ PASS - Both: $neoSum" -ForegroundColor Green
    $passed++
} else {
    Write-Host "  ❌ FAIL - Neo4j: $neoSum, Nexus: $nexSum" -ForegroundColor Red
    $failed++
}

Write-Host "`n[25] AVG aggregation"
$neo = Invoke-Neo4jQuery -Cypher 'MATCH (n:Product) RETURN avg(n.price) AS a'
$nex = Invoke-NexusQuery -Cypher 'MATCH (n:Product) RETURN avg(n.price) AS a'
$neoAvg = if ($neo.data[0].row[0]) { [double]$neo.data[0].row[0] } else { 0 }
$nexAvg = if ($nex.rows[0] -is [array]) { [double]$nex.rows[0][0] } else { 0 }
if ([Math]::Abs($neoAvg - $nexAvg) -lt 0.01) {
    Write-Host "  ✅ PASS - Both: ~$neoAvg" -ForegroundColor Green
    $passed++
} else {
    Write-Host "  ❌ FAIL - Neo4j: $neoAvg, Nexus: $nexAvg" -ForegroundColor Red
    $failed++
}

Write-Host "`n[26] MIN aggregation"
$neo = Invoke-Neo4jQuery -Cypher 'MATCH (n:Product) RETURN min(n.price) AS m'
$nex = Invoke-NexusQuery -Cypher 'MATCH (n:Product) RETURN min(n.price) AS m'
$neoMin = if ($neo.data[0].row[0]) { [int]$neo.data[0].row[0] } else { 0 }
$nexMin = if ($nex.rows[0] -is [array]) { [int]$nex.rows[0][0] } else { 0 }
if ($neoMin -eq $nexMin) {
    Write-Host "  ✅ PASS - Both: $neoMin" -ForegroundColor Green
    $passed++
} else {
    Write-Host "  ❌ FAIL - Neo4j: $neoMin, Nexus: $nexMin" -ForegroundColor Red
    $failed++
}

Write-Host "`n[27] MAX aggregation"
$neo = Invoke-Neo4jQuery -Cypher 'MATCH (n:Product) RETURN max(n.price) AS m'
$nex = Invoke-NexusQuery -Cypher 'MATCH (n:Product) RETURN max(n.price) AS m'
$neoMax = if ($neo.data[0].row[0]) { [int]$neo.data[0].row[0] } else { 0 }
$nexMax = if ($nex.rows[0] -is [array]) { [int]$nex.rows[0][0] } else { 0 }
if ($neoMax -eq $nexMax) {
    Write-Host "  ✅ PASS - Both: $neoMax" -ForegroundColor Green
    $passed++
} else {
    Write-Host "  ❌ FAIL - Neo4j: $neoMax, Nexus: $nexMax" -ForegroundColor Red
    $failed++
}

Write-Host "`n[28] COUNT with WHERE"
$neo = Invoke-Neo4jQuery -Cypher 'MATCH (n:Product) WHERE n.price > 500 RETURN count(*) AS c'
$nex = Invoke-NexusQuery -Cypher 'MATCH (n:Product) WHERE n.price > 500 RETURN count(*) AS c'
if (Compare-Results -Neo4jResult $neo -NexusResult $nex) { $passed++ } else { $failed++ }

Write-Host "`n[29] Multiple aggregations"
$neo = Invoke-Neo4jQuery -Cypher 'MATCH (n:Product) RETURN count(*) AS c, sum(n.price) AS s, avg(n.price) AS a'
$nex = Invoke-NexusQuery -Cypher 'MATCH (n:Product) RETURN count(*) AS c, sum(n.price) AS s, avg(n.price) AS a'
if ($neo.data[0].row.Count -eq 3 -and $nex.rows[0].Count -eq 3) {
    Write-Host "  ✅ PASS - Both returned 3 aggregations" -ForegroundColor Green
    $passed++
} else {
    Write-Host "  ❌ FAIL - Aggregation count mismatch" -ForegroundColor Red
    $failed++
}

Write-Host "`n[30] COUNT with label and property"
$neo = Invoke-Neo4jQuery -Cypher 'MATCH (n:Product {name: ''Laptop''}) RETURN count(*) AS c'
$nex = Invoke-NexusQuery -Cypher 'MATCH (n:Product {name: ''Laptop''}) RETURN count(*) AS c'
if (Compare-Results -Neo4jResult $neo -NexusResult $nex -Expected 1) { $passed++ } else { $failed++ }

# === TEST SUITE 4: DELETE OPERATIONS ===
Write-Host "`n`n=== TEST SUITE 4: DELETE OPERATIONS - 10 tests ===`n" -ForegroundColor Cyan

Write-Host "[31] DELETE single node by property"
Invoke-Neo4jQuery -Cypher 'MATCH (n:Test {value: 42}) DELETE n' | Out-Null
Invoke-NexusQuery -Cypher 'MATCH (n:Test {value: 42}) DELETE n' | Out-Null
$neo = Invoke-Neo4jQuery -Cypher 'MATCH (n:Test {value: 42}) RETURN count(*) AS c'
$nex = Invoke-NexusQuery -Cypher 'MATCH (n:Test {value: 42}) RETURN count(*) AS c'
if (Compare-Results -Neo4jResult $neo -NexusResult $nex -Expected 0) { $passed++ } else { $failed++ }

Write-Host "`n[32] DELETE nodes by label"
Invoke-Neo4jQuery -Cypher 'MATCH (n:NullTest) DELETE n' | Out-Null
Invoke-NexusQuery -Cypher 'MATCH (n:NullTest) DELETE n' | Out-Null
$neo = Invoke-Neo4jQuery -Cypher 'MATCH (n:NullTest) RETURN count(*) AS c'
$nex = Invoke-NexusQuery -Cypher 'MATCH (n:NullTest) RETURN count(*) AS c'
if (Compare-Results -Neo4jResult $neo -NexusResult $nex -Expected 0) { $passed++ } else { $failed++ }

Write-Host "`n[33] Verify total count after DELETE"
$neo = Invoke-Neo4jQuery -Cypher 'MATCH (n) RETURN count(*) AS c'
$nex = Invoke-NexusQuery -Cypher 'MATCH (n) RETURN count(*) AS c'
if (Compare-Results -Neo4jResult $neo -NexusResult $nex) { $passed++ } else { $failed++ }

Write-Host "`n[34] DELETE with WHERE clause"
Invoke-Neo4jQuery -Cypher 'MATCH (n:Numbers) WHERE n.int > 40 DELETE n' | Out-Null
Invoke-NexusQuery -Cypher 'MATCH (n:Numbers) WHERE n.int > 40 DELETE n' | Out-Null
$neo = Invoke-Neo4jQuery -Cypher 'MATCH (n:Numbers) RETURN count(*) AS c'
$nex = Invoke-NexusQuery -Cypher 'MATCH (n:Numbers) RETURN count(*) AS c'
if (Compare-Results -Neo4jResult $neo -NexusResult $nex -Expected 0) { $passed++ } else { $failed++ }

Write-Host "`n[35] DETACH DELETE (no relationships)"
Invoke-Neo4jQuery -Cypher 'MATCH (n:Strings) DETACH DELETE n' | Out-Null
Invoke-NexusQuery -Cypher 'MATCH (n:Strings) DETACH DELETE n' | Out-Null
$neo = Invoke-Neo4jQuery -Cypher 'MATCH (n:Strings) RETURN count(*) AS c'
$nex = Invoke-NexusQuery -Cypher 'MATCH (n:Strings) RETURN count(*) AS c'
if (Compare-Results -Neo4jResult $neo -NexusResult $nex -Expected 0) { $passed++ } else { $failed++ }

Write-Host "`n[36] DELETE multiple nodes at once"
Invoke-Neo4jQuery -Cypher 'MATCH (n:City) DELETE n' | Out-Null
Invoke-NexusQuery -Cypher 'MATCH (n:City) DELETE n' | Out-Null
$neo = Invoke-Neo4jQuery -Cypher 'MATCH (n:City) RETURN count(*) AS c'
$nex = Invoke-NexusQuery -Cypher 'MATCH (n:City) RETURN count(*) AS c'
if (Compare-Results -Neo4jResult $neo -NexusResult $nex -Expected 0) { $passed++ } else { $failed++ }

Write-Host "`n[37] DELETE with multi-label"
Invoke-Neo4jQuery -Cypher 'MATCH (n:Person:Employee:Manager) DELETE n' | Out-Null
Invoke-NexusQuery -Cypher 'MATCH (n:Person:Employee:Manager) DELETE n' | Out-Null
$neo = Invoke-Neo4jQuery -Cypher 'MATCH (n:Person:Employee:Manager) RETURN count(*) AS c'
$nex = Invoke-NexusQuery -Cypher 'MATCH (n:Person:Employee:Manager) RETURN count(*) AS c'
if (Compare-Results -Neo4jResult $neo -NexusResult $nex -Expected 0) { $passed++ } else { $failed++ }

Write-Host "`n[38] DELETE anonymous node"
Invoke-Neo4jQuery -Cypher 'MATCH (n {id: 1}) DELETE n' | Out-Null
Invoke-NexusQuery -Cypher 'MATCH (n {id: 1}) DELETE n' | Out-Null
$neo = Invoke-Neo4jQuery -Cypher 'MATCH (n {id: 1}) RETURN count(*) AS c'
$nex = Invoke-NexusQuery -Cypher 'MATCH (n {id: 1}) RETURN count(*) AS c'
if (Compare-Results -Neo4jResult $neo -NexusResult $nex -Expected 0) { $passed++ } else { $failed++ }

Write-Host "`n[39] Verify specific label remains"
$neo = Invoke-Neo4jQuery -Cypher 'MATCH (n:Product) RETURN count(*) AS c'
$nex = Invoke-NexusQuery -Cypher 'MATCH (n:Product) RETURN count(*) AS c'
if (Compare-Results -Neo4jResult $neo -NexusResult $nex) { $passed++ } else { $failed++ }

Write-Host "`n[40] DELETE all remaining nodes"
Invoke-Neo4jQuery -Cypher 'MATCH (n) DETACH DELETE n' | Out-Null
Invoke-NexusQuery -Cypher 'MATCH (n) DETACH DELETE n' | Out-Null
$neo = Invoke-Neo4jQuery -Cypher 'MATCH (n) RETURN count(*) AS c'
$nex = Invoke-NexusQuery -Cypher 'MATCH (n) RETURN count(*) AS c'
if (Compare-Results -Neo4jResult $neo -NexusResult $nex -Expected 0) { $passed++ } else { $failed++ }

# === SUMMARY ===
Write-Host "`n`n╔════════════════════════════════════════════════════════════╗" -ForegroundColor Cyan
Write-Host "║                    TEST SUMMARY                          ║" -ForegroundColor Cyan
Write-Host "╚════════════════════════════════════════════════════════════╝`n" -ForegroundColor Cyan

$total = $passed + $failed
$passRate = [math]::Round(($passed / $total) * 100, 2)

Write-Host "Total Tests:    $total" -ForegroundColor White
Write-Host "Passed:         $passed" -ForegroundColor Green
Write-Host "Failed:         $failed" -ForegroundColor $(if ($failed -eq 0) { "Green" } else { "Red" })
Write-Host "Pass Rate:      $passRate%" -ForegroundColor $(if ($passRate -eq 100) { "Green" } elseif ($passRate -ge 90) { "Yellow" } else { "Red" })

Write-Host "`n══ TEST BREAKDOWN ══" -ForegroundColor Cyan
Write-Host "  Suite 1 (CREATE):      10 tests" -ForegroundColor White
Write-Host "  Suite 2 (MATCH):       10 tests" -ForegroundColor White
Write-Host "  Suite 3 (AGGREGATION): 10 tests" -ForegroundColor White
Write-Host "  Suite 4 (DELETE):      10 tests" -ForegroundColor White

if ($passRate -eq 100) {
    Write-Host "`n🎉 PERFECT SCORE! 100% COMPATIBILITY! 🎉`n" -ForegroundColor Green
} elseif ($passRate -ge 95) {
    Write-Host "`n✅ EXCELLENT! Over 95% compatibility!`n" -ForegroundColor Green
} elseif ($passRate -ge 90) {
    Write-Host "`n✅ VERY GOOD! Over 90% compatibility!`n" -ForegroundColor Yellow
} elseif ($passRate -ge 80) {
    Write-Host "`n⚠️  GOOD - Over 80%, some issues remain`n" -ForegroundColor Yellow
} else {
    Write-Host "`n❌ NEEDS WORK - Below 80%!`n" -ForegroundColor Red
}

