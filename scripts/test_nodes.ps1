$response = Invoke-RestMethod -Uri 'http://localhost:15474/cypher' -Method POST -Headers @{'Content-Type'='application/json'} -Body '{"query": "MATCH (a:Node) RETURN a.id ORDER BY a.id"}'
Write-Host "Nodes found: $($response.rows.Count)"
$response.rows | ForEach-Object { Write-Host "  $_" }
