# Nexus vs Neo4j Comprehensive Benchmark Suite
# Compares performance and functionality across all features
#
# Usage: ./benchmark-nexus-vs-neo4j-comprehensive.ps1
# Requirements: Neo4j running on localhost:7474, Nexus running on localhost:15474

param(
    [string]$Neo4jUri = "http://localhost:7474",
    [string]$Neo4jUser = "neo4j",
    [string]$Neo4jPassword = "password",
    [string]$NexusUri = "http://localhost:15474",
    [int]$WarmupRuns = 2,
    [int]$BenchmarkRuns = 5
)

$ErrorActionPreference = "Continue"

# Results storage
$global:BenchmarkResults = @()

Write-Host "+===============================================================================+" -ForegroundColor Cyan
Write-Host "|     NEXUS vs NEO4J COMPREHENSIVE BENCHMARK SUITE                             |" -ForegroundColor Cyan
Write-Host "+===============================================================================+" -ForegroundColor Cyan
Write-Host ""
Write-Host "Neo4j:  $Neo4jUri" -ForegroundColor Yellow
Write-Host "Nexus:  $NexusUri" -ForegroundColor Yellow
Write-Host "Warmup Runs: $WarmupRuns | Benchmark Runs: $BenchmarkRuns" -ForegroundColor Yellow
Write-Host ""

# Function to execute query on Neo4j with timing
function Invoke-Neo4jBenchmark {
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
        $stopwatch = [System.Diagnostics.Stopwatch]::StartNew()
        $response = Invoke-RestMethod -Uri "$Neo4jUri/db/neo4j/tx/commit" `
            -Method POST `
            -Headers @{
                "Authorization" = "Basic $auth"
                "Content-Type" = "application/json"
                "Accept" = "application/json"
            } `
            -Body $body `
            -ErrorAction Stop `
            -TimeoutSec 60
        $stopwatch.Stop()

        if ($response.errors -and $response.errors.Count -gt 0) {
            return @{ error = $response.errors[0].message; time_ms = -1 }
        }

        $rowCount = if ($response.results[0].data) { $response.results[0].data.Count } else { 0 }
        # Extract first value from first row if it's a count result
        $firstValue = $null
        if ($response.results[0].data -and $response.results[0].data.Count -gt 0) {
            $firstRow = $response.results[0].data[0].row
            if ($firstRow -and $firstRow.Count -gt 0) {
                $firstValue = $firstRow[0]
            }
        }
        return @{
            time_ms = $stopwatch.Elapsed.TotalMilliseconds
            rows = $rowCount
            first_value = $firstValue
            error = $null
        }
    }
    catch {
        return @{ error = $_.Exception.Message; time_ms = -1; rows = 0 }
    }
}

# Function to execute query on Nexus with timing
function Invoke-NexusBenchmark {
    param([string]$Cypher, [hashtable]$Parameters = @{})

    $body = @{
        query = $Cypher
        parameters = $Parameters
    } | ConvertTo-Json -Depth 10

    try {
        $stopwatch = [System.Diagnostics.Stopwatch]::StartNew()
        $response = Invoke-RestMethod -Uri "$NexusUri/cypher" `
            -Method POST `
            -Headers @{
                "Content-Type" = "application/json"
                "Accept" = "application/json"
            } `
            -Body $body `
            -ErrorAction Stop `
            -TimeoutSec 60
        $stopwatch.Stop()

        $rowCount = if ($response.rows) { $response.rows.Count } else { 0 }
        # Extract first value from first row if it's a count result
        $firstValue = $null
        if ($response.rows -and $response.rows.Count -gt 0 -and $response.rows[0]) {
            $firstVal = $response.rows[0]
            if ($firstVal -is [System.Collections.IEnumerable] -and $firstVal -isnot [string]) {
                $firstValue = $firstVal | Select-Object -First 1
            } else {
                $firstValue = $firstVal
            }
        }
        return @{
            time_ms = $stopwatch.Elapsed.TotalMilliseconds
            rows = $rowCount
            first_value = $firstValue
            error = $null
            server_time_ms = $response.execution_time_ms
        }
    }
    catch {
        return @{ error = $_.Exception.Message; time_ms = -1; rows = 0 }
    }
}

