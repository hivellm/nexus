$body = '{"query":"MATCH (n) RETURN count(n) AS total"}'
$response = Invoke-RestMethod -Uri "http://localhost:15474/cypher" -Method Post -Body $body -ContentType "application/json"
Write-Host "Response:" -ForegroundColor Green
$response | ConvertTo-Json -Depth 10

