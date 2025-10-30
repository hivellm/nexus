$body = '{"query":"MATCH (n) RETURN id(n) AS node_id, n"}'
$response = Invoke-RestMethod -Uri "http://localhost:15474/cypher" -Method Post -Body $body -ContentType "application/json"
Write-Host "Query: MATCH (n) RETURN id(n), n"
$response | ConvertTo-Json -Depth 10

