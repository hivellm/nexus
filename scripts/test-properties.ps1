#!/usr/bin/env pwsh
$ErrorActionPreference = "Stop"

$body = @{ query = "MATCH (n:Document) RETURN n LIMIT 1" } | ConvertTo-Json
$result = Invoke-RestMethod -Uri "http://127.0.0.1:15474/cypher" -Method POST -Body $body -ContentType "application/json"

Write-Host "=== Document Node Test ===" -ForegroundColor Cyan
Write-Host "Columns: $($result.columns -join ', ')" -ForegroundColor Yellow
Write-Host "Row count: $($result.rows.Count)" -ForegroundColor Yellow

if ($result.rows.Count -gt 0) {
    $node = $result.rows[0][0]
    Write-Host "`nNode structure:" -ForegroundColor Green
    $node | ConvertTo-Json -Depth 10
    
    Write-Host "`nNode type: $($node.GetType().FullName)" -ForegroundColor Yellow
    
    if ($node -is [PSCustomObject]) {
        Write-Host "`nProperties:" -ForegroundColor Green
        $node.PSObject.Properties | ForEach-Object {
            Write-Host "  $($_.Name) = $($_.Value)"
        }
    }
}

# Test keys() function
Write-Host "`n=== Testing keys() function ===" -ForegroundColor Cyan
$body2 = @{ query = "MATCH (n:Document) RETURN keys(n) AS keys LIMIT 1" } | ConvertTo-Json
$result2 = Invoke-RestMethod -Uri "http://127.0.0.1:15474/cypher" -Method POST -Body $body2 -ContentType "application/json"

Write-Host "Keys result:" -ForegroundColor Yellow
$result2 | ConvertTo-Json -Depth 10

