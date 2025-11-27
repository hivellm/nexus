# Comprehensive Performance Benchmark: Nexus vs Neo4j
# Tests various query patterns and operations to compare performance
# 
# Usage: ./benchmark-nexus-vs-neo4j.ps1

$ErrorActionPreference = "Continue"

# Configuration
$NexusUrl = "http://localhost:15474"
$Neo4jUrl = "http://localhost:7474"
$Neo4jUser = "neo4j"
$Neo4jPassword = "password"

# Results storage
$global:BenchmarkResults = @()

# Helper function to execute Nexus query
function Execute-NexusQuery {
    param(
        [string]$Query,
        [int]$Iterations = 1
    )
    
    $totalTime = 0
    $successCount = 0
    $errorCount = 0
    
    for ($i = 0; $i -lt $Iterations; $i++) {
        try {
            $startTime = Get-Date
            $response = Invoke-RestMethod -Uri "$NexusUrl/cypher" `
                -Method POST `
                -ContentType "application/json" `
                -Body (@{ query = $Query } | ConvertTo-Json) `
                -TimeoutSec 30 `
                -ErrorAction Stop
            
            $endTime = Get-Date
            $duration = ($endTime - $startTime).TotalMilliseconds
            $totalTime += $duration
            $successCount++
        }
        catch {
            $errorCount++
            Write-Host "  [ERROR] Nexus query failed: $_" -ForegroundColor Red
        }
    }
    
    return @{
        AvgTime = if ($successCount -gt 0) { $totalTime / $successCount } else { 0 }
        TotalTime = $totalTime
        SuccessCount = $successCount
        ErrorCount = $errorCount
        Iterations = $Iterations
    }
}

# Helper function to execute Neo4j query
function Execute-Neo4jQuery {
    param(
        [string]$Query,
        [int]$Iterations = 1
    )
    
    $totalTime = 0
    $successCount = 0
    $errorCount = 0
    
    $headers = @{
        "Authorization" = "Basic " + [Convert]::ToBase64String([Text.Encoding]::ASCII.GetBytes("${Neo4jUser}:${Neo4jPassword}"))
        "Content-Type" = "application/json"
    }
    
    for ($i = 0; $i -lt $Iterations; $i++) {
        try {
            $startTime = Get-Date
            $body = @{
                statements = @(
                    @{
                        statement = $Query
                    }
                )
            } | ConvertTo-Json
            
            $response = Invoke-RestMethod -Uri "$Neo4jUrl/db/neo4j/tx/commit" `
                -Method POST `
                -Headers $headers `
                -Body $body `
                -TimeoutSec 30 `
                -ErrorAction Stop
            
            $endTime = Get-Date
            $duration = ($endTime - $startTime).TotalMilliseconds
            $totalTime += $duration
            $successCount++
        }
        catch {
            $errorCount++
            Write-Host "  [ERROR] Neo4j query failed: $_" -ForegroundColor Red
        }
    }
    
    return @{
        AvgTime = if ($successCount -gt 0) { $totalTime / $successCount } else { 0 }
        TotalTime = $totalTime
        SuccessCount = $successCount
        ErrorCount = $errorCount
        Iterations = $Iterations
    }
}

