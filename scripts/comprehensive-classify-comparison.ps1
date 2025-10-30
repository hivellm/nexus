$nexusUrl = "http://127.0.0.1:15474"
$neo4jUrl = "http://127.0.0.1:7474"
$neo4jUser = "neo4j"
$neo4jPass = "password"

Write-Host "`n=== Comprehensive Nexus vs Neo4j Comparison ===" -ForegroundColor Cyan
Write-Host "Testing classify cache data compatibility`n" -ForegroundColor Yellow

# Check connections
Write-Host "Checking connections...`n" -ForegroundColor Yellow

$nexusOnline = $false
$neo4jOnline = $false

try {
    Invoke-RestMethod -Uri "$nexusUrl/health" -ErrorAction Stop | Out-Null
    Write-Host "  [OK] Nexus: Online" -ForegroundColor Green
    $nexusOnline = $true
} catch {
    Write-Host "  [ERROR] Nexus: Offline - $_" -ForegroundColor Red
    exit 1
}

try {
    $auth = [Convert]::ToBase64String([Text.Encoding]::ASCII.GetBytes("${neo4jUser}:${neo4jPass}"))
    $testBody = @{ statements = @(@{ statement = "RETURN 1" }) } | ConvertTo-Json
    Invoke-RestMethod -Uri "$neo4jUrl/db/neo4j/tx/commit" -Method POST -Headers @{ "Authorization" = "Basic $auth"; "Content-Type" = "application/json" } -Body $testBody -ErrorAction Stop | Out-Null
    Write-Host "  [OK] Neo4j: Online`n" -ForegroundColor Green
    $neo4jOnline = $true
} catch {
    Write-Host "  [WARN] Neo4j: Not available (comparison disabled)`n" -ForegroundColor Yellow
}

# Comprehensive test queries from classify cache data
$tests = @(
    @{
        Name = "Count Documents"
        Query = "MATCH (d:Document) RETURN count(d) AS total"
        Type = "Count"
    },
    @{
        Name = "Count Modules"
        Query = "MATCH (m:Module) RETURN count(m) AS total"
        Type = "Count"
    },
    @{
        Name = "Count Classes"
        Query = "MATCH (c:Class) RETURN count(c) AS total"
        Type = "Count"
    },
    @{
        Name = "Count Functions"
        Query = "MATCH (f:Function) RETURN count(f) AS total"
        Type = "Count"
    },
    @{
        Name = "Count Relationships (MENTIONS)"
        Query = "MATCH ()-[r:MENTIONS]->() RETURN count(r) AS total"
        Type = "Count"
    },
    @{
        Name = "Count Relationships (IMPORTS)"
        Query = "MATCH ()-[r:IMPORTS]->() RETURN count(r) AS total"
        Type = "Count"
    },
    @{
        Name = "Count All Relationships"
        Query = "MATCH ()-[r]->() RETURN count(r) AS total"
        Type = "Count"
    },
    @{
        Name = "Documents by Domain (top 5)"
        Query = "MATCH (d:Document) RETURN d.domain AS domain, count(d) AS count ORDER BY count DESC LIMIT 5"
        Type = "Aggregate"
    },
    @{
        Name = "Document Mentions Pattern"
        Query = "MATCH (d:Document)-[r:MENTIONS]->(e) RETURN count(r) AS total"
        Type = "Count"
    },
    @{
        Name = "Module Imports Pattern"
        Query = "MATCH (m:Module)-[r:IMPORTS]->(e) RETURN count(r) AS total"
        Type = "Count"
    },
    @{
        Name = "Entities Mentioned by Documents (limit 10)"
        Query = "MATCH (doc:Document)-[:MENTIONS]->(entity) RETURN doc.title, entity.type, entity.name LIMIT 10"
        Type = "Pattern"
    },
    @{
        Name = "Document with specific entity"
        Query = "MATCH (d:Document)-[:MENTIONS]->(e) WHERE e.name = 'PostgreSQL' RETURN d.title, e.name LIMIT 10"
        Type = "Filter"
    },
    @{
        Name = "Classes with methods count"
        Query = "MATCH (c:Class)-[:HAS]->(f:Function) RETURN c.name, count(f) AS method_count LIMIT 10"
        Type = "Aggregate"
    },
    @{
        Name = "Modules with imports count"
        Query = "MATCH (m:Module)-[:IMPORTS]->(imp) RETURN m.name, count(imp) AS import_count LIMIT 10"
        Type = "Aggregate"
    }
)

