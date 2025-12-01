# Test triangle pattern with debug logging

Write-Host "Testing triangle pattern..." -ForegroundColor Cyan

# Test without DISTINCT first
$response1 = Invoke-RestMethod -Uri 'http://localhost:15474/cypher' -Method POST -Headers @{'Content-Type'='application/json'} -Body '{
  "query": "MATCH (a:Node)-[:CONNECTS]->(b:Node)-[:CONNECTS]->(c:Node)-[:CONNECTS]->(a) RETURN a.id AS node_a, b.id AS node_b, c.id AS node_c ORDER BY node_a, node_b, node_c"
}'

Write-Host "`nWithout DISTINCT:" -ForegroundColor Yellow
Write-Host "Row count: $($response1.rows.Count)"
Write-Host "Rows:"
$response1.rows | ForEach-Object { Write-Host "  $_" }

# Test with DISTINCT
$response2 = Invoke-RestMethod -Uri 'http://localhost:15474/cypher' -Method POST -Headers @{'Content-Type'='application/json'} -Body '{
  "query": "MATCH (a:Node)-[:CONNECTS]->(b:Node)-[:CONNECTS]->(c:Node)-[:CONNECTS]->(a) RETURN DISTINCT a.id AS node ORDER BY node"
}'

Write-Host "`nWith DISTINCT:" -ForegroundColor Yellow
Write-Host "Row count: $($response2.rows.Count)"
Write-Host "Expected: 3 rows (nodes 1, 2, 3)"
Write-Host "Rows:"
$response2.rows | ForEach-Object { Write-Host "  $_" }

# Test simple 2-hop pattern
$response3 = Invoke-RestMethod -Uri 'http://localhost:15474/cypher' -Method POST -Headers @{'Content-Type'='application/json'} -Body '{
  "query": "MATCH (a:Node)-[:CONNECTS]->(b:Node)-[:CONNECTS]->(c:Node) RETURN a.id, b.id, c.id ORDER BY a.id, b.id, c.id"
}'

Write-Host "`n2-hop pattern (no cycle):" -ForegroundColor Yellow
Write-Host "Row count: $($response3.rows.Count)"
Write-Host "Rows:"
$response3.rows | ForEach-Object { Write-Host "  $_" }