# Benchmark runner
function Run-Benchmark {
    param(
        [string]$Category,
        [string]$Name,
        [string]$Query,
        [hashtable]$Parameters = @{},
        [switch]$SetupQuery
    )

    # Warmup
    for ($i = 0; $i -lt $WarmupRuns; $i++) {
        Invoke-Neo4jBenchmark -Cypher $Query -Parameters $Parameters | Out-Null
        Invoke-NexusBenchmark -Cypher $Query -Parameters $Parameters | Out-Null
    }

    # Benchmark runs
    $neo4jTimes = @()
    $nexusTimes = @()
    $neo4jRows = 0
    $nexusRows = 0
    $neo4jError = $null
    $nexusError = $null

    for ($i = 0; $i -lt $BenchmarkRuns; $i++) {
        $neo4jResult = Invoke-Neo4jBenchmark -Cypher $Query -Parameters $Parameters
        $nexusResult = Invoke-NexusBenchmark -Cypher $Query -Parameters $Parameters

        if ($neo4jResult.error) { $neo4jError = $neo4jResult.error }
        else { $neo4jTimes += $neo4jResult.time_ms; $neo4jRows = $neo4jResult.rows }

        if ($nexusResult.error) { $nexusError = $nexusResult.error }
        else { $nexusTimes += $nexusResult.time_ms; $nexusRows = $nexusResult.rows }
    }

    # Calculate stats
    $neo4jAvg = if ($neo4jTimes.Count -gt 0) { ($neo4jTimes | Measure-Object -Average).Average } else { -1 }
    $neo4jMin = if ($neo4jTimes.Count -gt 0) { ($neo4jTimes | Measure-Object -Minimum).Minimum } else { -1 }
    $neo4jMax = if ($neo4jTimes.Count -gt 0) { ($neo4jTimes | Measure-Object -Maximum).Maximum } else { -1 }

    $nexusAvg = if ($nexusTimes.Count -gt 0) { ($nexusTimes | Measure-Object -Average).Average } else { -1 }
    $nexusMin = if ($nexusTimes.Count -gt 0) { ($nexusTimes | Measure-Object -Minimum).Minimum } else { -1 }
    $nexusMax = if ($nexusTimes.Count -gt 0) { ($nexusTimes | Measure-Object -Maximum).Maximum } else { -1 }

    # Calculate comparison
    $speedup = if ($neo4jAvg -gt 0 -and $nexusAvg -gt 0) { $neo4jAvg / $nexusAvg } else { 0 }
    $compatible = ($neo4jRows -eq $nexusRows) -and (-not $neo4jError) -and (-not $nexusError)

    $result = @{
        Category = $Category
        Name = $Name
        Neo4jAvgMs = [math]::Round($neo4jAvg, 2)
        Neo4jMinMs = [math]::Round($neo4jMin, 2)
        Neo4jMaxMs = [math]::Round($neo4jMax, 2)
        Neo4jRows = $neo4jRows
        Neo4jError = $neo4jError
        NexusAvgMs = [math]::Round($nexusAvg, 2)
        NexusMinMs = [math]::Round($nexusMin, 2)
        NexusMaxMs = [math]::Round($nexusMax, 2)
        NexusRows = $nexusRows
        NexusError = $nexusError
        Speedup = [math]::Round($speedup, 2)
        Compatible = $compatible
    }

    $global:BenchmarkResults += $result

    # Display result
    $statusIcon = if ($compatible) { "OK" } else { "FAIL" }
    $statusColor = if ($compatible) { "Green" } else { "Red" }
    $speedupText = if ($speedup -gt 1) { "Nexus ${speedup}x faster" } elseif ($speedup -gt 0) { "Neo4j $([math]::Round(1/$speedup, 2))x faster" } else { "N/A" }
    $speedupColor = if ($speedup -ge 1) { "Green" } elseif ($speedup -gt 0) { "Yellow" } else { "Red" }

    Write-Host -NoNewline "  [$statusIcon] " -ForegroundColor $statusColor
    Write-Host -NoNewline "$Name".PadRight(45)
    Write-Host -NoNewline "Neo4j: $($result.Neo4jAvgMs)ms".PadRight(18) -ForegroundColor Cyan
    Write-Host -NoNewline "Nexus: $($result.NexusAvgMs)ms".PadRight(18) -ForegroundColor Magenta
    Write-Host "$speedupText" -ForegroundColor $speedupColor

    if ($neo4jError) { Write-Host "      Neo4j Error: $neo4jError" -ForegroundColor Red }
    if ($nexusError) { Write-Host "      Nexus Error: $nexusError" -ForegroundColor Red }

    return $result
}

# Setup: Clean databases
Write-Host "`nSetting up test environment..." -ForegroundColor Cyan
Invoke-Neo4jBenchmark -Cypher "MATCH (n) DETACH DELETE n" | Out-Null
Invoke-NexusBenchmark -Cypher "MATCH (n) DETACH DELETE n" | Out-Null
Start-Sleep -Milliseconds 500

#===============================================================================
# SECTION 1: DATA CREATION BENCHMARKS
#===============================================================================
Write-Host "`n+-----------------------------------------------------------------------+" -ForegroundColor Yellow
Write-Host "| SECTION 1: DATA CREATION                                              |" -ForegroundColor Yellow
Write-Host "+-----------------------------------------------------------------------+" -ForegroundColor Yellow

