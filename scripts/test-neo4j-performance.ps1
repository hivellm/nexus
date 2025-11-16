# Neo4j Performance Comparison Test
# Compares query execution time between Nexus and Neo4j

param(
    [string]$Neo4jUri = "http://localhost:7474",
    [string]$Neo4jUser = "neo4j",
    [string]$Neo4jPassword = "password",
    [string]$NexusUri = "http://localhost:15474",
    [int]$Iterations = 10
)

Write-Host "=== Neo4j Performance Comparison Test ===" -ForegroundColor Cyan
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
        $stopwatch = [System.Diagnostics.Stopwatch]::StartNew()
        $response = Invoke-RestMethod -Uri "$Neo4jUri/db/neo4j/tx/commit" `
            -Method POST `
            -Headers @{
                "Authorization" = "Basic $auth"
                "Content-Type" = "application/json"
            } `
            -Body $body `
            -ErrorAction Stop
        $stopwatch.Stop()
        
        return @{
            Success = $true
            ElapsedMs = $stopwatch.ElapsedMilliseconds
            Result = $response.results[0]
        }
    }
    catch {
        return @{
            Success = $false
            ElapsedMs = 0
            Error = $_.Exception.Message
        }
    }
}

function Invoke-NexusQuery {
    param([string]$Cypher)
    
    $body = @{
        query = $Cypher
    } | ConvertTo-Json
    
    try {
        $stopwatch = [System.Diagnostics.Stopwatch]::StartNew()
        $response = Invoke-RestMethod -Uri "$NexusUri/cypher" `
            -Method POST `
            -Headers @{"Content-Type" = "application/json"} `
            -Body $body `
            -ErrorAction Stop
        $stopwatch.Stop()
        
        return @{
            Success = $true
            ElapsedMs = $stopwatch.ElapsedMilliseconds
            Result = $response
        }
    }
    catch {
        return @{
            Success = $false
            ElapsedMs = 0
            Error = $_.Exception.Message
        }
    }
}

# Performance test queries
$performanceQueries = @(
    @{
        name = "Simple count"
        cypher = "MATCH (n:Person) RETURN count(*) AS count"
        description = "Count all Person nodes"
    },
    @{
        name = "Aggregation with WHERE"
        cypher = "MATCH (n:Person) WHERE n.age > 25 RETURN avg(n.age) AS avg_age"
        description = "Average age with WHERE filter"
    },
    @{
        name = "Multiple aggregations"
        cypher = "MATCH (n:Person) RETURN count(n) AS total, sum(n.age) AS total_age, avg(n.age) AS avg_age"
        description = "Multiple aggregation functions"
    },
    @{
        name = "ORDER BY with LIMIT"
        cypher = "MATCH (n:Person) RETURN n.name ORDER BY n.age DESC LIMIT 10"
        description = "Ordered results with limit"
    },
    @{
        name = "Complex WHERE"
        cypher = "MATCH (n:Person) WHERE n.age > 25 AND n.city = 'NYC' RETURN count(n) AS count"
        description = "Complex WHERE clause with AND"
    },
    @{
        name = "Relationship traversal"
        cypher = "MATCH (a:Person)-[r:KNOWS]->(b:Person) RETURN count(r) AS count"
        description = "Count relationships"
    }
)

Write-Host "Running performance tests ($Iterations iterations per query)..." -ForegroundColor Yellow
Write-Host ""

$results = @()

