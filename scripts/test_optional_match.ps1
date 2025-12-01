$response = Invoke-RestMethod -Uri 'http://localhost:15474/cypher' -Method POST -Headers @{'Content-Type'='application/json'} -Body '{
  "query": "MATCH (n:Node) OPTIONAL MATCH (n)-[:CONNECTS]->() RETURN n.id, count(*) AS out_degree ORDER BY n.id"
}'

Write-Host "Row count: $($response.rows.Count)"
Write-Host "Rows:"
$response.rows | ForEach-Object { Write-Host $_ }