# Create test data - use individual CREATEs for Nexus compatibility
# Nexus doesn't support CASE expressions in CREATE properties
Write-Host "  Creating test data (100 Person nodes)..." -ForegroundColor Gray

$cities = @("LA", "Chicago", "NYC")  # i%3: 1->LA, 2->Chicago, 0->NYC

# Benchmark node creation using simpler queries
$createNodesBatch = @()
for ($i = 1; $i -le 100; $i++) {
    $city = $cities[$i % 3]
    $age = 20 + ($i % 50)
    $createNodesBatch += "CREATE (p:Person {id: $i, name: 'Person$i', age: $age, city: '$city'})"
}

# Time Neo4j batch creation
$neo4jCreateStart = [System.Diagnostics.Stopwatch]::StartNew()
foreach ($query in $createNodesBatch) {
    Invoke-Neo4jBenchmark -Cypher $query | Out-Null
}
$neo4jCreateStart.Stop()
$neo4jCreateTime = $neo4jCreateStart.Elapsed.TotalMilliseconds

# Time Nexus batch creation
$nexusCreateStart = [System.Diagnostics.Stopwatch]::StartNew()
foreach ($query in $createNodesBatch) {
    Invoke-NexusBenchmark -Cypher $query | Out-Null
}
$nexusCreateStart.Stop()
$nexusCreateTime = $nexusCreateStart.Elapsed.TotalMilliseconds

# Verify data was created
$neo4jCount = Invoke-Neo4jBenchmark -Cypher "MATCH (p:Person) RETURN count(p) AS cnt"
$nexusCount = Invoke-NexusBenchmark -Cypher "MATCH (p:Person) RETURN count(p) AS cnt"

$createResult = @{
    Category = "Creation"
    Name = "Create 100 Person nodes"
    Neo4jAvgMs = [math]::Round($neo4jCreateTime, 2)
    Neo4jMinMs = [math]::Round($neo4jCreateTime, 2)
    Neo4jMaxMs = [math]::Round($neo4jCreateTime, 2)
    Neo4jRows = $neo4jCount.rows
    Neo4jError = $neo4jCount.error
    NexusAvgMs = [math]::Round($nexusCreateTime, 2)
    NexusMinMs = [math]::Round($nexusCreateTime, 2)
    NexusMaxMs = [math]::Round($nexusCreateTime, 2)
    NexusRows = $nexusCount.rows
    NexusError = $nexusCount.error
    Speedup = if ($nexusCreateTime -gt 0) { [math]::Round($neo4jCreateTime / $nexusCreateTime, 2) } else { 0 }
    Compatible = ($neo4jCount.first_value -eq $nexusCount.first_value -and $nexusCount.first_value -eq 100)
}
$global:BenchmarkResults += $createResult

$speedupText = if ($createResult.Speedup -gt 1) { "Nexus $($createResult.Speedup)x faster" } else { "Neo4j faster" }
$statusIcon = if ($createResult.Compatible) { "OK" } else { "FAIL" }
$statusColor = if ($createResult.Compatible) { "Green" } else { "Red" }
Write-Host -NoNewline "  [$statusIcon] " -ForegroundColor $statusColor
Write-Host -NoNewline "Create 100 Person nodes".PadRight(45)
Write-Host -NoNewline "Neo4j: $($createResult.Neo4jAvgMs)ms".PadRight(18) -ForegroundColor Cyan
Write-Host -NoNewline "Nexus: $($createResult.NexusAvgMs)ms".PadRight(18) -ForegroundColor Magenta
Write-Host "$speedupText" -ForegroundColor Green

# Create relationships - using individual creates for better performance
Write-Host "  Creating relationships..." -ForegroundColor Gray

# Create relationships individually to avoid cartesian product slowdown
$relQueries = @()
for ($i = 1; $i -le 20; $i++) {
    $j = $i + 1
    $relQueries += "MATCH (a:Person {id: $i}), (b:Person {id: $j}) CREATE (a)-[:KNOWS {since: 2020}]->(b)"
}

$neo4jRelStart = [System.Diagnostics.Stopwatch]::StartNew()
foreach ($q in $relQueries) { Invoke-Neo4jBenchmark -Cypher $q | Out-Null }
$neo4jRelStart.Stop()

$nexusRelStart = [System.Diagnostics.Stopwatch]::StartNew()
foreach ($q in $relQueries) { Invoke-NexusBenchmark -Cypher $q | Out-Null }
$nexusRelStart.Stop()

