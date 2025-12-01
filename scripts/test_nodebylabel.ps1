# Test if NodeByLabel returns all nodes
$response = Invoke-RestMethod -Uri 'http://localhost:15474/cypher' -Method POST -Headers @{'Content-Type'='application/json'} -Body '{
  "query": "MATCH (a:Node) RETURN a.id ORDER BY a.id"
}'

Write-Host "NodeByLabel test - all nodes with label :Node"
Write-Host "Row count: $($response.rows.Count)"
Write-Host "Expected: 6 (nodes 1-6)"
$response.rows | ForEach-Object { Write-Host "  $_" }

# Test if we can start pattern from each node
Write-Host "`nStarting pattern from node 1:"
$r1 = Invoke-RestMethod -Uri 'http://localhost:15474/cypher' -Method POST -Headers @{'Content-Type'='application/json'} -Body '{
  "query": "MATCH (a:Node {id: 1})-[:CONNECTS]->(b:Node)-[:CONNECTS]->(c:Node)-[:CONNECTS]->(a) RETURN a.id, b.id, c.id"
}'
Write-Host "Result: $($r1.rows)"

Write-Host "`nStarting pattern from node 2:"
$r2 = Invoke-RestMethod -Uri 'http://localhost:15474/cypher' -Method POST -Headers @{'Content-Type'='application/json'} -Body '{
  "query": "MATCH (a:Node {id: 2})-[:CONNECTS]->(b:Node)-[:CONNECTS]->(c:Node)-[:CONNECTS]->(a) RETURN a.id, b.id, c.id"
}'
Write-Host "Result: $($r2.rows)"

Write-Host "`nStarting pattern from node 3:"
$r3 = Invoke-RestMethod -Uri 'http://localhost:15474/cypher' -Method POST -Headers @{'Content-Type'='application/json'} -Body '{
  "query": "MATCH (a:Node {id: 3})-[:CONNECTS]->(b:Node)-[:CONNECTS]->(c:Node)-[:CONNECTS]->(a) RETURN a.id, b.id, c.id"
}'
Write-Host "Result: $($r3.rows)"
