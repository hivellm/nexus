# Simple cache test
Write-Host "=== Testing Cache with Identical Queries ===" -ForegroundColor Cyan

$query1 = '{"query": "MATCH (n) RETURN count(n) AS total"}'
$query2 = '{"query": "MATCH (n) RETURN count(n) AS total"}'

# First query
Write-Host "First query..."
$start1 = Get-Date
$response1 = Invoke-RestMethod -Uri "http://localhost:15474/cypher" -Method POST -ContentType "application/json" -Body $query1
$time1 = ((Get-Date) - $start1).TotalMilliseconds
Write-Host "Time: $($time1)ms, Result: $($response1.rows[0][0])"

# Second identical query
Write-Host "Second identical query..."
$start2 = Get-Date
$response2 = Invoke-RestMethod -Uri "http://localhost:15474/cypher" -Method POST -ContentType "application/json" -Body $query2
$time2 = ((Get-Date) - $start2).TotalMilliseconds
Write-Host "Time: $($time2)ms, Result: $($response2.rows[0][0])"

# Calculate improvement
$improvement = (($time1 - $time2) / $time1) * 100
Write-Host "Improvement: $($improvement.ToString("F1"))%"

if ($improvement -gt 30) {
    Write-Host "✓ Cache appears to be working!" -ForegroundColor Green
} elseif ($improvement -gt 10) {
    Write-Host "! Slight improvement detected" -ForegroundColor Yellow
} else {
    Write-Host "✗ No significant cache effect" -ForegroundColor Red
}