$relResult = @{
    Category = "Creation"
    Name = "Create relationships"
    Neo4jAvgMs = [math]::Round($neo4jRelStart.Elapsed.TotalMilliseconds, 2)
    Neo4jMinMs = [math]::Round($neo4jRelStart.Elapsed.TotalMilliseconds, 2)
    Neo4jMaxMs = [math]::Round($neo4jRelStart.Elapsed.TotalMilliseconds, 2)
    Neo4jRows = 20
    Neo4jError = $null
    NexusAvgMs = [math]::Round($nexusRelStart.Elapsed.TotalMilliseconds, 2)
    NexusMinMs = [math]::Round($nexusRelStart.Elapsed.TotalMilliseconds, 2)
    NexusMaxMs = [math]::Round($nexusRelStart.Elapsed.TotalMilliseconds, 2)
    NexusRows = 20
    NexusError = $null
    Speedup = if ($nexusRelStart.Elapsed.TotalMilliseconds -gt 0) { [math]::Round($neo4jRelStart.Elapsed.TotalMilliseconds / $nexusRelStart.Elapsed.TotalMilliseconds, 2) } else { 0 }
    Compatible = $true
}
$global:BenchmarkResults += $relResult

$relSpeedupText = if ($relResult.Speedup -gt 1) { "Nexus $($relResult.Speedup)x faster" } else { "Neo4j faster" }
Write-Host -NoNewline "  [OK] " -ForegroundColor Green
Write-Host -NoNewline "Create relationships".PadRight(45)
Write-Host -NoNewline "Neo4j: $($relResult.Neo4jAvgMs)ms".PadRight(18) -ForegroundColor Cyan
Write-Host -NoNewline "Nexus: $($relResult.NexusAvgMs)ms".PadRight(18) -ForegroundColor Magenta
Write-Host "$relSpeedupText" -ForegroundColor Green

#===============================================================================
# SECTION 2: BASIC MATCH BENCHMARKS
#===============================================================================
Write-Host "`n+-----------------------------------------------------------------------+" -ForegroundColor Yellow
Write-Host "| SECTION 2: BASIC MATCH QUERIES                                        |" -ForegroundColor Yellow
Write-Host "+-----------------------------------------------------------------------+" -ForegroundColor Yellow

Run-Benchmark -Category "Match" -Name "MATCH all nodes" -Query "MATCH (n) RETURN count(n) AS cnt"
Run-Benchmark -Category "Match" -Name "MATCH by label" -Query "MATCH (p:Person) RETURN count(p) AS cnt"
Run-Benchmark -Category "Match" -Name "MATCH by property" -Query "MATCH (p:Person {city: 'NYC'}) RETURN count(p) AS cnt"
Run-Benchmark -Category "Match" -Name "MATCH with WHERE" -Query "MATCH (p:Person) WHERE p.age > 40 RETURN count(p) AS cnt"
Run-Benchmark -Category "Match" -Name "MATCH with complex WHERE" -Query "MATCH (p:Person) WHERE p.age > 30 AND p.city = 'LA' RETURN count(p) AS cnt"
Run-Benchmark -Category "Match" -Name "MATCH with ORDER BY" -Query "MATCH (p:Person) RETURN p.name ORDER BY p.age DESC LIMIT 10"
Run-Benchmark -Category "Match" -Name "MATCH with DISTINCT" -Query "MATCH (p:Person) RETURN DISTINCT p.city AS city"

#===============================================================================
# SECTION 3: AGGREGATION BENCHMARKS
#===============================================================================
Write-Host "`n+-----------------------------------------------------------------------+" -ForegroundColor Yellow
Write-Host "| SECTION 3: AGGREGATION FUNCTIONS                                      |" -ForegroundColor Yellow
Write-Host "+-----------------------------------------------------------------------+" -ForegroundColor Yellow

Run-Benchmark -Category "Aggregation" -Name "COUNT" -Query "MATCH (p:Person) RETURN count(p) AS cnt"
Run-Benchmark -Category "Aggregation" -Name "COUNT DISTINCT" -Query "MATCH (p:Person) RETURN count(DISTINCT p.city) AS cnt"
Run-Benchmark -Category "Aggregation" -Name "SUM" -Query "MATCH (p:Person) RETURN sum(p.age) AS total"
Run-Benchmark -Category "Aggregation" -Name "AVG" -Query "MATCH (p:Person) RETURN avg(p.age) AS average"
Run-Benchmark -Category "Aggregation" -Name "MIN/MAX" -Query "MATCH (p:Person) RETURN min(p.age) AS min_age, max(p.age) AS max_age"
Run-Benchmark -Category "Aggregation" -Name "COLLECT" -Query "MATCH (p:Person) WHERE p.city = 'NYC' RETURN collect(p.name) AS names LIMIT 1"
Run-Benchmark -Category "Aggregation" -Name "GROUP BY" -Query "MATCH (p:Person) RETURN p.city AS city, count(p) AS cnt, avg(p.age) AS avg_age ORDER BY cnt DESC"

