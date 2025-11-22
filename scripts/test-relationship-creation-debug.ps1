# Script para testar criação de relationships e verificar se first_rel_ptr está sendo atualizado
$ErrorActionPreference = "Stop"

$baseUrl = "http://localhost:15474/cypher"
$neo4jUrl = "http://localhost:7474/db/neo4j/tx/commit"

Write-Host "=== Test: Relationship Creation and first_rel_ptr Debug ===" -ForegroundColor Cyan

# Limpar dados existentes
Write-Host "`n1. Limpando dados existentes..." -ForegroundColor Yellow
$clearQuery = "MATCH (n) DETACH DELETE n RETURN count(n) AS deleted"
try {
    $clearBody = @{ query = $clearQuery } | ConvertTo-Json -Depth 10
    $clearResponse = Invoke-RestMethod -Uri $baseUrl -Method POST -Headers @{ 'Content-Type' = 'application/json' } -Body $clearBody
    Write-Host "   Nexus: Deleted $($clearResponse.rows[0][0]) nodes" -ForegroundColor Green
} catch {
    Write-Host "   Nexus: Error clearing - $_" -ForegroundColor Red
}

Start-Sleep -Seconds 1

# Criar nodes de teste
Write-Host "`n2. Criando nodes de teste..." -ForegroundColor Yellow
$createNodesQuery = @"
CREATE 
  (a:Person {name: 'Alice', age: 30}),
  (b:Person {name: 'Bob', age: 25}),
  (c1:Company {name: 'Acme', founded: 2000}),
  (c2:Company {name: 'TechCorp', founded: 2010})
RETURN id(a) AS alice_id, id(b) AS bob_id, id(c1) AS acme_id, id(c2) AS techcorp_id
"@

try {
    $createBody = @{ query = $createNodesQuery } | ConvertTo-Json -Depth 10
    $createResponse = Invoke-RestMethod -Uri $baseUrl -Method POST -Headers @{ 'Content-Type' = 'application/json' } -Body $createBody
    $aliceId = $createResponse.rows[0][0]
    $bobId = $createResponse.rows[0][1]
    $acmeId = $createResponse.rows[0][2]
    $techcorpId = $createResponse.rows[0][3]
    Write-Host "   Nodes criados: Alice=$aliceId, Bob=$bobId, Acme=$acmeId, TechCorp=$techcorpId" -ForegroundColor Green
} catch {
    Write-Host "   Erro criando nodes: $_" -ForegroundColor Red
    exit 1
}

Start-Sleep -Seconds 1

# Criar primeiro relationship
Write-Host "`n3. Criando primeiro relationship (Alice WORKS_AT Acme)..." -ForegroundColor Yellow
$createRel1Query = @"
MATCH (a:Person {name: 'Alice'}), (c:Company {name: 'Acme'})
CREATE (a)-[:WORKS_AT {since: 2020}]->(c)
RETURN id(a) AS alice_id, id(c) AS company_id
"@

try {
    $rel1Body = @{ query = $createRel1Query } | ConvertTo-Json -Depth 10
    $rel1Response = Invoke-RestMethod -Uri $baseUrl -Method POST -Headers @{ 'Content-Type' = 'application/json' } -Body $rel1Body
    Write-Host "   Relationship criado com sucesso" -ForegroundColor Green
} catch {
    Write-Host "   Erro criando relationship: $_" -ForegroundColor Red
}

Start-Sleep -Seconds 1

# Verificar se relationship foi criado
Write-Host "`n4. Verificando se relationship foi encontrado..." -ForegroundColor Yellow
$checkRel1Query = "MATCH (a:Person {name: 'Alice'})-[r:WORKS_AT]->(c:Company) RETURN count(r) AS count"

try {
    $check1Body = @{ query = $checkRel1Query } | ConvertTo-Json -Depth 10
    $check1Response = Invoke-RestMethod -Uri $baseUrl -Method POST -Headers @{ 'Content-Type' = 'application/json' } -Body $check1Body
    $count = $check1Response.rows[0][0]
    Write-Host "   Relationships encontrados: $count (esperado: 1)" -ForegroundColor $(if ($count -eq 1) { "Green" } else { "Red" })
} catch {
    Write-Host "   Erro verificando relationships: $_" -ForegroundColor Red
}

# Verificar count total de relationships
Write-Host "`n5. Verificando count total de relationships..." -ForegroundColor Yellow
$countAllQuery = "MATCH ()-[r]->() RETURN count(r) AS total"

