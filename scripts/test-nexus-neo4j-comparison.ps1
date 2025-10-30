$nexusUrl = "http://127.0.0.1:15474"
$neo4jUrl = "http://127.0.0.1:7474"
$neo4jUser = "neo4j"
$neo4jPass = "password"

Write-Host "`n=== Nexus vs Neo4j Comparison ===" -ForegroundColor Cyan
Write-Host "`nChecking connections...`n" -ForegroundColor Yellow

# Check Nexus
try {
    Invoke-RestMethod -Uri "$nexusUrl/health" -ErrorAction Stop | Out-Null
    Write-Host "  [OK] Nexus: Online" -ForegroundColor Green
} catch {
    Write-Host "  [ERROR] Nexus: Offline" -ForegroundColor Red
    exit 1
}

# Check Neo4j
try {
    $auth = [Convert]::ToBase64String([Text.Encoding]::ASCII.GetBytes("${neo4jUser}:${neo4jPass}"))
    $testBody = @{ statements = @(@{ statement = "RETURN 1" }) } | ConvertTo-Json
    Invoke-RestMethod -Uri "$neo4jUrl/db/neo4j/tx/commit" -Method POST -Headers @{ "Authorization" = "Basic $auth"; "Content-Type" = "application/json" } -Body $testBody -ErrorAction Stop | Out-Null
    Write-Host "  [OK] Neo4j: Online`n" -ForegroundColor Green
} catch {
    Write-Host "  [WARN] Neo4j: Not available`n" -ForegroundColor Yellow
}

# Test queries
$tests = @(
    "MATCH (d:Document) RETURN count(d) AS total",
    "MATCH (m:Module) RETURN count(m) AS total",
    "MATCH (c:Class) RETURN count(c) AS total",
    "MATCH (f:Function) RETURN count(f) AS total",
    "MATCH ()-[r:MENTIONS]->() RETURN count(r) AS total"
)

$results = @()

foreach ($query in $tests) {
    Write-Host "Testing: $query" -ForegroundColor Cyan
    
    # Nexus
    $nexusRows = 0
    try {
        $nBody = @{ query = $query } | ConvertTo-Json
        $nRes = Invoke-RestMethod -Uri "$nexusUrl/cypher" -Method POST -Body $nBody -ContentType "application/json"
        $nexusRows = if ($nRes.rows) { $nRes.rows.Count } else { 0 }
        Write-Host "  Nexus: $nexusRows rows" -ForegroundColor Green
    } catch {
        Write-Host "  Nexus: ERROR - $_" -ForegroundColor Red
    }
    
    # Neo4j
    $neo4jRows = 0
    try {
        $auth = [Convert]::ToBase64String([Text.Encoding]::ASCII.GetBytes("${neo4jUser}:${neo4jPass}"))
        $njBody = @{ statements = @(@{ statement = $query }) } | ConvertTo-Json
        $njRes = Invoke-RestMethod -Uri "$neo4jUrl/db/neo4j/tx/commit" -Method POST -Headers @{ "Authorization" = "Basic $auth"; "Content-Type" = "application/json" } -Body $njBody
        if ($njRes.results -and $njRes.results[0].data) {
            $neo4jRows = $njRes.results[0].data.Count
        }
        Write-Host "  Neo4j: $neo4jRows rows" -ForegroundColor Yellow
    } catch {
        Write-Host "  Neo4j: Not available" -ForegroundColor DarkYellow
    }
    
    $match = $nexusRows -eq $neo4jRows
    $status = if ($match) { "MATCH" } else { "DIFF" }
    Write-Host "  Result: $status`n" -ForegroundColor $(if ($match) { "Green" } else { "Yellow" })
    
    $results += [PSCustomObject]@{
        Query = $query
        Nexus = $nexusRows
        Neo4j = $neo4jRows
        Match = $match
    }
}

Write-Host "=== Summary ===" -ForegroundColor Cyan
$results | Format-Table -AutoSize
$matchCount = ($results | Where-Object { $_.Match }).Count
$totalCount = $results.Count
Write-Host "Match Rate: $matchCount/$totalCount ($([math]::Round(($matchCount/$totalCount)*100, 1))%)" -ForegroundColor $(if ($matchCount -eq $totalCount) { "Green" } else { "Yellow" })

