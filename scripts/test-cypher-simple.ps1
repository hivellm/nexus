# Test Cypher endpoint with simple query
Write-Host "Waiting for server to start..." -ForegroundColor Yellow
Start-Sleep -Seconds 3

Write-Host "Testing Cypher endpoint..." -ForegroundColor Cyan

$body = '{"query":"MATCH (n) RETURN count(n) AS total"}'

try {
    $response = Invoke-RestMethod -Uri "http://localhost:15474/cypher" -Method Post -Body $body -ContentType "application/json" -TimeoutSec 10
    Write-Host "✅ Success!" -ForegroundColor Green
    $response | ConvertTo-Json -Depth 10
} catch {
    Write-Host "❌ Error!" -ForegroundColor Red
    Write-Host "StatusCode:" $_.Exception.Response.StatusCode.value__
    Write-Host "Message:" $_.Exception.Message
}

