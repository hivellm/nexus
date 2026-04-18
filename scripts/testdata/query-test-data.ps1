# Script to query test data from Nexus
# Usage: .\scripts\query-test-data.ps1

$baseUrl = "http://localhost:15474"

Write-Host "Querying test data from Nexus...`n" -ForegroundColor Green

# Get stats
Write-Host "=== Database Statistics ===" -ForegroundColor Cyan
try {
    $stats = Invoke-RestMethod -Uri "$baseUrl/stats" -Method GET
    Write-Host "Nodes: $($stats.catalog.node_count)" -ForegroundColor White
    Write-Host "Relationships: $($stats.catalog.rel_count)" -ForegroundColor White
    Write-Host "Labels: $($stats.catalog.label_count)" -ForegroundColor White
    Write-Host "Relationship Types: $($stats.catalog.rel_type_count)" -ForegroundColor White
} catch {
    Write-Host "Error getting stats: $_" -ForegroundColor Red
}

Write-Host "`n=== All Nodes ===" -ForegroundColor Cyan
$query = @{
    query = "MATCH (n) RETURN n LIMIT 10"
    params = @{}
} | ConvertTo-Json

try {
    $result = Invoke-RestMethod -Uri "$baseUrl/cypher" -Method POST -Body $query -ContentType "application/json"
    if ($result.rows) {
        foreach ($row in $result.rows) {
            Write-Host $row | ConvertTo-Json -Depth 5
        }
    }
} catch {
    Write-Host "Error querying nodes: $_" -ForegroundColor Red
}

Write-Host "`n=== Person Nodes ===" -ForegroundColor Cyan
$query = @{
    query = "MATCH (p:Person) RETURN p.name, p.age, p.email"
    params = @{}
} | ConvertTo-Json

try {
    $result = Invoke-RestMethod -Uri "$baseUrl/cypher" -Method POST -Body $query -ContentType "application/json"
    if ($result.rows) {
        foreach ($row in $result.rows) {
            Write-Host "Name: $($row.'p.name'), Age: $($row.'p.age'), Email: $($row.'p.email')" -ForegroundColor White
        }
    }
} catch {
    Write-Host "Error querying persons: $_" -ForegroundColor Red
}

Write-Host "`n=== Relationships ===" -ForegroundColor Cyan
$query = @{
    query = "MATCH (a)-[r]->(b) RETURN a.name, type(r), b.name LIMIT 10"
    params = @{}
} | ConvertTo-Json

try {
    $result = Invoke-RestMethod -Uri "$baseUrl/cypher" -Method POST -Body $query -ContentType "application/json"
    if ($result.rows) {
        foreach ($row in $result.rows) {
            $from = if ($row.'a.name') { $row.'a.name' } else { "Node" }
            $rel = $row.'type(r)'
            $to = if ($row.'b.name') { $row.'b.name' } else { "Node" }
            Write-Host "$from -[$rel]-> $to" -ForegroundColor White
        }
    }
} catch {
    Write-Host "Error querying relationships: $_" -ForegroundColor Red
}

