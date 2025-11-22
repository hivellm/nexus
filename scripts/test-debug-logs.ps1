# Script simples para testar relationship creation e ver logs
$ErrorActionPreference = "Stop"

$baseUrl = "http://localhost:15474/cypher"

Write-Host "=== Test: Relationship Creation Debug ===" -ForegroundColor Cyan

# Limpar
Write-Host "`n1. Limpando..." -ForegroundColor Yellow
$clearQuery = "MATCH (n) DETACH DELETE n RETURN count(n) AS deleted"
$clearBody = @{ query = $clearQuery } | ConvertTo-Json -Depth 10
try {
    $clearResponse = Invoke-RestMethod -Uri $baseUrl -Method POST -Headers @{ 'Content-Type' = 'application/json' } -Body $clearBody
    Write-Host "   Deleted: $($clearResponse.rows[0][0]) nodes" -ForegroundColor Green
} catch {
    Write-Host "   Error: $_" -ForegroundColor Red
}

Start-Sleep -Seconds 1

# Criar nodes e relationship
Write-Host "`n2. Criando nodes e relationship..." -ForegroundColor Yellow
$createQuery = "CREATE (a:Person {name: 'Alice'}), (c:Company {name: 'Acme'}) CREATE (a)-[:WORKS_AT {since: 2020}]->(c) RETURN id(a) AS alice_id"
$createBody = @{ query = $createQuery } | ConvertTo-Json -Depth 10
try {
    $createResponse = Invoke-RestMethod -Uri $baseUrl -Method POST -Headers @{ 'Content-Type' = 'application/json' } -Body $createBody
    Write-Host "   Created successfully" -ForegroundColor Green
} catch {
    Write-Host "   Error: $_" -ForegroundColor Red
}

Start-Sleep -Seconds 1

# Verificar count
Write-Host "`n3. Verificando count..." -ForegroundColor Yellow
$checkQuery = "MATCH (a:Person {name: 'Alice'})-[r:WORKS_AT]->() RETURN count(r) AS count"
$checkBody = @{ query = $checkQuery } | ConvertTo-Json -Depth 10
try {
    $checkResponse = Invoke-RestMethod -Uri $baseUrl -Method POST -Headers @{ 'Content-Type' = 'application/json' } -Body $checkBody
    $count = $checkResponse.rows[0][0]
    Write-Host "   Count: $count (expected: 1)" -ForegroundColor $(if ($count -eq 1) { "Green" } else { "Red" })
} catch {
    Write-Host "   Error: $_" -ForegroundColor Red
}

Write-Host "`n=== Teste concluido - verifique logs em /tmp/nexus-server-debug.log ===" -ForegroundColor Cyan