$results = @()

function Invoke-NexusQuery {
    param($query)
    try {
        $body = @{ query = $query } | ConvertTo-Json
        $res = Invoke-RestMethod -Uri "$nexusUrl/cypher" -Method POST -Body $body -ContentType "application/json"
        
        if ($res.rows -and $res.rows.Count -gt 0) {
            # Extract values from rows
            $rowData = @()
            foreach ($row in $res.rows) {
                if ($row -is [PSCustomObject] -or $row -is [Array]) {
                    # Row is array of values or object
                    if ($row -is [Array]) {
                        $rowData += $row
                    } else {
                        # Convert object to array of property values
                        $values = @()
                        foreach ($prop in $row.PSObject.Properties) {
                            $values += $prop.Value
                        }
                        if ($values.Count -gt 0) {
                            $rowData += ,$values
                        }
                    }
                } else {
                    $rowData += $row
                }
            }
            return @{
                Success = $true
                Rows = $rowData
                RowCount = $rowData.Count
                FirstValue = if ($rowData.Count -gt 0) { $rowData[0] } else { $null }
            }
        }
        return @{ Success = $true; Rows = @(); RowCount = 0; FirstValue = $null }
    } catch {
        return @{ Success = $false; Error = $_.Exception.Message; Rows = @(); RowCount = 0; FirstValue = $null }
    }
}

function Invoke-Neo4jQuery {
    param($query)
    if (-not $neo4jOnline) { return @{ Success = $false; Error = "Neo4j offline"; Rows = @(); RowCount = 0; FirstValue = $null } }
    
    try {
        $auth = [Convert]::ToBase64String([Text.Encoding]::ASCII.GetBytes("${neo4jUser}:${neo4jPass}"))
        $body = @{ statements = @(@{ statement = $query }) } | ConvertTo-Json
        $res = Invoke-RestMethod -Uri "$neo4jUrl/db/neo4j/tx/commit" -Method POST -Headers @{ "Authorization" = "Basic $auth"; "Content-Type" = "application/json" } -Body $body
        
        if ($res.results -and $res.results[0].data) {
            $data = $res.results[0].data
            $rowData = @()
            foreach ($datum in $data) {
                if ($datum.row) {
                    $rowData += ,$datum.row
                }
            }
            return @{
                Success = $true
                Rows = $rowData
                RowCount = $rowData.Count
                FirstValue = if ($rowData.Count -gt 0) { $rowData[0] } else { $null }
            }
        }
        return @{ Success = $true; Rows = @(); RowCount = 0; FirstValue = $null }
    } catch {
        return @{ Success = $false; Error = $_.Exception.Message; Rows = @(); RowCount = 0; FirstValue = $null }
    }
}

function Compare-Values {
    param($val1, $val2)
    
    if ($val1 -eq $null -and $val2 -eq $null) { return $true }
    if ($val1 -eq $null -or $val2 -eq $null) { return $false }
    
    # Handle arrays
    if ($val1 -is [Array] -and $val2 -is [Array]) {
        if ($val1.Count -ne $val2.Count) { return $false }
        for ($i = 0; $i -lt $val1.Count; $i++) {
            if (-not (Compare-Values $val1[$i] $val2[$i])) {
                return $false
            }
        }
        return $true
    }
    
    # Compare simple values
    return $val1 -eq $val2
}

