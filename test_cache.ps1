# Test script to check if cache is working
Write-Host "Testing Nexus cache..."

# Test query
$query = '{"query": "MATCH (n) RETURN count(n) AS total"}'

# Make first request
Write-Host "First request..."
$start1 = Get-Date
try {
    $response1 = Invoke-RestMethod -Uri "http://localhost:8080/cypher" -Method POST -ContentType "application/json" -Body $query -TimeoutSec 10
    $time1 = ((Get-Date) - $start1).TotalMilliseconds
    Write-Host "First request: $($time1)ms - Result: $($response1.rows[0][0])"
} catch {
    Write-Host "First request failed: $($_.Exception.Message)"
    exit 1
}

# Make second request (should be cached)
Write-Host "Second request (should be cached)..."
$start2 = Get-Date
try {
    $response2 = Invoke-RestMethod -Uri "http://localhost:8080/cypher" -Method POST -ContentType "application/json" -Body $query -TimeoutSec 10
    $time2 = ((Get-Date) - $start2).TotalMilliseconds
    Write-Host "Second request: $($time2)ms - Result: $($response2.rows[0][0])"
} catch {
    Write-Host "Second request failed: $($_.Exception.Message)"
    exit 1
}

# Calculate improvement
if ($time1 -gt 0) {
    $improvement = (($time1 - $time2) / $time1) * 100
    Write-Host "Cache improvement: $($improvement.ToString("F1"))%"
    if ($improvement -gt 50) {
        Write-Host "✓ Cache is working!"
    } else {
        Write-Host "✗ Cache may not be working properly"
    }
}
