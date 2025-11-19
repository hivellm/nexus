$body = '{"query": "MATCH (n) RETURN count(n) AS total"}'
Invoke-RestMethod -Uri "http://localhost:15474/cypher" -Method POST -ContentType "application/json" -Body $body
