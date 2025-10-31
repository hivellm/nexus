# Test if DELETE query is being parsed

$NexusUri = "http://localhost:15474"

Write-Host "[TEST] Testing DELETE query parsing`n" -ForegroundColor Cyan

# Test 1: Simple query
Write-Host "1. Testing: MATCH (n) RETURN count(*)" -ForegroundColor Yellow
$body1 = @{ query = "MATCH (n) RETURN count(*) AS count" } | ConvertTo-Json
try {
    $r1 = Invoke-RestMethod -Uri "$NexusUri/cypher" -Method POST -Headers @{"Content-Type" = "application/json"} -Body $body1
    Write-Host "   Response type: $($r1.GetType().Name)" -ForegroundColor Cyan
    Write-Host "   Has rows: $($r1.rows -ne $null)" -ForegroundColor Cyan
    Write-Host "   Row count: $($r1.rows.Count)" -ForegroundColor Cyan
} catch {
    Write-Host "   [ERROR] $_" -ForegroundColor Red
}

# Test 2: DELETE query
Write-Host "`n2. Testing: MATCH (n) DETACH DELETE n" -ForegroundColor Yellow
$body2 = @{ query = "MATCH (n) DETACH DELETE n" } | ConvertTo-Json
try {
    $r2 = Invoke-RestMethod -Uri "$NexusUri/cypher" -Method POST -Headers @{"Content-Type" = "application/json"} -Body $body2
    Write-Host "   Response type: $($r2.GetType().Name)" -ForegroundColor Cyan
    Write-Host "   Has rows: $($r2.rows -ne $null)" -ForegroundColor Cyan
    Write-Host "   Row count: $($r2.rows.Count)" -ForegroundColor Cyan
    
    if ($r2.rows.Count -eq 0) {
        Write-Host "   [PASS] DELETE returned empty result!" -ForegroundColor Green
    } else {
        Write-Host "   [FAIL] DELETE returned $($r2.rows.Count) rows instead of 0!" -ForegroundColor Red
    }
} catch {
    Write-Host "   [ERROR] $_" -ForegroundColor Red
}

# Test 3: DELETE query variant
Write-Host "`n3. Testing: MATCH (n) DELETE n" -ForegroundColor Yellow
$body3 = @{ query = "MATCH (n) DELETE n" } | ConvertTo-Json
try {
    $r3 = Invoke-RestMethod -Uri "$NexusUri/cypher" -Method POST -Headers @{"Content-Type" = "application/json"} -Body $body3
    Write-Host "   Response type: $($r3.GetType().Name)" -ForegroundColor Cyan
    Write-Host "   Has rows: $($r3.rows -ne $null)" -ForegroundColor Cyan
    Write-Host "   Row count: $($r3.rows.Count)" -ForegroundColor Cyan
    
    if ($r3.rows.Count -eq 0) {
        Write-Host "   [PASS] DELETE returned empty result!" -ForegroundColor Green
    } else {
        Write-Host "   [FAIL] DELETE returned $($r3.rows.Count) rows instead of 0!" -ForegroundColor Red
    }
} catch {
    Write-Host "   [ERROR] $_" -ForegroundColor Red
}

