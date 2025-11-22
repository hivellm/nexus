# Script para testar linked list diretamente
$ErrorActionPreference = "Stop"

$baseUrl = "http://localhost:15474/cypher"

Write-Host "=== Test: Direct Linked List Verification ===" -ForegroundColor Cyan

# Limpar
Write-Host "`n1. Limpando..." -ForegroundColor Yellow
$clearQuery = "MATCH (n) DETACH DELETE n RETURN count(n) AS deleted"
$clearBody = @{ query = $clearQuery } | ConvertTo-Json -Depth 10
try {
    $clearResponse = Invoke-RestMethod -Uri $baseUrl -Method POST -Headers @{ 'Content-Type' = 'application/json' } -Body $clearBody
    Write-Host "   Deleted: $($clearResponse.rows[0][0]) nodes" -ForegroundColor Green
} catch {
    Write-Host "   Error: $_" -ForegroundColor Red
    exit 1
}

Start-Sleep -Seconds 1

# Criar node
Write-Host "`n2. Criando node Alice..." -ForegroundColor Yellow
$createNodeQuery = "CREATE (a:Person {name: 'Alice'}) RETURN id(a) AS alice_id"
$createNodeBody = @{ query = $createNodeQuery } | ConvertTo-Json -Depth 10
try {
    $createNodeResponse = Invoke-RestMethod -Uri $baseUrl -Method POST -Headers @{ 'Content-Type' = 'application/json' } -Body $createNodeBody
    $aliceId = $createNodeResponse.rows[0][0]
    Write-Host "   Alice node_id: $aliceId" -ForegroundColor Green
} catch {
    Write-Host "   Error: $_" -ForegroundColor Red
    exit 1
}

Start-Sleep -Seconds 1

# Criar primeiro relationship
Write-Host "`n3. Criando primeiro relationship (rel_id deve ser 0)..." -ForegroundColor Yellow
$createRel1Query = "MATCH (a:Person {name: 'Alice'}), (c:Company {name: 'Acme'}) CREATE (a)-[:WORKS_AT {since: 2020}]->(c) RETURN count(*) AS created"
$createRel1Body = @{ query = $createRel1Query } | ConvertTo-Json -Depth 10
try {
    $createRel1Response = Invoke-RestMethod -Uri $baseUrl -Method POST -Headers @{ 'Content-Type' = 'application/json' } -Body $createRel1Body
    Write-Host "   First relationship created" -ForegroundColor Green
} catch {
    Write-Host "   Error: $_" -ForegroundColor Red
    exit 1
}

Start-Sleep -Seconds 1

# Verificar count após primeiro
Write-Host "`n4. Verificando count após primeiro relationship..." -ForegroundColor Yellow
$check1Query = "MATCH (a:Person {name: 'Alice'})-[r:WORKS_AT]->() RETURN count(r) AS count"
$check1Body = @{ query = $check1Query } | ConvertTo-Json -Depth 10
try {
    $check1Response = Invoke-RestMethod -Uri $baseUrl -Method POST -Headers @{ 'Content-Type' = 'application/json' } -Body $check1Body
    $count1 = $check1Response.rows[0][0]
    Write-Host "   Count: $count1 (expected: 1)" -ForegroundColor $(if ($count1 -eq 1) { "Green" } else { "Red" })
} catch {
    Write-Host "   Error: $_" -ForegroundColor Red
}

Start-Sleep -Seconds 1

# Criar segundo relationship
Write-Host "`n5. Criando segundo relationship (rel_id deve ser 1)..." -ForegroundColor Yellow
$createRel2Query = "MATCH (a:Person {name: 'Alice'}), (c:Company {name: 'TechCorp'}) CREATE (a)-[:WORKS_AT {since: 2022}]->(c) RETURN count(*) AS created"
$createRel2Body = @{ query = $createRel2Query } | ConvertTo-Json -Depth 10
try {
    $createRel2Response = Invoke-RestMethod -Uri $baseUrl -Method POST -Headers @{ 'Content-Type' = 'application/json' } -Body $createRel2Body
    Write-Host "   Second relationship created" -ForegroundColor Green
} catch {
    Write-Host "   Error: $_" -ForegroundColor Red
    exit 1
}

Start-Sleep -Seconds 1

# Verificar count após segundo
Write-Host "`n6. Verificando count após segundo relationship..." -ForegroundColor Yellow
$check2Query = "MATCH (a:Person {name: 'Alice'})-[r:WORKS_AT]->() RETURN count(r) AS count"
$check2Body = @{ query = $check2Query } | ConvertTo-Json -Depth 10
try {
    $check2Response = Invoke-RestMethod -Uri $baseUrl -Method POST -Headers @{ 'Content-Type' = 'application/json' } -Body $check2Body
    $count2 = $check2Response.rows[0][0]
    Write-Host "   Count: $count2 (expected: 2)" -ForegroundColor $(if ($count2 -eq 2) { "Green" } else { "Red" })
} catch {
    Write-Host "   Error: $_" -ForegroundColor Red
}

Write-Host "`n=== Teste concluido - verifique logs em /tmp/nexus-server-debug.log ===" -ForegroundColor Cyan