foreach ($test in $tests) {
    Write-Host "Testing: $($test.Name)" -ForegroundColor Cyan
    Write-Host "  Query: $($test.Query)" -ForegroundColor Gray
    
    # Nexus
    $nexusResult = Invoke-NexusQuery -query $test.Query
    if ($nexusResult.Success) {
        $nexusDisplay = if ($test.Type -eq "Count") {
            "$($nexusResult.FirstValue) (rows: $($nexusResult.RowCount))"
        } else {
            "$($nexusResult.RowCount) rows"
        }
        Write-Host "  Nexus: $nexusDisplay" -ForegroundColor Green
    } else {
        Write-Host "  Nexus: ERROR - $($nexusResult.Error)" -ForegroundColor Red
    }
    
    # Neo4j
    $neo4jResult = Invoke-Neo4jQuery -query $test.Query
    if ($neo4jResult.Success) {
        $neo4jDisplay = if ($test.Type -eq "Count") {
            "$($neo4jResult.FirstValue) (rows: $($neo4jResult.RowCount))"
        } else {
            "$($neo4jResult.RowCount) rows"
        }
        Write-Host "  Neo4j: $neo4jDisplay" -ForegroundColor Yellow
    } else {
        Write-Host "  Neo4j: N/A" -ForegroundColor DarkYellow
    }
    
    # Compare
    $match = $false
    $matchDetails = ""
    if ($nexusResult.Success -and $neo4jResult.Success) {
        if ($test.Type -eq "Count") {
            # For count queries, compare first values
            $match = Compare-Values $nexusResult.FirstValue $neo4jResult.FirstValue
            $matchDetails = if ($match) { "MATCH" } else { "DIFF: Nexus=$($nexusResult.FirstValue), Neo4j=$($neo4jResult.FirstValue)" }
        } else {
            # For other queries, compare row counts (basic check)
            $match = $nexusResult.RowCount -eq $neo4jResult.RowCount
            $matchDetails = if ($match) { "MATCH (row count)" } else { "DIFF: Nexus=$($nexusResult.RowCount) rows, Neo4j=$($neo4jResult.RowCount) rows" }
        }
        $color = if ($match) { "Green" } else { "Yellow" }
        Write-Host "  Result: $matchDetails" -ForegroundColor $color
    } else {
        $matchDetails = "N/A (comparison skipped)"
        Write-Host "  Result: $matchDetails" -ForegroundColor DarkGray
    }
    
    Write-Host ""
    
    $results += [PSCustomObject]@{
        Test = $test.Name
        Type = $test.Type
        Query = $test.Query
        Nexus = if ($nexusResult.Success) { 
            if ($test.Type -eq "Count") { $nexusResult.FirstValue.ToString() } else { "$($nexusResult.RowCount) rows" }
        } else { "ERROR" }
        Neo4j = if ($neo4jResult.Success) { 
            if ($test.Type -eq "Count") { $neo4jResult.FirstValue.ToString() } else { "$($neo4jResult.RowCount) rows" }
        } else { "N/A" }
        Match = if ($matchDetails -ne "N/A (comparison skipped)") { $match.ToString() } else { "N/A" }
        Details = $matchDetails
    }
}

Write-Host "=== Summary ===" -ForegroundColor Cyan
$results | Format-Table -AutoSize -Wrap

if ($neo4jOnline) {
    $matchCount = ($results | Where-Object { $_.Match -eq "True" }).Count
    $totalCount = ($results | Where-Object { $_.Match -ne "N/A" }).Count
    if ($totalCount -gt 0) {
        $matchRate = [math]::Round(($matchCount/$totalCount)*100, 1)
        $color = if ($matchCount -eq $totalCount) { "Green" } else { "Yellow" }
        Write-Host "`nMatch Rate: $matchCount/$totalCount tests ($matchRate%)" -ForegroundColor $color
        
        if ($matchCount -lt $totalCount) {
            Write-Host "`nFailed Tests:" -ForegroundColor Red
            $results | Where-Object { $_.Match -eq "False" } | ForEach-Object {
                Write-Host "  - $($_.Test): $($_.Details)" -ForegroundColor Red
            }
        }
    }
}

Write-Host "`n=== Detailed Results ===" -ForegroundColor Cyan
$results | Select-Object Test, Type, Nexus, Neo4j, Match | Format-Table -AutoSize



