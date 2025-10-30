# Manual Test Script - Run this AFTER starting the server manually
# Instructions:
# 1. Open a separate terminal
# 2. Run: wsl -d Ubuntu-24.04 -- bash -l -c "cd /mnt/f/Node/hivellm/nexus && ./target/release/nexus-server"
# 3. Then run this script in PowerShell

$baseUrl = "http://localhost:15474"

Write-Host "=== Manual Relationship Query Tests ===" -ForegroundColor Cyan
Write-Host ""

# Check if server is running
Write-Host "Checking if server is running..." -ForegroundColor Yellow
try {
    $stats = Invoke-RestMethod -Uri "$baseUrl/stats" -Method Get -TimeoutSec 3
    Write-Host "✅ Server is running!" -ForegroundColor Green
    Write-Host "  Stats: $($stats.catalog.node_count) nodes, $($stats.catalog.rel_count) rels" -ForegroundColor Gray
} catch {
    Write-Host "❌ Server is not responding!" -ForegroundColor Red
    Write-Host "Please start the server first:" -ForegroundColor Yellow
    Write-Host '  wsl -d Ubuntu-24.04 -- bash -l -c "cd /mnt/f/Node/hivellm/nexus && ./target/release/nexus-server"' -ForegroundColor Cyan
    exit 1
}
Write-Host ""

function Test-Query {
    param([string]$name, [string]$query)
    
    Write-Host "$name" -ForegroundColor Yellow
    Write-Host "  Query: $query" -ForegroundColor Gray
    
    $body = @{ query = $query } | ConvertTo-Json
    
    try {
        $response = Invoke-RestMethod -Uri "$baseUrl/cypher" -Method Post -Body $body -ContentType "application/json" -TimeoutSec 10
        
        if ($response.rows -and $response.rows.Count -gt 0) {
            Write-Host "  ✅ Success! Rows returned: $($response.rows.Count)" -ForegroundColor Green
            
            # Show first few results
            $maxShow = [Math]::Min(5, $response.rows.Count)
            for ($i = 0; $i -lt $maxShow; $i++) {
                $row = $response.rows[$i]
                Write-Host "    [$i] $($row -join ' | ')" -ForegroundColor Cyan
            }
            
            return $true
        } else {
            Write-Host "  ⚠️ Query executed but returned 0 rows" -ForegroundColor Yellow
            return $false
        }
    } catch {
        Write-Host "  ❌ Error: $($_.Exception.Message)" -ForegroundColor Red
        return $false
    }
}

Write-Host "Running tests..." -ForegroundColor Cyan
Write-Host ""

$results = @{}

# Test 1
$results['nodes'] = Test-Query "Test 1: Count all nodes" "MATCH (n) RETURN count(n) AS total"
Write-Host ""
Start-Sleep -Milliseconds 500

# Test 2
$results['rels'] = Test-Query "Test 2: Count all relationships" "MATCH ()-[r]->() RETURN count(r) AS total"
Write-Host ""
Start-Sleep -Milliseconds 500

# Test 3
$results['rel_types'] = Test-Query "Test 3: Get relationship types" "MATCH ()-[r]->() RETURN DISTINCT type(r) AS relType LIMIT 5"
Write-Host ""
Start-Sleep -Milliseconds 500

# Test 4
$results['mentions'] = Test-Query "Test 4: Count MENTIONS" "MATCH ()-[r:MENTIONS]->() RETURN count(r) AS total"
Write-Host ""
Start-Sleep -Milliseconds 500

# Test 5
$results['sample'] = Test-Query "Test 5: Sample relationships" "MATCH (a)-[r]->(b) RETURN id(a), type(r), id(b) LIMIT 3"
Write-Host ""

# Summary
Write-Host "=== Summary ===" -ForegroundColor Cyan
$passed = ($results.Values | Where-Object { $_ -eq $true }).Count
$total = $results.Count
Write-Host "Passed: $passed / $total tests" -ForegroundColor $(if ($passed -eq $total) { "Green" } else { "Yellow" })
Write-Host ""

if ($results['rels'] -eq $false) {
    Write-Host "⚠️ CRITICAL: Relationship queries returning 0 rows!" -ForegroundColor Red
    Write-Host "   This indicates the Expand operator may not be working correctly." -ForegroundColor Red
}