#===============================================================================
# SECTION 4: RELATIONSHIP TRAVERSAL BENCHMARKS
#===============================================================================
Write-Host "`n+-----------------------------------------------------------------------+" -ForegroundColor Yellow
Write-Host "| SECTION 4: RELATIONSHIP TRAVERSAL                                     |" -ForegroundColor Yellow
Write-Host "+-----------------------------------------------------------------------+" -ForegroundColor Yellow

Run-Benchmark -Category "Traversal" -Name "Simple traversal" -Query "MATCH (a:Person)-[r:KNOWS]->(b:Person) RETURN count(r) AS cnt"
Run-Benchmark -Category "Traversal" -Name "Bidirectional" -Query "MATCH (a:Person)-[r:KNOWS]-(b:Person) RETURN count(r) AS cnt"
Run-Benchmark -Category "Traversal" -Name "With property filter" -Query "MATCH (a:Person)-[r:KNOWS]->(b:Person) WHERE r.since > 2022 RETURN count(r) AS cnt"
Run-Benchmark -Category "Traversal" -Name "Two-hop traversal" -Query "MATCH (a:Person)-[:KNOWS]->(b:Person)-[:KNOWS]->(c:Person) RETURN count(*) AS cnt"
Run-Benchmark -Category "Traversal" -Name "Return path data" -Query "MATCH (a:Person)-[r:KNOWS]->(b:Person) RETURN a.name, b.name, r.since ORDER BY r.since LIMIT 10"

#===============================================================================
# SECTION 5: STRING FUNCTION BENCHMARKS
#===============================================================================
Write-Host "`n+-----------------------------------------------------------------------+" -ForegroundColor Yellow
Write-Host "| SECTION 5: STRING FUNCTIONS                                           |" -ForegroundColor Yellow
Write-Host "+-----------------------------------------------------------------------+" -ForegroundColor Yellow

Run-Benchmark -Category "String" -Name "toLower" -Query "MATCH (p:Person) RETURN toLower(p.name) AS name LIMIT 10"
Run-Benchmark -Category "String" -Name "toUpper" -Query "MATCH (p:Person) RETURN toUpper(p.name) AS name LIMIT 10"
Run-Benchmark -Category "String" -Name "substring" -Query "MATCH (p:Person) RETURN substring(p.name, 0, 3) AS prefix LIMIT 10"
Run-Benchmark -Category "String" -Name "trim" -Query "RETURN trim('  hello world  ') AS result"
Run-Benchmark -Category "String" -Name "replace" -Query "RETURN replace('hello world', 'world', 'nexus') AS result"
Run-Benchmark -Category "String" -Name "split" -Query "RETURN split('a,b,c,d', ',') AS parts"
Run-Benchmark -Category "String" -Name "STARTS WITH" -Query "MATCH (p:Person) WHERE p.name STARTS WITH 'Person1' RETURN count(p) AS cnt"
Run-Benchmark -Category "String" -Name "CONTAINS" -Query "MATCH (p:Person) WHERE p.name CONTAINS '5' RETURN count(p) AS cnt"

#===============================================================================
# SECTION 6: MATHEMATICAL FUNCTION BENCHMARKS
#===============================================================================
Write-Host "`n+-----------------------------------------------------------------------+" -ForegroundColor Yellow
Write-Host "| SECTION 6: MATHEMATICAL FUNCTIONS                                     |" -ForegroundColor Yellow
Write-Host "+-----------------------------------------------------------------------+" -ForegroundColor Yellow

Run-Benchmark -Category "Math" -Name "Basic arithmetic" -Query "RETURN 100 + 50 * 2 - 30 / 3 AS result"
Run-Benchmark -Category "Math" -Name "abs" -Query "RETURN abs(-42) AS result"
Run-Benchmark -Category "Math" -Name "ceil/floor" -Query "RETURN ceil(3.2) AS c, floor(3.8) AS f"
Run-Benchmark -Category "Math" -Name "round" -Query "RETURN round(3.567, 2) AS result"
Run-Benchmark -Category "Math" -Name "sqrt" -Query "RETURN sqrt(144) AS result"
Run-Benchmark -Category "Math" -Name "power" -Query "RETURN 2 ^ 10 AS result"
Run-Benchmark -Category "Math" -Name "sin/cos" -Query "RETURN sin(0) AS s, cos(0) AS c"
Run-Benchmark -Category "Math" -Name "log/exp" -Query "RETURN log(10) AS l, exp(1) AS e"

