# Script para testar múltiplos relationships no mesmo node
$ErrorActionPreference = "Stop"

$baseUrl = "http://localhost:15474/cypher"

Write-Host "=== Test: Multiple Relationships on Same Node ===" -ForegroundColor Cyan

# Limpar
Write-Host "`n1. Limpando..." -ForegroundColor Yellow
$clearQuery = "MATCH (n) DETACH DELETE n RETURN count(n) AS deleted"
$clearBody = @{ query = $clearQuery } | ConvertTo-Json -Depth 10
$clearResponse = Invoke-RestMethod -Uri $baseUrl -Method POST -Headers @{ 'Content-Type' = 'application/json' } -Body $clearBody
Write-Host "   Deleted: $($clearResponse.rows[0][0]) nodes" -ForegroundColor Green

Start-Sleep -Seconds 1

# Criar nodes
Write-Host "`n2. Criando nodes..." -ForegroundColor Yellow
$createNodesQuery = "CREATE (a:Person {name: 'Alice'}), (c1:Company {name: 'Acme'}), (c2:Company {name: 'TechCorp'}) RETURN id(a) AS alice_id"
$createNodesBody = @{ query = $createNodesQuery } | ConvertTo-Json -Depth 10
$createNodesResponse = Invoke-RestMethod -Uri $baseUrl -Method POST -Headers @{ 'Content-Type' = 'application/json' } -Body $createNodesBody
Write-Host "   Nodes created" -ForegroundColor Green

Start-Sleep -Seconds 1

# Criar primeiro relationship
Write-Host "`n3. Criando primeiro relationship..." -ForegroundColor Yellow
$createRel1Query = "MATCH (a:Person {name: 'Alice'}), (c:Company {name: 'Acme'}) CREATE (a)-[:WORKS_AT {since: 2020}]->(c) RETURN count(*) AS created"
$createRel1Body = @{ query = $createRel1Query } | ConvertTo-Json -Depth 10
$createRel1Response = Invoke-RestMethod -Uri $baseUrl -Method POST -Headers @{ 'Content-Type' = 'application/json' } -Body $createRel1Body
Write-Host "   First relationship created" -ForegroundColor Green

Start-Sleep -Seconds 1

# Criar segundo relationship
Write-Host "`n4. Criando segundo relationship..." -ForegroundColor Yellow
$createRel2Query = "MATCH (a:Person {name: 'Alice'}), (c:Company {name: 'TechCorp'}) CREATE (a)-[:WORKS_AT {since: 2022}]->(c) RETURN count(*) AS created"
$createRel2Body = @{ query = $createRel2Query } | ConvertTo-Json -Depth 10
$createRel2Response = Invoke-RestMethod -Uri $baseUrl -Method POST -Headers @{ 'Content-Type' = 'application/json' } -Body $createRel2Body
Write-Host "   Second relationship created" -ForegroundColor Green

Start-Sleep -Seconds 1

# Verificar count
Write-Host "`n5. Verificando count de relationships..." -ForegroundColor Yellow
$checkQuery = "MATCH (a:Person {name: 'Alice'})-[r:WORKS_AT]->() RETURN count(r) AS count"
$checkBody = @{ query = $checkQuery } | ConvertTo-Json -Depth 10
$checkResponse = Invoke-RestMethod -Uri $baseUrl -Method POST -Headers @{ 'Content-Type' = 'application/json' } -Body $checkBody
$count = $checkResponse.rows[0][0]
Write-Host "   Count: $count (expected: 2)" -ForegroundColor $(if ($count -eq 2) { "Green" } else { "Red" })

# Verificar query completa
Write-Host "`n6. Verificando query completa..." -ForegroundColor Yellow
$fullQuery = "MATCH (a:Person {name: 'Alice'})-[r:WORKS_AT]->(c:Company) RETURN a.name AS person, c.name AS company, r.since AS year ORDER BY year"
$fullBody = @{ query = $fullQuery } | ConvertTo-Json -Depth 10
$fullResponse = Invoke-RestMethod -Uri $baseUrl -Method POST -Headers @{ 'Content-Type' = 'application/json' } -Body $fullBody
$rowCount = $fullResponse.rows.Count
Write-Host "   Rows returned: $rowCount (expected: 2)" -ForegroundColor $(if ($rowCount -eq 2) { "Green" } else { "Red" })
if ($rowCount -gt 0) {
    foreach ($row in $fullResponse.rows) {
        Write-Host "     Person: $($row[0]), Company: $($row[1]), Year: $($row[2])" -ForegroundColor Cyan
    }
}

Write-Host "`n=== Teste concluido ===" -ForegroundColor Cyan

