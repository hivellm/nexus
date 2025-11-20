# Concurrent Execution Test: Nexus vs Neo4j
# Tests true parallel query execution (not sequential)
# 
# Usage: ./test-concurrent-execution.ps1

$ErrorActionPreference = "Continue"

# Configuration
$NexusUrl = "http://localhost:15474"
$Neo4jUrl = "http://localhost:7474"
$Neo4jUser = "neo4j"
$Neo4jPassword = "password"

Write-Host "========================================" -ForegroundColor Cyan
Write-Host "Concurrent Execution Test" -ForegroundColor Cyan
Write-Host "========================================" -ForegroundColor Cyan
Write-Host ""

# Test query (simple read query)
$testQuery = "MATCH (n:Person) WHERE n.age > 30 RETURN n.name LIMIT 10"

# Number of concurrent queries
$concurrentQueries = 20

Write-Host "[INFO] Testing with $concurrentQueries concurrent queries" -ForegroundColor Yellow
Write-Host "[INFO] Query: $testQuery" -ForegroundColor Yellow
Write-Host ""

# ============================================================================
# NEXUS: Sequential Execution (baseline)
# ============================================================================
Write-Host "[Nexus] Running $concurrentQueries queries SEQUENTIALLY..." -ForegroundColor Cyan
$nexusSequentialStart = Get-Date

for ($i = 0; $i -lt $concurrentQueries; $i++) {
    try {
        $response = Invoke-RestMethod -Uri "$NexusUrl/cypher" `
            -Method POST `
            -ContentType "application/json" `
            -Body (@{ query = $testQuery } | ConvertTo-Json) `
            -TimeoutSec 30 `
            -ErrorAction Stop | Out-Null
    }
    catch {
        Write-Host "  [ERROR] Query $i failed: $_" -ForegroundColor Red
    }
}

$nexusSequentialEnd = Get-Date
$nexusSequentialTime = ($nexusSequentialEnd - $nexusSequentialStart).TotalMilliseconds
$nexusSequentialAvg = $nexusSequentialTime / $concurrentQueries

Write-Host "  Sequential Time: $([math]::Round($nexusSequentialTime, 2)) ms" -ForegroundColor White
Write-Host "  Average per query: $([math]::Round($nexusSequentialAvg, 2)) ms" -ForegroundColor White
Write-Host ""

# ============================================================================
# NEXUS: Concurrent Execution (using Jobs)
# ============================================================================
Write-Host "[Nexus] Running $concurrentQueries queries CONCURRENTLY (parallel)..." -ForegroundColor Cyan
$nexusConcurrentStart = Get-Date

$nexusJobs = @()
for ($i = 0; $i -lt $concurrentQueries; $i++) {
    $job = Start-Job -ScriptBlock {
        param($url, $query)
        try {
            $response = Invoke-RestMethod -Uri "$url/cypher" `
                -Method POST `
                -ContentType "application/json" `
                -Body (@{ query = $query } | ConvertTo-Json) `
                -TimeoutSec 30 `
                -ErrorAction Stop
            return @{ Success = $true; Error = $null }
        }
        catch {
            return @{ Success = $false; Error = $_.Exception.Message }
        }
    } -ArgumentList $NexusUrl, $testQuery
    $nexusJobs += $job
}

# Wait for all jobs to complete
$nexusResults = $nexusJobs | Wait-Job | Receive-Job
$nexusJobs | Remove-Job

$nexusConcurrentEnd = Get-Date
$nexusConcurrentTime = ($nexusConcurrentEnd - $nexusConcurrentStart).TotalMilliseconds
$nexusConcurrentAvg = $nexusConcurrentTime / $concurrentQueries
$nexusSuccessCount = ($nexusResults | Where-Object { $_.Success -eq $true }).Count
$nexusErrorCount = ($nexusResults | Where-Object { $_.Success -eq $false }).Count

Write-Host "  Concurrent Time: $([math]::Round($nexusConcurrentTime, 2)) ms" -ForegroundColor White
Write-Host "  Average per query: $([math]::Round($nexusConcurrentAvg, 2)) ms" -ForegroundColor White
Write-Host "  Success: $nexusSuccessCount / $concurrentQueries" -ForegroundColor $(if ($nexusErrorCount -eq 0) { "Green" } else { "Yellow" })
if ($nexusErrorCount -gt 0) {
    Write-Host "  Errors: $nexusErrorCount" -ForegroundColor Red
}
Write-Host ""

# Calculate speedup
$nexusSpeedup = if ($nexusConcurrentTime -gt 0) { $nexusSequentialTime / $nexusConcurrentTime } else { 0 }
$nexusEfficiency = if ($nexusConcurrentTime -gt 0) { ($nexusSequentialTime / $nexusConcurrentTime) / $concurrentQueries * 100 } else { 0 }

