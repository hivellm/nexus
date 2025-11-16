# Test Neo4j Compatibility - Focused on implemented features
# Tests aggregation functions, WHERE clauses, logical operators, mathematical operators

param(
    [string]$Neo4jUri = "http://localhost:7474",
    [string]$Neo4jUser = "neo4j",
    [string]$Neo4jPassword = "password",
    [string]$NexusUri = "http://localhost:15474"
)

Write-Host "=== Neo4j Compatibility Test - Implemented Features ===" -ForegroundColor Cyan
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

function Compare-Values {
    param($Val1, $Val2)
    
    if ($null -eq $Val1 -and $null -eq $Val2) { return $true }
    if ($null -eq $Val1 -or $null -eq $Val2) { return $false }
    
    # Handle arrays
    if ($Val1 -is [array] -and $Val2 -is [array]) {
        if ($Val1.Count -ne $Val2.Count) { return $false }
        for ($i = 0; $i -lt $Val1.Count; $i++) {
            if (-not (Compare-Values $Val1[$i] $Val2[$i])) { return $false }
        }
        return $true
    }
    
    # Handle numbers (float comparison)
    if ($Val1 -is [double] -and $Val2 -is [double]) {
        return [math]::Abs($Val1 - $Val2) -lt 0.0001
    }
    
    return $Val1 -eq $Val2
}

# Test queries for implemented features
$testQueries = @(
    @{
        name = "RETURN count(*) without MATCH"
        cypher = "RETURN count(*) AS count"
        expectedNeo4j = 1
        expectedNexus = 1
    },
    @{
        name = "RETURN sum(1) without MATCH"
        cypher = "RETURN sum(1) AS sum_val"
    },
    @{
        name = "RETURN avg(10) without MATCH"
        cypher = "RETURN avg(10) AS avg_val"
    },
    @{
        name = "RETURN 2 ^ 3 AS power"
        cypher = "RETURN 2 ^ 3 AS power"
    },
    @{
        name = "RETURN 10 % 3 AS mod"
        cypher = "RETURN 10 % 3 AS mod"
    },
    @{
        name = "RETURN 5 IN [1, 2, 5] AS in_list"
        cypher = "RETURN 5 IN [1, 2, 5] AS in_list"
    },
    @{
        name = "RETURN (5 > 3 AND 2 < 4) AS and_result"
        cypher = "RETURN (5 > 3 AND 2 < 4) AS and_result"
    },
    @{
        name = "RETURN null = null AS null_eq"
        cypher = "RETURN null = null AS null_eq"
    },
    @{
        name = "RETURN substring('hello', 1, 3) AS substr"
        cypher = "RETURN substring('hello', 1, 3) AS substr"
    },
    @{
        name = "RETURN tail([1, 2, 3]) AS tail"
        cypher = "RETURN tail([1, 2, 3]) AS tail"
    }
)

$passed = 0
$failed = 0
$skipped = 0

Write-Host "Testing implemented features..." -ForegroundColor Yellow
Write-Host ""

foreach ($test in $testQueries) {
    Write-Host "Testing: $($test.name)" -ForegroundColor Cyan
    Write-Host "  Query: $($test.cypher)"
    
    $neo4jResult = Invoke-Neo4jQuery -Cypher $test.cypher
    $nexusResult = Invoke-NexusQuery -Cypher $test.cypher
    
    if ($null -eq $neo4jResult) {
        Write-Host "  [SKIP] Neo4j query failed" -ForegroundColor Yellow
        $skipped++
        continue
    }
    
    if ($null -eq $nexusResult) {
        Write-Host "  [FAIL] Nexus query failed" -ForegroundColor Red
        $failed++
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
Write-Host "COMPATIBILITY TEST SUMMARY" -ForegroundColor Cyan
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

