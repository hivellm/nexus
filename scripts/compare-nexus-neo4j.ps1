#!/usr/bin/env pwsh
# Compare Nexus queries with Neo4j queries
# Test if Nexus returns similar results to Neo4j

$nexusUrl = "http://127.0.0.1:15474"
$neo4jUrl = "http://127.0.0.1:7474"
$neo4jUser = "neo4j"
$neo4jPass = "password"

function Execute-NexusQuery {
    param([string]$Query, [string]$Name)
    
    Write-Host "`n[NEXUS] $Name" -ForegroundColor Cyan
    Write-Host "  Query: $Query" -ForegroundColor DarkGray
    
    try {
        $body = @{ query = $Query } | ConvertTo-Json
        $response = Invoke-RestMethod -Uri "$nexusUrl/cypher" -Method POST -Body $body -ContentType "application/json" -ErrorAction Stop
        
        Write-Host "  ✅ Status: Success" -ForegroundColor Green
        Write-Host "  ⏱️  Time: $($response.execution_time_ms)ms" -ForegroundColor Gray
        if ($response.rows) {
            Write-Host "  📊 Rows: $($response.rows.Count)" -ForegroundColor White
            # Show first few rows
            $response.rows | Select-Object -First 3 | ForEach-Object {
                $rowJson = $_ | ConvertTo-Json -Compress -Depth 5
                if ($rowJson.Length -gt 100) {
                    $rowJson = $rowJson.Substring(0, 100) + "..."
                }
                Write-Host "    - $rowJson" -ForegroundColor DarkGray
            }
        }
        return $response
    } catch {
        Write-Host "  ❌ Error: $_" -ForegroundColor Red
        return $null
    }
}

function Execute-Neo4jQuery {
    param([string]$Query, [string]$Name)
    
    Write-Host "`n[NEO4J] $Name" -ForegroundColor Yellow
    Write-Host "  Query: $Query" -ForegroundColor DarkGray
    
    try {
        $auth = [Convert]::ToBase64String([Text.Encoding]::ASCII.GetBytes("${neo4jUser}:${neo4jPass}"))
        $body = @{
            statements = @(@{ statement = $Query })
        } | ConvertTo-Json
        
        $response = Invoke-RestMethod -Uri "$neo4jUrl/db/neo4j/tx/commit" `
            -Method POST `
            -Headers @{
                "Authorization" = "Basic $auth"
                "Content-Type" = "application/json"
            } `
            -Body $body `
            -ErrorAction Stop
        
        if ($response.errors -and $response.errors.Count -gt 0) {
            Write-Host "  ❌ Error: $($response.errors[0].message)" -ForegroundColor Red
            return $null
        }
        
        Write-Host "  ✅ Status: Success" -ForegroundColor Green
        if ($response.results -and $response.results.Count -gt 0) {
            $data = $response.results[0].data
            $rowCount = $data.Count
            Write-Host "  📊 Rows: $rowCount" -ForegroundColor White
            # Show first few rows
            $data | Select-Object -First 3 | ForEach-Object {
                $row = $_.row
                $rowJson = $row | ConvertTo-Json -Compress -Depth 5
                if ($rowJson.Length -gt 100) {
                    $rowJson = $rowJson.Substring(0, 100) + "..."
                }
                Write-Host "    - $rowJson" -ForegroundColor DarkGray
            }
        }
        return $response
    } catch {
        Write-Host "  ⚠️  Neo4j not available or query failed: $_" -ForegroundColor Yellow
        return $null
    }
}

Write-Host "╔═══════════════════════════════════════════════════╗" -ForegroundColor Cyan
Write-Host "║  Nexus vs Neo4j Query Comparison                 ║" -ForegroundColor Cyan
Write-Host "╚═══════════════════════════════════════════════════╝" -ForegroundColor Cyan

# Check connections
Write-Host "`n🔍 Verificando conexões...`n" -ForegroundColor Cyan
try {
    $nexusHealth = Invoke-RestMethod -Uri "$nexusUrl/health" -ErrorAction Stop
    Write-Host "  ✅ Nexus: Online" -ForegroundColor Green
} catch {
    Write-Host "  ❌ Nexus: Offline" -ForegroundColor Red
    exit 1
}

try {
    $neo4jAuth = [Convert]::ToBase64String([Text.Encoding]::ASCII.GetBytes("${neo4jUser}:${neo4jPass}"))
    $neo4jCheck = Invoke-RestMethod -Uri "$neo4jUrl/db/neo4j/tx/commit" `
        -Method POST `
        -Headers @{
            "Authorization" = "Basic $neo4jAuth"
            "Content-Type" = "application/json"
        } `
        -Body (@{ statements = @(@{ statement = "RETURN 1" }) } | ConvertTo-Json) `
        -ErrorAction Stop
    Write-Host "  ✅ Neo4j: Online" -ForegroundColor Green
} catch {
    Write-Host "  ⚠️  Neo4j: Não disponível (continuando apenas com Nexus)" -ForegroundColor Yellow
}