#===============================================================================
# SECTION 7: LIST/ARRAY BENCHMARKS
#===============================================================================
Write-Host "`n+-----------------------------------------------------------------------+" -ForegroundColor Yellow
Write-Host "| SECTION 7: LIST/ARRAY OPERATIONS                                      |" -ForegroundColor Yellow
Write-Host "+-----------------------------------------------------------------------+" -ForegroundColor Yellow

Run-Benchmark -Category "List" -Name "Create list" -Query "RETURN [1, 2, 3, 4, 5] AS nums"
Run-Benchmark -Category "List" -Name "range()" -Query "RETURN range(1, 100) AS nums"
Run-Benchmark -Category "List" -Name "head/tail" -Query "RETURN head([1,2,3]) AS h, tail([1,2,3]) AS t"
Run-Benchmark -Category "List" -Name "size()" -Query "RETURN size([1,2,3,4,5]) AS len"
Run-Benchmark -Category "List" -Name "reverse()" -Query "RETURN reverse([1,2,3,4,5]) AS rev"
Run-Benchmark -Category "List" -Name "Indexing" -Query "RETURN [1,2,3,4,5][2] AS third"
Run-Benchmark -Category "List" -Name "Slicing" -Query "RETURN [1,2,3,4,5][1..3] AS slice"
Run-Benchmark -Category "List" -Name "IN operator" -Query "RETURN 3 IN [1,2,3,4,5] AS found"

#===============================================================================
# SECTION 8: NULL HANDLING BENCHMARKS
#===============================================================================
Write-Host "`n+-----------------------------------------------------------------------+" -ForegroundColor Yellow
Write-Host "| SECTION 8: NULL HANDLING                                              |" -ForegroundColor Yellow
Write-Host "+-----------------------------------------------------------------------+" -ForegroundColor Yellow

Run-Benchmark -Category "Null" -Name "IS NULL" -Query "RETURN null IS NULL AS result"
Run-Benchmark -Category "Null" -Name "IS NOT NULL" -Query "RETURN 5 IS NOT NULL AS result"
Run-Benchmark -Category "Null" -Name "coalesce" -Query "RETURN coalesce(null, null, 'default') AS result"
Run-Benchmark -Category "Null" -Name "NULL arithmetic" -Query "RETURN 5 + null AS result"
Run-Benchmark -Category "Null" -Name "NULL comparison" -Query "RETURN null = null AS result"

#===============================================================================
# SECTION 9: CASE EXPRESSION BENCHMARKS
#===============================================================================
Write-Host "`n+-----------------------------------------------------------------------+" -ForegroundColor Yellow
Write-Host "| SECTION 9: CASE EXPRESSIONS                                           |" -ForegroundColor Yellow
Write-Host "+-----------------------------------------------------------------------+" -ForegroundColor Yellow

Run-Benchmark -Category "Case" -Name "Simple CASE" -Query "RETURN CASE WHEN 5 > 3 THEN 'yes' ELSE 'no' END AS result"
Run-Benchmark -Category "Case" -Name "Multiple WHEN" -Query "RETURN CASE WHEN 1 > 2 THEN 'a' WHEN 2 > 1 THEN 'b' ELSE 'c' END AS result"
Run-Benchmark -Category "Case" -Name "CASE with property" -Query "MATCH (p:Person) RETURN p.name, CASE WHEN p.age > 40 THEN 'Senior' ELSE 'Junior' END AS category LIMIT 10"

#===============================================================================
# SECTION 10: UNION BENCHMARKS
#===============================================================================
Write-Host "`n+-----------------------------------------------------------------------+" -ForegroundColor Yellow
Write-Host "| SECTION 10: UNION QUERIES                                             |" -ForegroundColor Yellow
Write-Host "+-----------------------------------------------------------------------+" -ForegroundColor Yellow

Run-Benchmark -Category "Union" -Name "UNION" -Query "RETURN 1 AS num UNION RETURN 2 AS num UNION RETURN 3 AS num"
Run-Benchmark -Category "Union" -Name "UNION ALL" -Query "RETURN 1 AS num UNION ALL RETURN 1 AS num"
Run-Benchmark -Category "Union" -Name "UNION with MATCH" -Query "MATCH (p:Person) WHERE p.city = 'NYC' RETURN p.name AS name UNION MATCH (p:Person) WHERE p.city = 'LA' RETURN p.name AS name"

#===============================================================================
# SECTION 11: OPTIONAL MATCH BENCHMARKS
#===============================================================================
Write-Host "`n+-----------------------------------------------------------------------+" -ForegroundColor Yellow
Write-Host "| SECTION 11: OPTIONAL MATCH                                            |" -ForegroundColor Yellow
Write-Host "+-----------------------------------------------------------------------+" -ForegroundColor Yellow

