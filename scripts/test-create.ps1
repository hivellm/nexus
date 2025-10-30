# Test CREATE to verify if storage is being updated

Write-Host "=== Testing CREATE Operations ===" -ForegroundColor Cyan
Write-Host ""

$baseUrl = "http://localhost:15474"

function Test-Create {
    param([string]$query)
    
    $body = @{ query = $query } | ConvertTo-Json
    
    try {
        $response = Invoke-RestMethod -Uri "$baseUrl/cypher" -Method Post -Body $body -ContentType "application/json" -TimeoutSec 10
        return $response
    } catch {
        Write-Host "Error: $($_.Exception.Message)" -ForegroundColor Red
        return $null
    }
}

# Get initial stats
Write-Host "Initial stats:" -ForegroundColor Yellow
$initialStats = Invoke-RestMethod -Uri "$baseUrl/stats" -Method Get
Write-Host "  Nodes: $($initialStats.catalog.node_count)" -ForegroundColor Gray
Write-Host "  Rels: $($initialStats.catalog.rel_count)" -ForegroundColor Gray
Write-Host ""

# Create a test node
Write-Host "Creating test node..." -ForegroundColor Yellow
$timestamp = Get-Date -Format "yyyy-MM-dd HH:mm:ss"
$result = Test-Create "CREATE (n:TestNode {name: 'TestCreate', timestamp: '$timestamp'}) RETURN id(n) AS node_id"
if ($result) {
    Write-Host "  Result: $($result | ConvertTo-Json -Depth 5)" -ForegroundColor Cyan
}
Write-Host ""
Start-Sleep -Seconds 1

# Get updated stats
Write-Host "After CREATE stats:" -ForegroundColor Yellow
$afterStats = Invoke-RestMethod -Uri "$baseUrl/stats" -Method Get
Write-Host "  Nodes: $($afterStats.catalog.node_count) (was $($initialStats.catalog.node_count))" -ForegroundColor Gray
Write-Host "  Rels: $($afterStats.catalog.rel_count) (was $($initialStats.catalog.rel_count))" -ForegroundColor Gray
Write-Host ""

# Try to MATCH the created node
Write-Host "Trying to MATCH the created node..." -ForegroundColor Yellow
$result = Test-Create "MATCH (n:TestNode) WHERE n.name = 'TestCreate' RETURN n.name, n.timestamp"
if ($result -and $result.rows) {
    Write-Host "  ✅ Found $($result.rows.Count) nodes!" -ForegroundColor Green
    $result.rows | ForEach-Object { Write-Host "    $_" -ForegroundColor Cyan }
} else {
    Write-Host "  ❌ Node not found via MATCH!" -ForegroundColor Red
}
Write-Host ""

Write-Host "=== Test Complete ===" -ForegroundColor Cyan