# Test queries
$queries = @(
    @{ name = "Count all documents"; query = "MATCH (d:Document) RETURN count(d) AS total" },
    @{ name = "Count all modules"; query = "MATCH (m:Module) RETURN count(m) AS total" },
    @{ name = "Count all classes"; query = "MATCH (c:Class) RETURN count(c) AS total" },
    @{ name = "Count all functions"; query = "MATCH (f:Function) RETURN count(f) AS total" },
    @{ name = "Count relationships MENTIONS"; query = "MATCH ()-[r:MENTIONS]->() RETURN count(r) AS total" },
    @{ name = "Documents by domain"; query = "MATCH (d:Document) RETURN d.domain AS domain, count(d) AS count ORDER BY count DESC LIMIT 10" },
    @{ name = "Sample documents"; query = "MATCH (d:Document) RETURN d.title AS title, d.domain AS domain LIMIT 5" },
    @{ name = "Sample modules"; query = "MATCH (m:Module) RETURN m.name AS name LIMIT 5" },
    @{ name = "Documents mentioning PostgreSQL"; query = "MATCH (d:Document)-[:MENTIONS]->(e) WHERE e.name = 'PostgreSQL' OR e.name = 'pg' RETURN d.title, e.name LIMIT 5" }
)

$comparisonResults = @()

foreach ($q in $queries) {
    Write-Host "`n━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━" -ForegroundColor DarkGray
    Write-Host "[TEST] $($q.name)" -ForegroundColor Magenta
    
    $nexusResult = Execute-NexusQuery -Query $q.query -Name $q.name
    $neo4jResult = Execute-Neo4jQuery -Query $q.query -Name $q.name
    
    if ($nexusResult -and $neo4jResult) {
        $nexusRows = if ($nexusResult.rows) { $nexusResult.rows.Count } else { 0 }
        $neo4jRows = if ($neo4jResult.results -and $neo4jResult.results[0].data) { 
            $neo4jResult.results[0].data.Count 
        } else { 0 }
        
        $match = $nexusRows -eq $neo4jRows
        $status = if ($match) { "✅ MATCH" } else { "⚠️  DIFF" }
        $color = if ($match) { "Green" } else { "Yellow" }
        
        Write-Host "  $status Nexus=$nexusRows rows, Neo4j=$neo4jRows rows" -ForegroundColor $color
        
        $comparisonResults += @{
            Test = $q.name
            NexusRows = $nexusRows
            Neo4jRows = $neo4jRows
            Match = $match
        }
    } elseif ($nexusResult) {
        $nexusRows = if ($nexusResult.rows) { $nexusResult.rows.Count } else { 0 }
        Write-Host "  ℹ️  Nexus only: $nexusRows rows" -ForegroundColor Cyan
        $comparisonResults += @{
            Test = $q.name
            NexusRows = $nexusRows
            Neo4jRows = 0
            Match = $false
        }
    }
}

# Summary
Write-Host "`n━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━" -ForegroundColor DarkGray
Write-Host "`n╔═══════════════════════════════════════════════════╗" -ForegroundColor Cyan
Write-Host "║  Comparison Summary                              ║" -ForegroundColor Cyan
Write-Host "╚═══════════════════════════════════════════════════╝" -ForegroundColor Cyan

$totalTests = $comparisonResults.Count
$matchingTests = ($comparisonResults | Where-Object { $_.Match }).Count
$matchPercentage = if ($totalTests -gt 0) { [math]::Round(($matchingTests / $totalTests) * 100, 1) } else { 0 }

Write-Host "`nTotal Tests: $totalTests" -ForegroundColor White
Write-Host "Matching: $matchingTests ✅" -ForegroundColor Green
Write-Host "Different: $($totalTests - $matchingTests) ⚠️" -ForegroundColor Yellow
Write-Host "Match Rate: $matchPercentage%`n" -ForegroundColor $(if ($matchPercentage -ge 80) { "Green" } else { "Yellow" })

Write-Host "Detailed Results:" -ForegroundColor Cyan
$comparisonResults | ForEach-Object {
    $status = if ($_.Match) { "✅" } else { "⚠️ " }
    $color = if ($_.Match) { "Green" } else { "Yellow" }
    Write-Host "  $status $($_.Test)" -ForegroundColor $color
    Write-Host "     Nexus: $($_.NexusRows) rows | Neo4j: $($_.Neo4jRows) rows" -ForegroundColor DarkGray
}

Write-Host "`n✨ Comparison complete!`n" -ForegroundColor Cyan