try {
    $countBody = @{ query = $countAllQuery } | ConvertTo-Json -Depth 10
    $countResponse = Invoke-RestMethod -Uri $baseUrl -Method POST -Headers @{ 'Content-Type' = 'application/json' } -Body $countBody
    $total = $countResponse.rows[0][0]
    Write-Host "   Total de relationships no banco: $total (esperado: >= 1)" -ForegroundColor $(if ($total -ge 1) { "Green" } else { "Red" })
} catch {
    Write-Host "   Erro contando relationships: $_" -ForegroundColor Red
}

# Verificar stats do catalog
Write-Host "`n6. Verificando stats do catalog..." -ForegroundColor Yellow
try {
    $statsResponse = Invoke-RestMethod -Uri "http://localhost:15474/stats" -Method GET
    $relCount = $statsResponse.catalog.rel_count
    Write-Host "   Catalog rel_count: $relCount (esperado: >= 1)" -ForegroundColor $(if ($relCount -ge 1) { "Green" } else { "Red" })
} catch {
    Write-Host "   Erro obtendo stats: $_" -ForegroundColor Red
}

# Criar segundo relationship para o mesmo node (Alice)
Write-Host "`n7. Criando segundo relationship (Alice WORKS_AT TechCorp)..." -ForegroundColor Yellow
$createRel2Query = @"
MATCH (a:Person {name: 'Alice'}), (c:Company {name: 'TechCorp'})
CREATE (a)-[:WORKS_AT {since: 2022}]->(c)
RETURN id(a) AS alice_id, id(c) AS company_id
"@

try {
    $rel2Body = @{ query = $createRel2Query } | ConvertTo-Json -Depth 10
    $rel2Response = Invoke-RestMethod -Uri $baseUrl -Method POST -Headers @{ 'Content-Type' = 'application/json' } -Body $rel2Body
    Write-Host "   Segundo relationship criado com sucesso" -ForegroundColor Green
} catch {
    Write-Host "   Erro criando segundo relationship: $_" -ForegroundColor Red
}

Start-Sleep -Seconds 1

# Verificar se ambos relationships foram encontrados
Write-Host "`n8. Verificando se ambos relationships foram encontrados..." -ForegroundColor Yellow
$checkRel2Query = "MATCH (a:Person {name: 'Alice'})-[r:WORKS_AT]->(c:Company) RETURN count(r) AS count"

try {
    $check2Body = @{ query = $checkRel2Query } | ConvertTo-Json -Depth 10
    $check2Response = Invoke-RestMethod -Uri $baseUrl -Method POST -Headers @{ 'Content-Type' = 'application/json' } -Body $check2Body
    $count2 = $check2Response.rows[0][0]
    Write-Host "   Relationships encontrados: $count2 (esperado: 2)" -ForegroundColor $(if ($count2 -eq 2) { "Green" } else { "Red" })
} catch {
    Write-Host "   Erro verificando relationships: $_" -ForegroundColor Red
}

# Verificar query completa
Write-Host "`n9. Verificando query completa com retorno de properties..." -ForegroundColor Yellow
$fullQuery = "MATCH (a:Person {name: 'Alice'})-[r:WORKS_AT]->(c:Company) RETURN a.name AS person, c.name AS company, r.since AS year ORDER BY year"

try {
    $fullBody = @{ query = $fullQuery } | ConvertTo-Json -Depth 10
    $fullResponse = Invoke-RestMethod -Uri $baseUrl -Method POST -Headers @{ 'Content-Type' = 'application/json' } -Body $fullBody
    $rowCount = $fullResponse.rows.Count
    Write-Host "   Rows retornadas: $rowCount (esperado: 2)" -ForegroundColor $(if ($rowCount -eq 2) { "Green" } else { "Red" })
    if ($rowCount -gt 0) {
        Write-Host "   Primeiras rows:" -ForegroundColor Cyan
        foreach ($row in $fullResponse.rows[0..([Math]::Min(2, $rowCount-1))]) {
            Write-Host "     Person: $($row[0]), Company: $($row[1]), Year: $($row[2])" -ForegroundColor Cyan
        }
    }
} catch {
    Write-Host "   Erro executando query completa: $_" -ForegroundColor Red
}

Write-Host "`n=== Teste concluído ===" -ForegroundColor Cyan

