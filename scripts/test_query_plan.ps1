# Test to see what query plan is being used
Write-Host "Testing query without filter (should have NodeByLabel for 'a'):"
Write-Host "MATCH (a:Node)-[:CONNECTS]->(b:Node)-[:CONNECTS]->(c:Node)-[:CONNECTS]->(a) RETURN a.id, b.id, c.id"

$response = Invoke-RestMethod -Uri 'http://localhost:15474/cypher' -Method POST -Headers @{'Content-Type'='application/json'} -Body '{
  "query": "MATCH (a:Node)-[:CONNECTS]->(b:Node)-[:CONNECTS]->(c:Node)-[:CONNECTS]->(a) RETURN a.id, b.id, c.id ORDER BY a.id, b.id, c.id"
}'

Write-Host "`nRow count: $($response.rows.Count)"
Write-Host "Expected: 3 rows"
Write-Host "Rows:"
$response.rows | ForEach-Object { Write-Host "  $_" }