Write-Host "  [Nexus Results]" -ForegroundColor Green
Write-Host "    Speedup: $([math]::Round($nexusSpeedup, 2))x (ideal: $concurrentQueries x)" -ForegroundColor $(if ($nexusSpeedup -gt 5) { "Green" } else { "Yellow" })
Write-Host "    Efficiency: $([math]::Round($nexusEfficiency, 1))% (ideal: 100%)" -ForegroundColor $(if ($nexusEfficiency -gt 50) { "Green" } else { "Yellow" })
Write-Host ""

# ============================================================================
# NEO4J: Sequential Execution (baseline)
# ============================================================================
Write-Host "[Neo4j] Running $concurrentQueries queries SEQUENTIALLY..." -ForegroundColor Cyan
$neo4jSequentialStart = Get-Date

$headers = @{
    "Authorization" = "Basic " + [Convert]::ToBase64String([Text.Encoding]::ASCII.GetBytes("${Neo4jUser}:${Neo4jPassword}"))
    "Content-Type" = "application/json"
}

for ($i = 0; $i -lt $concurrentQueries; $i++) {
    try {
        $body = @{
            statements = @(
                @{
                    statement = $testQuery
                }
            )
        } | ConvertTo-Json
        
        $response = Invoke-RestMethod -Uri "$Neo4jUrl/db/neo4j/tx/commit" `
            -Method POST `
            -Headers $headers `
            -Body $body `
            -TimeoutSec 30 `
            -ErrorAction Stop | Out-Null
    }
    catch {
        Write-Host "  [ERROR] Query $i failed: $_" -ForegroundColor Red
    }
}

$neo4jSequentialEnd = Get-Date
$neo4jSequentialTime = ($neo4jSequentialEnd - $neo4jSequentialStart).TotalMilliseconds
$neo4jSequentialAvg = $neo4jSequentialTime / $concurrentQueries

Write-Host "  Sequential Time: $([math]::Round($neo4jSequentialTime, 2)) ms" -ForegroundColor White
Write-Host "  Average per query: $([math]::Round($neo4jSequentialAvg, 2)) ms" -ForegroundColor White
Write-Host ""

# ============================================================================
# NEO4J: Concurrent Execution (using Jobs)
# ============================================================================
Write-Host "[Neo4j] Running $concurrentQueries queries CONCURRENTLY (parallel)..." -ForegroundColor Cyan
$neo4jConcurrentStart = Get-Date

$neo4jJobs = @()
for ($i = 0; $i -lt $concurrentQueries; $i++) {
    $job = Start-Job -ScriptBlock {
        param($url, $user, $password, $query)
        try {
            $headers = @{
                "Authorization" = "Basic " + [Convert]::ToBase64String([Text.Encoding]::ASCII.GetBytes("${user}:${password}"))
                "Content-Type" = "application/json"
            }
            $body = @{
                statements = @(
                    @{
                        statement = $query
                    }
                )
            } | ConvertTo-Json
            
            $response = Invoke-RestMethod -Uri "$url/db/neo4j/tx/commit" `
                -Method POST `
                -Headers $headers `
                -Body $body `
                -TimeoutSec 30 `
                -ErrorAction Stop
            return @{ Success = $true; Error = $null }
        }
        catch {
            return @{ Success = $false; Error = $_.Exception.Message }
        }
    } -ArgumentList $Neo4jUrl, $Neo4jUser, $Neo4jPassword, $testQuery
    $neo4jJobs += $job
}

# Wait for all jobs to complete
$neo4jResults = $neo4jJobs | Wait-Job | Receive-Job
$neo4jJobs | Remove-Job

$neo4jConcurrentEnd = Get-Date
$neo4jConcurrentTime = ($neo4jConcurrentEnd - $neo4jConcurrentStart).TotalMilliseconds
$neo4jConcurrentAvg = $neo4jConcurrentTime / $concurrentQueries
$neo4jSuccessCount = ($neo4jResults | Where-Object { $_.Success -eq $true }).Count
$neo4jErrorCount = ($neo4jResults | Where-Object { $_.Success -eq $false }).Count

Write-Host "  Concurrent Time: $([math]::Round($neo4jConcurrentTime, 2)) ms" -ForegroundColor White
Write-Host "  Average per query: $([math]::Round($neo4jConcurrentAvg, 2)) ms" -ForegroundColor White
Write-Host "  Success: $neo4jSuccessCount / $concurrentQueries" -ForegroundColor $(if ($neo4jErrorCount -eq 0) { "Green" } else { "Yellow" })
if ($neo4jErrorCount -gt 0) {
    Write-Host "  Errors: $neo4jErrorCount" -ForegroundColor Red
}
Write-Host ""

# Calculate speedup
$neo4jSpeedup = if ($neo4jConcurrentTime -gt 0) { $neo4jSequentialTime / $neo4jConcurrentTime } else { 0 }
$neo4jEfficiency = if ($neo4jConcurrentTime -gt 0) { ($neo4jSequentialTime / $neo4jConcurrentTime) / $concurrentQueries * 100 } else { 0 }