# Benchmark function
function Run-Benchmark {
    param(
        [string]$Name,
        [string]$Query,
        [int]$Iterations = 10,
        [string]$Category = "General"
    )
    
    Write-Host "`n[Benchmark] $Name" -ForegroundColor Cyan
    Write-Host "  Query: $Query" -ForegroundColor Gray
    Write-Host "  Iterations: $Iterations" -ForegroundColor Gray
    
    # Warm-up runs (not counted)
    Write-Host "  Warming up..." -ForegroundColor Yellow
    Execute-NexusQuery -Query $Query -Iterations 2 | Out-Null
    Execute-Neo4jQuery -Query $Query -Iterations 2 | Out-Null
    
    # Actual benchmark
    Write-Host "  Running Nexus benchmark..." -ForegroundColor Yellow
    $nexusResult = Execute-NexusQuery -Query $Query -Iterations $Iterations
    
    Write-Host "  Running Neo4j benchmark..." -ForegroundColor Yellow
    $neo4jResult = Execute-Neo4jQuery -Query $Query -Iterations $Iterations
    
    # Calculate improvement
    $improvement = 0
    $winner = "Tie"
    if ($nexusResult.AvgTime -gt 0 -and $neo4jResult.AvgTime -gt 0) {
        if ($nexusResult.AvgTime -lt $neo4jResult.AvgTime) {
            $improvement = (($neo4jResult.AvgTime - $nexusResult.AvgTime) / $neo4jResult.AvgTime) * 100
            $winner = "Nexus"
        }
        else {
            $improvement = (($nexusResult.AvgTime - $neo4jResult.AvgTime) / $nexusResult.AvgTime) * 100
            $winner = "Neo4j"
        }
    }
    
    # Display results
    Write-Host "  Results:" -ForegroundColor Green
    Write-Host "    Nexus:  $([math]::Round($nexusResult.AvgTime, 2))ms avg ($($nexusResult.SuccessCount)/$Iterations success)" -ForegroundColor $(if ($winner -eq "Nexus") { "Green" } else { "White" })
    Write-Host "    Neo4j: $([math]::Round($neo4jResult.AvgTime, 2))ms avg ($($neo4jResult.SuccessCount)/$Iterations success)" -ForegroundColor $(if ($winner -eq "Neo4j") { "Green" } else { "White" })
    Write-Host "    Winner: $winner ($([math]::Round($improvement, 1))% faster)" -ForegroundColor $(if ($winner -eq "Nexus") { "Green" } else { "Yellow" })
    
    # Store results
    $global:BenchmarkResults += @{
        Name = $Name
        Category = $Category
        Query = $Query
        NexusAvgTime = $nexusResult.AvgTime
        Neo4jAvgTime = $neo4jResult.AvgTime
        NexusSuccess = $nexusResult.SuccessCount
        Neo4jSuccess = $neo4jResult.SuccessCount
        Winner = $winner
        Improvement = $improvement
        Iterations = $Iterations
    }
}

