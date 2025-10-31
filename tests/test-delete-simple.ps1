# Simple DELETE test to isolate the issue

$NexusUri = "http://localhost:15474"

function Invoke-NexusQuery {
    param([string]$Cypher, [switch]$ShowRows)
    
    $body = @{ query = $Cypher } | ConvertTo-Json
    
    try {
        $response = Invoke-RestMethod -Uri "$NexusUri/cypher" `
            -Method POST `
            -Headers @{"Content-Type" = "application/json"} `
            -Body $body `
            -ErrorAction Stop
        
        if ($ShowRows) {
            Write-Host "   Response rows: $($response.rows.Count)" -ForegroundColor Cyan
            if ($response.rows.Count -gt 0 -and $response.rows[0] -is [array]) {
                Write-Host "   Value: $($response.rows[0][0])" -ForegroundColor Cyan
            }
        }
        
        return $response
    }
    catch {
        Write-Host "[ERROR] $_" -ForegroundColor Red
        return $null
    }
}

Write-Host "`n[TEST] Simple DELETE Test`n" -ForegroundColor Cyan

# Step 1: Count existing nodes
Write-Host "1. Counting existing nodes..." -ForegroundColor Yellow
$r1 = Invoke-NexusQuery -Cypher "MATCH (n) RETURN count(*) AS count" -ShowRows

# Step 2: Try DETACH DELETE ALL
Write-Host "`n2. Running MATCH (n) DETACH DELETE n..." -ForegroundColor Yellow
$r2 = Invoke-NexusQuery -Cypher "MATCH (n) DETACH DELETE n" -ShowRows

# Step 3: Count after delete
Write-Host "`n3. Counting after DETACH DELETE..." -ForegroundColor Yellow
$r3 = Invoke-NexusQuery -Cypher "MATCH (n) RETURN count(*) AS count" -ShowRows

$finalCount = if ($r3.rows[0] -is [array]) { $r3.rows[0][0] } else { $r3.rows[0].values[0] }

if ($finalCount -eq 0) {
    Write-Host "`n[PASS] DELETE works! All nodes removed." -ForegroundColor Green
} else {
    Write-Host "`n[FAIL] DELETE failed! $finalCount nodes remain." -ForegroundColor Red
    
    # Show remaining nodes
    Write-Host "`nListing remaining nodes..." -ForegroundColor Yellow
    $r4 = Invoke-NexusQuery -Cypher "MATCH (n) RETURN labels(n), n.name LIMIT 10"
    Write-Host "   Found $($r4.rows.Count) nodes" -ForegroundColor Cyan
}