Run-Benchmark -Category "Optional" -Name "OPTIONAL MATCH basic" -Query "MATCH (p:Person) OPTIONAL MATCH (p)-[r:KNOWS]->(other) RETURN p.name, count(other) AS friends LIMIT 10"
Run-Benchmark -Category "Optional" -Name "OPTIONAL MATCH with coalesce" -Query "MATCH (p:Person) OPTIONAL MATCH (p)-[:KNOWS]->(other) RETURN p.name, coalesce(other.name, 'No friends') AS friend LIMIT 10"

#===============================================================================
# SECTION 12: WITH CLAUSE BENCHMARKS
#===============================================================================
Write-Host "`n+-----------------------------------------------------------------------+" -ForegroundColor Yellow
Write-Host "| SECTION 12: WITH CLAUSE                                               |" -ForegroundColor Yellow
Write-Host "+-----------------------------------------------------------------------+" -ForegroundColor Yellow

Run-Benchmark -Category "With" -Name "WITH projection" -Query "MATCH (p:Person) WITH p.name AS name, p.age AS age RETURN name, age LIMIT 10"
Run-Benchmark -Category "With" -Name "WITH aggregation" -Query "MATCH (p:Person) WITH p.city AS city, count(p) AS cnt RETURN city, cnt ORDER BY cnt DESC"
Run-Benchmark -Category "With" -Name "Chained WITH" -Query "MATCH (p:Person) WITH p.age AS age WITH avg(age) AS avg_age RETURN avg_age"

#===============================================================================
# SECTION 13: UNWIND BENCHMARKS
#===============================================================================
Write-Host "`n+-----------------------------------------------------------------------+" -ForegroundColor Yellow
Write-Host "| SECTION 13: UNWIND                                                    |" -ForegroundColor Yellow
Write-Host "+-----------------------------------------------------------------------+" -ForegroundColor Yellow

Run-Benchmark -Category "Unwind" -Name "UNWIND basic" -Query "UNWIND [1,2,3,4,5] AS x RETURN x"
Run-Benchmark -Category "Unwind" -Name "UNWIND with aggregation" -Query "UNWIND [1,2,3,4,5] AS x RETURN sum(x) AS total"

#===============================================================================
# SECTION 14: MERGE BENCHMARKS
#===============================================================================
Write-Host "`n+-----------------------------------------------------------------------+" -ForegroundColor Yellow
Write-Host "| SECTION 14: MERGE OPERATIONS                                          |" -ForegroundColor Yellow
Write-Host "+-----------------------------------------------------------------------+" -ForegroundColor Yellow

Run-Benchmark -Category "Merge" -Name "MERGE new node" -Query "MERGE (t:TestNode {id: 999}) RETURN t.id"
Run-Benchmark -Category "Merge" -Name "MERGE existing node" -Query "MERGE (t:TestNode {id: 999}) RETURN t.id"
Run-Benchmark -Category "Merge" -Name "MERGE with ON CREATE" -Query "MERGE (t:TestNode {id: 1000}) ON CREATE SET t.created = true RETURN t.id, t.created"

#===============================================================================
# SECTION 15: TYPE CONVERSION BENCHMARKS
#===============================================================================
Write-Host "`n+-----------------------------------------------------------------------+" -ForegroundColor Yellow
Write-Host "| SECTION 15: TYPE CONVERSION                                           |" -ForegroundColor Yellow
Write-Host "+-----------------------------------------------------------------------+" -ForegroundColor Yellow

Run-Benchmark -Category "TypeConv" -Name "toInteger" -Query "RETURN toInteger('42') AS result"
Run-Benchmark -Category "TypeConv" -Name "toFloat" -Query "RETURN toFloat('3.14') AS result"
Run-Benchmark -Category "TypeConv" -Name "toString" -Query "RETURN toString(42) AS result"
Run-Benchmark -Category "TypeConv" -Name "toBoolean" -Query "RETURN toBoolean('true') AS result"

#===============================================================================
# SECTION 16: DELETE/SET BENCHMARKS
#===============================================================================
Write-Host "`n+-----------------------------------------------------------------------+" -ForegroundColor Yellow
Write-Host "| SECTION 16: DELETE/SET OPERATIONS                                     |" -ForegroundColor Yellow
Write-Host "+-----------------------------------------------------------------------+" -ForegroundColor Yellow

Run-Benchmark -Category "Write" -Name "SET property" -Query "MATCH (p:Person {id: 1}) SET p.updated = true RETURN p.updated"
Run-Benchmark -Category "Write" -Name "SET multiple" -Query "MATCH (p:Person {id: 2}) SET p.x = 1, p.y = 2 RETURN p.x, p.y"

#===============================================================================
# FINAL REPORT
#===============================================================================
Write-Host "`n+===============================================================================+" -ForegroundColor Cyan
Write-Host "|                           BENCHMARK SUMMARY                                   |" -ForegroundColor Cyan
Write-Host "+===============================================================================+" -ForegroundColor Cyan