# Helper function to execute Neo4j batch (multiple statements in one transaction)
function Execute-Neo4jBatch {
    param(
        [array]$Statements
    )

    $headers = @{
        "Authorization" = "Basic " + [Convert]::ToBase64String([Text.Encoding]::ASCII.GetBytes("${Neo4jUser}:${Neo4jPassword}"))
        "Content-Type" = "application/json"
    }

    $stmts = $Statements | ForEach-Object { @{ statement = $_ } }
    $body = @{ statements = $stmts } | ConvertTo-Json -Depth 10

    try {
        $response = Invoke-RestMethod -Uri "$Neo4jUrl/db/neo4j/tx/commit" `
            -Method POST `
            -Headers $headers `
            -Body $body `
            -TimeoutSec 120 `
            -ErrorAction Stop
        return $true
    }
    catch {
        Write-Host "  [ERROR] Neo4j batch failed: $_" -ForegroundColor Red
        return $false
    }
}

# Setup test data - OPTIMIZED with batch operations
function Setup-TestData {
    Write-Host "`n[Setup] Creating test data..." -ForegroundColor Cyan

    # Clear existing data
    Write-Host "  Clearing existing data..." -ForegroundColor Yellow
    Execute-NexusQuery -Query "MATCH (n) DETACH DELETE n" -Iterations 1 | Out-Null
    Execute-Neo4jQuery -Query "MATCH (n) DETACH DELETE n" -Iterations 1 | Out-Null

    # Generate Person data (reduced for faster setup - 200 persons)
    $cities = @("New York", "London", "Tokyo", "Paris", "Berlin")
    $personData = @()
    for ($i = 0; $i -lt 200; $i++) {
        $personData += @{
            name = "Person$i"
            age = Get-Random -Minimum 18 -Maximum 80
            city = $cities[(Get-Random -Maximum 5)]
        }
    }

    # Generate Company data (reduced - 20 companies)
    $industries = @("Tech", "Finance", "Healthcare", "Education", "Retail")
    $companyData = @()
    for ($i = 0; $i -lt 20; $i++) {
        $companyData += @{
            name = "Company$i"
            industry = $industries[(Get-Random -Maximum 5)]
        }
    }

    # Generate Relationship data (reduced - 100 relationships)
    $relData = @()
    for ($i = 0; $i -lt 100; $i++) {
        $relData += @{
            personId = Get-Random -Minimum 0 -Maximum 200
            companyId = Get-Random -Minimum 0 -Maximum 20
            since = Get-Random -Minimum 2010 -Maximum 2024
        }
    }

    # ========== NEXUS: Use multiple CREATE in single query ==========
    Write-Host "  Creating 200 Person nodes in Nexus (batch)..." -ForegroundColor Yellow
    $nexusPersonStart = Get-Date

    # Nexus: Create all 200 nodes in 2 batches of 100
    for ($batch = 0; $batch -lt 200; $batch += 100) {
        $batchEnd = [Math]::Min($batch + 100, 200)
        $creates = @()
        for ($i = $batch; $i -lt $batchEnd; $i++) {
            $p = $personData[$i]
            $creates += "CREATE (:Person {name: '$($p.name)', age: $($p.age), city: '$($p.city)'})"
        }
        $batchQuery = $creates -join " "
        Execute-NexusQuery -Query $batchQuery -Iterations 1 | Out-Null
    }
    $nexusPersonTime = ((Get-Date) - $nexusPersonStart).TotalSeconds
    Write-Host "    Nexus Person nodes: $([math]::Round($nexusPersonTime, 2))s" -ForegroundColor Gray

    Write-Host "  Creating 20 Company nodes in Nexus (batch)..." -ForegroundColor Yellow
    $nexusCompanyStart = Get-Date

    # All 20 companies in one query
    $creates = @()
    foreach ($c in $companyData) {
        $creates += "CREATE (:Company {name: '$($c.name)', industry: '$($c.industry)'})"
    }
    $batchQuery = $creates -join " "
    Execute-NexusQuery -Query $batchQuery -Iterations 1 | Out-Null

    $nexusCompanyTime = ((Get-Date) - $nexusCompanyStart).TotalSeconds
    Write-Host "    Nexus Company nodes: $([math]::Round($nexusCompanyTime, 2))s" -ForegroundColor Gray

    Write-Host "  Creating 100 WORKS_AT relationships in Nexus (batch)..." -ForegroundColor Yellow
    $nexusRelStart = Get-Date

    # Nexus: Use UNWIND to create all relationships in batches
    for ($batch = 0; $batch -lt 100; $batch += 50) {
        $batchEnd = [Math]::Min($batch + 50, 100)
        $relItems = @()
        for ($i = $batch; $i -lt $batchEnd; $i++) {
            $r = $relData[$i]
            $relItems += "{pname: 'Person$($r.personId)', cname: 'Company$($r.companyId)', since: $($r.since)}"
        }
        $relList = $relItems -join ", "
        $unwindQuery = "UNWIND [$relList] AS r MATCH (p:Person {name: r.pname}), (c:Company {name: r.cname}) CREATE (p)-[:WORKS_AT {since: r.since}]->(c)"
        Execute-NexusQuery -Query $unwindQuery -Iterations 1 | Out-Null
    }
    $nexusRelTime = ((Get-Date) - $nexusRelStart).TotalSeconds
    Write-Host "    Nexus relationships: $([math]::Round($nexusRelTime, 2))s" -ForegroundColor Gray

    # ========== NEO4J: Use batch transaction API ==========
    Write-Host "  Creating 200 Person nodes in Neo4j (batch)..." -ForegroundColor Yellow
    $neo4jPersonStart = Get-Date

    # Neo4j: All 200 persons in 2 batches of 100
    for ($batch = 0; $batch -lt 200; $batch += 100) {
        $batchEnd = [Math]::Min($batch + 100, 200)
        $statements = @()
        for ($i = $batch; $i -lt $batchEnd; $i++) {
            $p = $personData[$i]
            $statements += "CREATE (n:Person {name: '$($p.name)', age: $($p.age), city: '$($p.city)'})"
        }
        Execute-Neo4jBatch -Statements $statements | Out-Null
    }
    $neo4jPersonTime = ((Get-Date) - $neo4jPersonStart).TotalSeconds
    Write-Host "    Neo4j Person nodes: $([math]::Round($neo4jPersonTime, 2))s" -ForegroundColor Gray

    Write-Host "  Creating 20 Company nodes in Neo4j (batch)..." -ForegroundColor Yellow
    $neo4jCompanyStart = Get-Date

    # Neo4j: All 20 companies in one batch
    $statements = @()
    foreach ($c in $companyData) {
        $statements += "CREATE (n:Company {name: '$($c.name)', industry: '$($c.industry)'})"
    }
    Execute-Neo4jBatch -Statements $statements | Out-Null

    $neo4jCompanyTime = ((Get-Date) - $neo4jCompanyStart).TotalSeconds
    Write-Host "    Neo4j Company nodes: $([math]::Round($neo4jCompanyTime, 2))s" -ForegroundColor Gray

    Write-Host "  Creating 100 WORKS_AT relationships in Neo4j (batch)..." -ForegroundColor Yellow
    $neo4jRelStart = Get-Date

    # Neo4j: All 100 relationships in 2 batches of 50
    for ($batch = 0; $batch -lt 100; $batch += 50) {
        $batchEnd = [Math]::Min($batch + 50, 100)
        $statements = @()
        for ($i = $batch; $i -lt $batchEnd; $i++) {
            $r = $relData[$i]
            $statements += "MATCH (p:Person {name: 'Person$($r.personId)'}), (c:Company {name: 'Company$($r.companyId)'}) CREATE (p)-[:WORKS_AT {since: $($r.since)}]->(c)"
        }
        Execute-Neo4jBatch -Statements $statements | Out-Null
    }
    $neo4jRelTime = ((Get-Date) - $neo4jRelStart).TotalSeconds
    Write-Host "    Neo4j relationships: $([math]::Round($neo4jRelTime, 2))s" -ForegroundColor Gray

    Write-Host "`n  Test data created successfully!" -ForegroundColor Green
    Write-Host "  Total Nexus time: $([math]::Round($nexusPersonTime + $nexusCompanyTime + $nexusRelTime, 2))s" -ForegroundColor Cyan
    Write-Host "  Total Neo4j time: $([math]::Round($neo4jPersonTime + $neo4jCompanyTime + $neo4jRelTime, 2))s" -ForegroundColor Cyan
}

# ============================================================================
# BENCHMARK SUITE
# ============================================================================

Write-Host "========================================" -ForegroundColor Cyan
Write-Host "Nexus vs Neo4j Performance Benchmark" -ForegroundColor Cyan
Write-Host "========================================" -ForegroundColor Cyan

# Setup
Setup-TestData

# Category 1: Simple Queries
Write-Host "`n========================================" -ForegroundColor Cyan
Write-Host "Category 1: Simple Queries" -ForegroundColor Cyan
Write-Host "========================================" -ForegroundColor Cyan

Run-Benchmark -Name "Count All Nodes" `
    -Query "MATCH (n) RETURN count(n) AS total" `
    -Iterations 20 `
    -Category "Simple"

Run-Benchmark -Name "Get Single Node" `
    -Query "MATCH (n:Person {name: 'Person100'}) RETURN n" `
    -Iterations 20 `
    -Category "Simple"

Run-Benchmark -Name "Get All Nodes" `
    -Query "MATCH (n:Person) RETURN n LIMIT 100" `
    -Iterations 10 `
    -Category "Simple"

# Category 2: Filtering and WHERE Clauses
Write-Host "`n========================================" -ForegroundColor Cyan
Write-Host "Category 2: Filtering and WHERE Clauses" -ForegroundColor Cyan
Write-Host "========================================" -ForegroundColor Cyan

Run-Benchmark -Name "WHERE Age Filter" `
    -Query "MATCH (n:Person) WHERE n.age > 30 RETURN n.name, n.age LIMIT 50" `
    -Iterations 15 `
    -Category "Filtering"

Run-Benchmark -Name "WHERE City Filter" `
    -Query "MATCH (n:Person) WHERE n.city = 'New York' RETURN n.name LIMIT 50" `
    -Iterations 15 `
    -Category "Filtering"

Run-Benchmark -Name "Complex WHERE" `
    -Query "MATCH (n:Person) WHERE n.age > 25 AND n.age < 50 AND n.city IN ['New York', 'London'] RETURN n.name LIMIT 50" `
    -Iterations 10 `
    -Category "Filtering"

# Category 3: Aggregations
Write-Host "`n========================================" -ForegroundColor Cyan
Write-Host "Category 3: Aggregations" -ForegroundColor Cyan
Write-Host "========================================" -ForegroundColor Cyan

Run-Benchmark -Name "COUNT Aggregation" `
    -Query "MATCH (n:Person) RETURN count(n) AS total" `
    -Iterations 20 `
    -Category "Aggregation"

Run-Benchmark -Name "AVG Aggregation" `
    -Query "MATCH (n:Person) RETURN avg(n.age) AS avg_age" `
    -Iterations 15 `
    -Category "Aggregation"

Run-Benchmark -Name "GROUP BY Aggregation" `
    -Query "MATCH (n:Person) RETURN n.city, count(n) AS count ORDER BY count DESC LIMIT 10" `
    -Iterations 10 `
    -Category "Aggregation"

Run-Benchmark -Name "COLLECT Aggregation" `
    -Query "MATCH (n:Person) RETURN collect(n.name) AS names LIMIT 1" `
    -Iterations 15 `
    -Category "Aggregation"

# Category 4: Relationship Traversal
Write-Host "`n========================================" -ForegroundColor Cyan
Write-Host "Category 4: Relationship Traversal" -ForegroundColor Cyan
Write-Host "========================================" -ForegroundColor Cyan

Run-Benchmark -Name "Single Hop Relationship" `
    -Query "MATCH (p:Person)-[:WORKS_AT]->(c:Company) RETURN p.name, c.name LIMIT 50" `
    -Iterations 15 `
    -Category "Relationships"

Run-Benchmark -Name "Relationship with WHERE" `
    -Query "MATCH (p:Person)-[r:WORKS_AT]->(c:Company) WHERE r.since > 2020 RETURN p.name, c.name LIMIT 50" `
    -Iterations 10 `
    -Category "Relationships"

Run-Benchmark -Name "Count Relationships" `
    -Query "MATCH ()-[r:WORKS_AT]->() RETURN count(r) AS total" `
    -Iterations 20 `
    -Category "Relationships"

# Category 5: Complex Queries
Write-Host "`n========================================" -ForegroundColor Cyan
Write-Host "Category 5: Complex Queries" -ForegroundColor Cyan
Write-Host "========================================" -ForegroundColor Cyan

Run-Benchmark -Name "Multi-Label Match" `
    -Query "MATCH (p:Person), (c:Company) WHERE p.city = 'New York' RETURN count(p) AS persons, count(c) AS companies" `
    -Iterations 10 `
    -Category "Complex"

Run-Benchmark -Name "JOIN-like Query" `
    -Query "MATCH (p:Person)-[:WORKS_AT]->(c:Company) WHERE p.age > 30 RETURN p.name, c.name, c.industry LIMIT 50" `
    -Iterations 10 `
    -Category "Complex"

Run-Benchmark -Name "Nested Aggregation" `
    -Query "MATCH (p:Person)-[:WORKS_AT]->(c:Company) RETURN c.name, count(p) AS employees, avg(p.age) AS avg_age ORDER BY employees DESC LIMIT 10" `
    -Iterations 8 `
    -Category "Complex"

# Category 6: Write Operations
Write-Host "`n========================================" -ForegroundColor Cyan
Write-Host "Category 6: Write Operations" -ForegroundColor Cyan
Write-Host "========================================" -ForegroundColor Cyan

Run-Benchmark -Name "CREATE Single Node" `
    -Query "CREATE (n:TestNode {id: 9999, value: 'test'})" `
    -Iterations 20 `
    -Category "Write"

Run-Benchmark -Name "CREATE with Properties" `
    -Query "CREATE (n:TestNode {id: 9998, name: 'Test', age: 30, active: true})" `
    -Iterations 15 `
    -Category "Write"

Run-Benchmark -Name "CREATE Relationship" `
    -Query "MATCH (p:Person {name: 'Person1'}), (c:Company {name: 'Company1'}) CREATE (p)-[:TEST_REL {created: 2024}]->(c)" `
    -Iterations 15 `
    -Category "Write"

# Category 7: Sorting and Ordering
Write-Host "`n========================================" -ForegroundColor Cyan
Write-Host "Category 7: Sorting and Ordering" -ForegroundColor Cyan
Write-Host "========================================" -ForegroundColor Cyan

Run-Benchmark -Name "ORDER BY Single Column" `
    -Query "MATCH (n:Person) RETURN n.name ORDER BY n.name LIMIT 100" `
    -Iterations 10 `
    -Category "Sorting"

Run-Benchmark -Name "ORDER BY Multiple Columns" `
    -Query "MATCH (n:Person) RETURN n.city, n.age ORDER BY n.city, n.age DESC LIMIT 100" `
    -Iterations 10 `
    -Category "Sorting"

# Category 8: Throughput Test
Write-Host "`n========================================" -ForegroundColor Cyan
Write-Host "Category 8: Throughput Test" -ForegroundColor Cyan
Write-Host "========================================" -ForegroundColor Cyan

Write-Host "`n[Throughput] Running 100 sequential queries..." -ForegroundColor Cyan
$throughputQuery = "MATCH (n:Person) WHERE n.age > 30 RETURN n.name LIMIT 10"

$nexusStart = Get-Date
for ($i = 0; $i -lt 100; $i++) {
    Execute-NexusQuery -Query $throughputQuery -Iterations 1 | Out-Null
}
$nexusEnd = Get-Date
$nexusThroughput = 100 / ($nexusEnd - $nexusStart).TotalSeconds

$neo4jStart = Get-Date
for ($i = 0; $i -lt 100; $i++) {
    Execute-Neo4jQuery -Query $throughputQuery -Iterations 1 | Out-Null
}
$neo4jEnd = Get-Date
$neo4jThroughput = 100 / ($neo4jEnd - $neo4jStart).TotalSeconds

Write-Host "  Nexus:  $([math]::Round($nexusThroughput, 2)) queries/sec" -ForegroundColor $(if ($nexusThroughput -gt $neo4jThroughput) { "Green" } else { "White" })
Write-Host "  Neo4j:  $([math]::Round($neo4jThroughput, 2)) queries/sec" -ForegroundColor $(if ($neo4jThroughput -gt $nexusThroughput) { "Green" } else { "White" })

$global:BenchmarkResults += @{
    Name = "Throughput (100 queries)"
    Category = "Throughput"
    Query = $throughputQuery
    NexusAvgTime = 1000 / $nexusThroughput
    Neo4jAvgTime = 1000 / $neo4jThroughput
    NexusSuccess = 100
    Neo4jSuccess = 100
    Winner = if ($nexusThroughput -gt $neo4jThroughput) { "Nexus" } else { "Neo4j" }
    Improvement = [math]::Abs((($nexusThroughput - $neo4jThroughput) / [math]::Max($nexusThroughput, $neo4jThroughput)) * 100)
    Iterations = 100
}

# ============================================================================
# SUMMARY REPORT
# ============================================================================

Write-Host "`n========================================" -ForegroundColor Cyan
Write-Host "SUMMARY REPORT" -ForegroundColor Cyan
Write-Host "========================================" -ForegroundColor Cyan

# Calculate statistics
$nexusWins = ($global:BenchmarkResults | Where-Object { $_.Winner -eq "Nexus" }).Count
$neo4jWins = ($global:BenchmarkResults | Where-Object { $_.Winner -eq "Neo4j" }).Count
$ties = ($global:BenchmarkResults | Where-Object { $_.Winner -eq "Tie" }).Count

$nexusResults = $global:BenchmarkResults | Where-Object { $_.NexusAvgTime -gt 0 }
$neo4jResults = $global:BenchmarkResults | Where-Object { $_.Neo4jAvgTime -gt 0 }
$nexusAvgTime = if ($nexusResults.Count -gt 0) { ($nexusResults | Measure-Object -Property NexusAvgTime -Average).Average } else { 0 }
$neo4jAvgTime = if ($neo4jResults.Count -gt 0) { ($neo4jResults | Measure-Object -Property Neo4jAvgTime -Average).Average } else { 0 }

Write-Host "`nOverall Statistics:" -ForegroundColor Yellow
Write-Host "  Total Benchmarks: $($global:BenchmarkResults.Count)" -ForegroundColor White
Write-Host "  Nexus Wins: $nexusWins" -ForegroundColor $(if ($nexusWins -gt $neo4jWins) { "Green" } else { "White" })
Write-Host "  Neo4j Wins: $neo4jWins" -ForegroundColor $(if ($neo4jWins -gt $nexusWins) { "Green" } else { "White" })
Write-Host "  Ties: $ties" -ForegroundColor Gray
Write-Host "  Average Latency - Nexus: $([math]::Round($nexusAvgTime, 2))ms" -ForegroundColor White
Write-Host "  Average Latency - Neo4j: $([math]::Round($neo4jAvgTime, 2))ms" -ForegroundColor White

# Category breakdown
Write-Host "`nCategory Breakdown:" -ForegroundColor Yellow
$categories = $global:BenchmarkResults | Group-Object Category
foreach ($cat in $categories) {
    $catNexusWins = ($cat.Group | Where-Object { $_.Winner -eq "Nexus" }).Count
    $catNeo4jWins = ($cat.Group | Where-Object { $_.Winner -eq "Neo4j" }).Count
    Write-Host "  $($cat.Name): Nexus $catNexusWins - Neo4j $catNeo4jWins" -ForegroundColor White
}

# Top 5 fastest improvements
Write-Host "`nTop 5 Nexus Performance Wins:" -ForegroundColor Yellow
$nexusWinsList = $global:BenchmarkResults | Where-Object { $_.Winner -eq "Nexus" } | Sort-Object Improvement -Descending | Select-Object -First 5
foreach ($win in $nexusWinsList) {
    Write-Host "  $($win.Name): $([math]::Round($win.Improvement, 1))% faster" -ForegroundColor Green
}

Write-Host "`nTop 5 Neo4j Performance Wins:" -ForegroundColor Yellow
$neo4jWinsList = $global:BenchmarkResults | Where-Object { $_.Winner -eq "Neo4j" } | Sort-Object Improvement -Descending | Select-Object -First 5
foreach ($win in $neo4jWinsList) {
    Write-Host "  $($win.Name): $([math]::Round($win.Improvement, 1))% faster" -ForegroundColor Yellow
}

# Export detailed results
$timestamp = Get-Date -Format "yyyy-MM-dd_HH-mm-ss"
$resultsFile = "benchmark-results-$timestamp.json"
$global:BenchmarkResults | ConvertTo-Json -Depth 10 | Out-File $resultsFile
Write-Host "`nDetailed results saved to: $resultsFile" -ForegroundColor Green

Write-Host "`nBenchmark completed!" -ForegroundColor Green