foreach ($query in $performanceQueries) {
    Write-Host "Testing: $($query.name)" -ForegroundColor Cyan
    Write-Host "  Query: $($query.cypher)"
    Write-Host "  Description: $($query.description)"
    
    $neo4jTimes = @()
    $nexusTimes = @()
    
    # Warmup
    Invoke-Neo4jQuery -Cypher $query.cypher | Out-Null
    Invoke-NexusQuery -Cypher $query.cypher | Out-Null
    
    # Run iterations
    for ($i = 1; $i -le $Iterations; $i++) {
        $neo4jResult = Invoke-Neo4jQuery -Cypher $query.cypher
        $nexusResult = Invoke-NexusQuery -Cypher $query.cypher
        
        if ($neo4jResult.Success) {
            $neo4jTimes += $neo4jResult.ElapsedMs
        }
        
        if ($nexusResult.Success) {
            $nexusTimes += $nexusResult.ElapsedMs
        }
        
        Start-Sleep -Milliseconds 100
    }
    
    if ($neo4jTimes.Count -eq 0 -or $nexusTimes.Count -eq 0) {
        Write-Host "  [SKIP] One or both queries failed" -ForegroundColor Yellow
        Write-Host ""
        continue
    }
    
    $neo4jAvg = ($neo4jTimes | Measure-Object -Average).Average
    $nexusAvg = ($nexusTimes | Measure-Object -Average).Average
    $neo4jMin = ($neo4jTimes | Measure-Object -Minimum).Minimum
    $nexusMin = ($nexusTimes | Measure-Object -Minimum).Minimum
    $neo4jMax = ($neo4jTimes | Measure-Object -Maximum).Maximum
    $nexusMax = ($nexusTimes | Measure-Object -Maximum).Maximum
    
    $speedup = if ($nexusAvg -gt 0) { [math]::Round($neo4jAvg / $nexusAvg, 2) } else { 0 }
    $slower = if ($neo4jAvg -gt 0) { [math]::Round($nexusAvg / $neo4jAvg, 2) } else { 0 }
    
    Write-Host "  Neo4j: Avg=$([math]::Round($neo4jAvg, 2))ms Min=$neo4jMin ms Max=$neo4jMax ms" -ForegroundColor Yellow
    Write-Host "  Nexus:  Avg=$([math]::Round($nexusAvg, 2))ms Min=$nexusMin ms Max=$nexusMax ms" -ForegroundColor Yellow
    
    if ($nexusAvg -lt $neo4jAvg) {
        Write-Host "  [FASTER] Nexus is $speedup x faster" -ForegroundColor Green
    } elseif ($nexusAvg -gt $neo4jAvg) {
        Write-Host "  [SLOWER] Nexus is $slower x slower" -ForegroundColor Red
    } else {
        Write-Host "  [SAME] Performance is similar" -ForegroundColor Cyan
    }
    
    Write-Host ""
    
    $results += @{
        name = $query.name
        query = $query.cypher
        neo4j_avg_ms = [math]::Round($neo4jAvg, 2)
        nexus_avg_ms = [math]::Round($nexusAvg, 2)
        neo4j_min_ms = $neo4jMin
        nexus_min_ms = $nexusMin
        neo4j_max_ms = $neo4jMax
        nexus_max_ms = $nexusMax
        speedup = $speedup
        slower = $slower
    }
}

Write-Host "========================================" -ForegroundColor Cyan
Write-Host "PERFORMANCE TEST SUMMARY" -ForegroundColor Cyan
Write-Host "========================================" -ForegroundColor Cyan

$fasterCount = ($results | Where-Object { $_.nexus_avg_ms -lt $_.neo4j_avg_ms }).Count
$slowerCount = ($results | Where-Object { $_.nexus_avg_ms -gt $_.neo4j_avg_ms }).Count
$sameCount = ($results | Where-Object { $_.nexus_avg_ms -eq $_.neo4j_avg_ms }).Count

Write-Host "Total Queries Tested: $($results.Count)"
Write-Host "Nexus Faster: $fasterCount" -ForegroundColor Green
Write-Host "Nexus Slower: $slowerCount" -ForegroundColor Red
Write-Host "Similar Performance: $sameCount" -ForegroundColor Cyan

# Export results
$reportDir = Join-Path $PSScriptRoot ".." "tests" "cross-compatibility" "reports"
if (-not (Test-Path $reportDir)) {
    New-Item -ItemType Directory -Path $reportDir -Force | Out-Null
}
$reportPath = Join-Path $reportDir "neo4j-performance-comparison-report.json"
$results | ConvertTo-Json -Depth 3 | Out-File $reportPath
Write-Host ""
Write-Host "Detailed report saved to: $reportPath" -ForegroundColor Cyan