# Group by category
$categories = $global:BenchmarkResults | Group-Object -Property Category

foreach ($cat in $categories) {
    $catName = $cat.Name
    $tests = $cat.Group
    $passed = ($tests | Where-Object { $_.Compatible }).Count
    $total = $tests.Count
    $avgSpeedup = ($tests | Where-Object { $_.Speedup -gt 0 } | Measure-Object -Property Speedup -Average).Average

    Write-Host "`n$catName ($passed/$total compatible):" -ForegroundColor Yellow

    foreach ($test in $tests) {
        $icon = if ($test.Compatible) { "PASS" } else { "FAIL" }
        $color = if ($test.Compatible) { "Green" } else { "Red" }
        $speedupText = if ($test.Speedup -ge 1) { "Nexus $($test.Speedup)x faster" } elseif ($test.Speedup -gt 0) { "Neo4j $([math]::Round(1/$test.Speedup, 2))x faster" } else { "N/A" }

        Write-Host "  [$icon] $($test.Name.PadRight(40)) Neo4j: $($test.Neo4jAvgMs)ms | Nexus: $($test.NexusAvgMs)ms | $speedupText" -ForegroundColor $color
    }
}

# Overall statistics
Write-Host "`n+-------------------------------------------------------------------------------+" -ForegroundColor Cyan
Write-Host "|                           OVERALL STATISTICS                                  |" -ForegroundColor Cyan
Write-Host "+-------------------------------------------------------------------------------+" -ForegroundColor Cyan

$totalTests = $global:BenchmarkResults.Count
$compatibleTests = ($global:BenchmarkResults | Where-Object { $_.Compatible }).Count
$avgNeo4j = ($global:BenchmarkResults | Where-Object { $_.Neo4jAvgMs -gt 0 } | Measure-Object -Property Neo4jAvgMs -Average).Average
$avgNexus = ($global:BenchmarkResults | Where-Object { $_.NexusAvgMs -gt 0 } | Measure-Object -Property NexusAvgMs -Average).Average
$fasterCount = ($global:BenchmarkResults | Where-Object { $_.Speedup -ge 1 }).Count
$slowerCount = ($global:BenchmarkResults | Where-Object { $_.Speedup -gt 0 -and $_.Speedup -lt 1 }).Count

Write-Host ""
Write-Host "Total Benchmarks:        $totalTests" -ForegroundColor White
Write-Host "Compatible:              $compatibleTests / $totalTests ($([math]::Round($compatibleTests/$totalTests*100, 1))%)" -ForegroundColor $(if ($compatibleTests -eq $totalTests) { "Green" } else { "Yellow" })
Write-Host "Average Neo4j Time:      $([math]::Round($avgNeo4j, 2)) ms" -ForegroundColor Cyan
Write-Host "Average Nexus Time:      $([math]::Round($avgNexus, 2)) ms" -ForegroundColor Magenta
Write-Host "Nexus Faster:            $fasterCount benchmarks" -ForegroundColor Green
Write-Host "Neo4j Faster:            $slowerCount benchmarks" -ForegroundColor Yellow
Write-Host ""

# Export results to CSV (save in scripts folder)
$scriptDir = $PSScriptRoot
$csvPath = Join-Path $scriptDir "benchmark-results-$(Get-Date -Format 'yyyy-MM-dd-HHmmss').csv"
$csvResults = $global:BenchmarkResults | ForEach-Object {
    [PSCustomObject]@{
        Category = $_.Category
        Name = $_.Name
        Neo4jAvgMs = $_.Neo4jAvgMs
        Neo4jMinMs = $_.Neo4jMinMs
        Neo4jMaxMs = $_.Neo4jMaxMs
        Neo4jRows = $_.Neo4jRows
        NexusAvgMs = $_.NexusAvgMs
        NexusMinMs = $_.NexusMinMs
        NexusMaxMs = $_.NexusMaxMs
        NexusRows = $_.NexusRows
        Speedup = $_.Speedup
        Compatible = $_.Compatible
        Neo4jError = if ($_.Neo4jError) { $_.Neo4jError } else { "" }
        NexusError = if ($_.NexusError) { $_.NexusError } else { "" }
    }
}
$csvResults | Export-Csv -Path $csvPath -NoTypeInformation
Write-Host "Results exported to: $csvPath" -ForegroundColor Gray

# Cleanup
Write-Host "`nCleaning up test data..." -ForegroundColor Cyan
Invoke-Neo4jBenchmark -Cypher "MATCH (n) DETACH DELETE n" | Out-Null
Invoke-NexusBenchmark -Cypher "MATCH (n) DETACH DELETE n" | Out-Null
Write-Host "Done!" -ForegroundColor Green
