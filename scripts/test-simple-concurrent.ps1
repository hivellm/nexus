# Simple Concurrent Test: Test basic parallelism
# Tests if queries actually run in parallel
#
# Usage: ./test-simple-concurrent.ps1

$ErrorActionPreference = "Continue"

# Configuration
$NexusUrl = "http://localhost:15474"

Write-Host "========================================" -ForegroundColor Cyan
Write-Host "Simple Concurrent Test" -ForegroundColor Cyan
Write-Host "========================================" -ForegroundColor Cyan
Write-Host ""

# Test query (very simple)
$testQuery = "RETURN 42"

# Number of concurrent queries (start small)
$concurrentQueries = 5

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
# NEXUS: Concurrent Execution
# ============================================================================
Write-Host "[Nexus] Running $concurrentQueries queries CONCURRENTLY..." -ForegroundColor Cyan
$nexusConcurrentStart = Get-Date

$nexusJobs = @()
for ($i = 0; $i -lt $concurrentQueries; $i++) {
    $job = Start-Job -ScriptBlock {
        param($url, $query, $jobId)
        try {
            $startTime = Get-Date
            $response = Invoke-RestMethod -Uri "$url/cypher" `
                -Method POST `
                -ContentType "application/json" `
                -Body (@{ query = $query } | ConvertTo-Json) `
                -TimeoutSec 30 `
                -ErrorAction Stop
            $endTime = Get-Date
            $duration = ($endTime - $startTime).TotalMilliseconds
            return @{ Success = $true; Duration = $duration; JobId = $jobId; Error = $null }
        }
        catch {
            return @{ Success = $false; Duration = 0; JobId = $jobId; Error = $_.Exception.Message }
        }
    } -ArgumentList $NexusUrl, $testQuery, $i
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

# Calculate speedup
$nexusSpeedup = if ($nexusConcurrentTime -gt 0) { $nexusSequentialTime / $nexusConcurrentTime } else { 0 }
$nexusEfficiency = if ($nexusConcurrentTime -gt 0) { ($nexusSequentialTime / $nexusConcurrentTime) / $concurrentQueries * 100 } else { 0 }

Write-Host "  [Nexus Results]" -ForegroundColor Green
Write-Host "    Speedup: $([math]::Round($nexusSpeedup, 2))x (ideal: $concurrentQueries x)" -ForegroundColor $(if ($nexusSpeedup -gt 2) { "Green" } else { "Yellow" })
Write-Host "    Efficiency: $([math]::Round($nexusEfficiency, 1))% (ideal: 100%)" -ForegroundColor $(if ($nexusEfficiency -gt 50) { "Green" } else { "Yellow" })

# Show individual query times
$nexusResults | Sort-Object -Property Duration -Descending | ForEach-Object {
    $color = if ($_.Success) { "White" } else { "Red" }
    Write-Host "    Query $($_.JobId): $([math]::Round($_.Duration, 2)) ms" -ForegroundColor $color
}
Write-Host ""

# ============================================================================
# ANALYSIS
# ============================================================================
Write-Host "========================================" -ForegroundColor Cyan
Write-Host "Analysis" -ForegroundColor Cyan
Write-Host "========================================" -ForegroundColor Cyan

if ($nexusSpeedup -gt ($concurrentQueries * 0.8)) {
    Write-Host "  ✅ EXCELLENT: Strong parallelization ($([math]::Round($nexusSpeedup, 1))x speedup)" -ForegroundColor Green
}
elseif ($nexusSpeedup -gt ($concurrentQueries * 0.5)) {
    Write-Host "  ✅ GOOD: Good parallelization ($([math]::Round($nexusSpeedup, 1))x speedup)" -ForegroundColor Green
}
elseif ($nexusSpeedup -gt ($concurrentQueries * 0.3)) {
    Write-Host "  ⚠️  MODERATE: Some parallelization ($([math]::Round($nexusSpeedup, 1))x speedup)" -ForegroundColor Yellow
}
else {
    Write-Host "  ❌ POOR: Queries are serialized ($([math]::Round($nexusSpeedup, 2))x speedup)" -ForegroundColor Red
    Write-Host "     Expected: ~$concurrentQueries x speedup for true parallelism" -ForegroundColor Red
}

Write-Host ""

