#!/usr/bin/env pwsh
# Check database stats

Write-Host "=== Checking Database Status ===" -ForegroundColor Cyan

try {
    $stats = Invoke-RestMethod -Uri "http://localhost:15474/stats" -Method GET -TimeoutSec 5
    
    Write-Host "`nDatabase Stats:" -ForegroundColor Green
    $stats | ConvertTo-Json -Depth 10
    
    Write-Host "`nNode count: $($stats.node_count)" -ForegroundColor Yellow
    Write-Host "Relationship count: $($stats.relationship_count)" -ForegroundColor Yellow
    
    if ($stats.node_count -eq 0) {
        Write-Host "`nWARNING: Database is empty! Need to import data." -ForegroundColor Red
    }
} catch {
    Write-Host "ERROR: Could not get stats - $_" -ForegroundColor Red
}

# Try a simple MATCH query
Write-Host "`n=== Testing Simple MATCH Query ===" -ForegroundColor Cyan
$matchQuery = "MATCH (d:Document) RETURN d LIMIT 1"
Write-Host "Query: $matchQuery" -ForegroundColor Yellow

try {
    $response = Invoke-RestMethod -Uri "http://localhost:15474/cypher" -Method POST -Body (@{
        query = $matchQuery
    } | ConvertTo-Json) -ContentType "application/json" -TimeoutSec 10
    
    Write-Host "`nMatch Result:" -ForegroundColor Green
    Write-Host "Rows returned: $($response.rows.Count)" -ForegroundColor Cyan
    if ($response.rows.Count -gt 0) {
        Write-Host "First row:" -ForegroundColor Yellow
        $response.rows[0] | ConvertTo-Json -Depth 5
    }
} catch {
    Write-Host "ERROR: $_" -ForegroundColor Red
}