Write-Host "  [Neo4j Results]" -ForegroundColor Green
Write-Host "    Speedup: $([math]::Round($neo4jSpeedup, 2))x (ideal: $concurrentQueries x)" -ForegroundColor $(if ($neo4jSpeedup -gt 5) { "Green" } else { "Yellow" })
Write-Host "    Efficiency: $([math]::Round($neo4jEfficiency, 1))% (ideal: 100%)" -ForegroundColor $(if ($neo4jEfficiency -gt 50) { "Green" } else { "Yellow" })
Write-Host ""

# ============================================================================
# SUMMARY
# ============================================================================
Write-Host "========================================" -ForegroundColor Cyan
Write-Host "Summary" -ForegroundColor Cyan
Write-Host "========================================" -ForegroundColor Cyan
Write-Host ""

Write-Host "Nexus Concurrent Performance:" -ForegroundColor Yellow
Write-Host "  Sequential: $([math]::Round($nexusSequentialTime, 2)) ms ($([math]::Round($nexusSequentialAvg, 2)) ms/query)" -ForegroundColor White
Write-Host "  Concurrent: $([math]::Round($nexusConcurrentTime, 2)) ms ($([math]::Round($nexusConcurrentAvg, 2)) ms/query)" -ForegroundColor White
Write-Host "  Speedup: $([math]::Round($nexusSpeedup, 2))x" -ForegroundColor $(if ($nexusSpeedup -gt 5) { "Green" } else { "Yellow" })
Write-Host "  Efficiency: $([math]::Round($nexusEfficiency, 1))%" -ForegroundColor $(if ($nexusEfficiency -gt 50) { "Green" } else { "Yellow" })
Write-Host ""

Write-Host "Neo4j Concurrent Performance:" -ForegroundColor Yellow
Write-Host "  Sequential: $([math]::Round($neo4jSequentialTime, 2)) ms ($([math]::Round($neo4jSequentialAvg, 2)) ms/query)" -ForegroundColor White
Write-Host "  Concurrent: $([math]::Round($neo4jConcurrentTime, 2)) ms ($([math]::Round($neo4jConcurrentAvg, 2)) ms/query)" -ForegroundColor White
Write-Host "  Speedup: $([math]::Round($neo4jSpeedup, 2))x" -ForegroundColor $(if ($neo4jSpeedup -gt 5) { "Green" } else { "Yellow" })
Write-Host "  Efficiency: $([math]::Round($neo4jEfficiency, 1))%" -ForegroundColor $(if ($neo4jEfficiency -gt 50) { "Green" } else { "Yellow" })
Write-Host ""

Write-Host "Comparison:" -ForegroundColor Yellow
$nexusVsNeo4jConcurrent = if ($neo4jConcurrentTime -gt 0) { ($neo4jConcurrentTime / $nexusConcurrentTime) } else { 0 }
Write-Host "  Nexus concurrent time vs Neo4j: $([math]::Round($nexusVsNeo4jConcurrent, 2))x" -ForegroundColor $(if ($nexusVsNeo4jConcurrent -lt 1) { "Green" } else { "White" })
Write-Host ""

# Interpretation
Write-Host "Interpretation:" -ForegroundColor Cyan
if ($nexusSpeedup -gt 10) {
    Write-Host "  ✅ EXCELLENT: Nexus shows strong parallelization ($([math]::Round($nexusSpeedup, 1))x speedup)" -ForegroundColor Green
}
elseif ($nexusSpeedup -gt 5) {
    Write-Host "  ✅ GOOD: Nexus shows good parallelization ($([math]::Round($nexusSpeedup, 1))x speedup)" -ForegroundColor Green
}
elseif ($nexusSpeedup -gt 2) {
    Write-Host "  ⚠️  MODERATE: Nexus shows some parallelization ($([math]::Round($nexusSpeedup, 1))x speedup)" -ForegroundColor Yellow
}
else {
    Write-Host "  ❌ POOR: Nexus queries are still serialized ($([math]::Round($nexusSpeedup, 2))x speedup)" -ForegroundColor Red
    Write-Host "     Expected: ~$concurrentQueries x speedup for true parallelism" -ForegroundColor Red
}

if ($nexusEfficiency -gt 80) {
    Write-Host "  ✅ EXCELLENT: High efficiency ($([math]::Round($nexusEfficiency, 1))%)" -ForegroundColor Green
}
elseif ($nexusEfficiency -gt 50) {
    Write-Host "  ✅ GOOD: Reasonable efficiency ($([math]::Round($nexusEfficiency, 1))%)" -ForegroundColor Green
}
else {
    Write-Host "  ⚠️  LOW: Efficiency could be improved ($([math]::Round($nexusEfficiency, 1))%)" -ForegroundColor Yellow
}

Write-Host ""

