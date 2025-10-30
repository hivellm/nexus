# Script para testar todas as rotas REST do Nexus Server
# Uso: .\test-all-routes.ps1

$baseUrl = "http://127.0.0.1:15474"
$results = @()

Write-Host "=========================================" -ForegroundColor Cyan
Write-Host "Nexus Server REST API Test Suite" -ForegroundColor Cyan
Write-Host "=========================================" -ForegroundColor Cyan
Write-Host ""

function Test-Route {
    param(
        [string]$Method,
        [string]$Path,
        [object]$Body = $null,
        [string]$Description
    )
    
    $url = "$baseUrl$Path"
    $headers = @{
        "Content-Type" = "application/json"
    }
    
    try {
        Write-Host "[TEST] $Method $Path" -ForegroundColor Yellow
        Write-Host "  Desc: $Description" -ForegroundColor Gray
        
        if ($Body) {
            $jsonBody = $Body | ConvertTo-Json -Depth 10
            $response = Invoke-RestMethod -Uri $url -Method $Method -Headers $headers -Body $jsonBody -ErrorAction Stop
        } else {
            $response = Invoke-RestMethod -Uri $url -Method $Method -Headers $headers -ErrorAction Stop
        }
        
        $status = "✅ PASS"
        Write-Host "  Status: $status" -ForegroundColor Green
        Write-Host "  Response: $(($response | ConvertTo-Json -Compress -Depth 2))" -ForegroundColor Gray
        Write-Host ""
        
        $results += @{
            Method = $Method
            Path = $Path
            Status = "PASS"
            Description = $Description
        }
        
        return $response
    } catch {
        $statusCode = $_.Exception.Response.StatusCode.value__
        $status = "❌ FAIL ($statusCode)"
        
        if ($statusCode -eq 404 -or $statusCode -eq 400 -or $statusCode -eq 422) {
            # Alguns erros esperados são OK (endpoints que precisam de dados)
            Write-Host "  Status: $status (Expected for empty/data requirements)" -ForegroundColor Yellow
        } else {
            Write-Host "  Status: $status" -ForegroundColor Red
            Write-Host "  Error: $($_.Exception.Message)" -ForegroundColor Red
        }
        Write-Host ""
        
        $results += @{
            Method = $Method
            Path = $Path
            Status = "FAIL ($statusCode)"
            Description = $Description
        }
        
        return $null
    }
}

# Verifica se o servidor está rodando
Write-Host "Verificando se o servidor está rodando..." -ForegroundColor Cyan
try {
    $health = Invoke-RestMethod -Uri "$baseUrl/health" -Method GET -ErrorAction Stop
    Write-Host "✅ Servidor está rodando!" -ForegroundColor Green
    Write-Host ""
} catch {
    Write-Host "❌ Servidor não está rodando em $baseUrl" -ForegroundColor Red
    Write-Host "Por favor, inicie o servidor primeiro com: cargo run --release" -ForegroundColor Yellow
    exit 1
}

Write-Host "=========================================" -ForegroundColor Cyan
Write-Host "Iniciando testes de rotas..." -ForegroundColor Cyan
Write-Host "=========================================" -ForegroundColor Cyan
Write-Host ""

# 1. Health & Metrics
Test-Route -Method GET -Path "/health" -Description "Health check endpoint"
Test-Route -Method GET -Path "/" -Description "Root health check"
Test-Route -Method GET -Path "/metrics" -Description "Metrics endpoint"

# 2. Schema Management
Test-Route -Method GET -Path "/schema/labels" -Description "List labels"
Test-Route -Method POST -Path "/schema/labels" -Body @{name="TestLabel"} -Description "Create label"
Test-Route -Method GET -Path "/schema/rel_types" -Description "List relationship types"
Test-Route -Method POST -Path "/schema/rel_types" -Body @{name="KNOWS"} -Description "Create relationship type"

# 3. Data Management - Nodes
$nodeId = $null
$createNodeResponse = Test-Route -Method POST -Path "/data/nodes" -Body @{
    labels = @("Person")
    properties = @{
        name = "Alice"
        age = 30
    }
} -Description "Create node"
if ($createNodeResponse -and $createNodeResponse.node_id) {
    $nodeId = $createNodeResponse.node_id
    Write-Host "  Created node with ID: $nodeId" -ForegroundColor Green
}

if ($nodeId) {
    Test-Route -Method GET -Path "/data/nodes?id=$nodeId" -Description "Get node by ID"
    Test-Route -Method PUT -Path "/data/nodes" -Body @{
        node_id = $nodeId
        properties = @{
            age = 31
        }
    } -Description "Update node"
    Test-Route -Method DELETE -Path "/data/nodes" -Body @{node_id = $nodeId} -Description "Delete node"
} else {
    Write-Host "[SKIP] Node operations skipped (node not created)" -ForegroundColor Yellow
    Write-Host ""
}

# 4. Data Management - Relationships
Test-Route -Method POST -Path "/data/relationships" -Body @{
    source_id = 1
    target_id = 2
    rel_type = "KNOWS"
    properties = @{}
} -Description "Create relationship (may fail if nodes don't exist)"

# 5. Cypher Query
Test-Route -Method POST -Path "/cypher" -Body @{
    query = "MATCH (n) RETURN n LIMIT 10"
} -Description "Execute Cypher query"

Test-Route -Method POST -Path "/cypher" -Body @{
    query = "CREATE (p:Person {name: 'Bob', age: 25}) RETURN p"
} -Description "Execute CREATE Cypher query"

# 6. KNN Traverse
$testVector = (1..128 | ForEach-Object { [double](Get-Random -Minimum 0.0 -Maximum 1.0) })
Test-Route -Method POST -Path "/knn_traverse" -Body @{
    label = "Person"
    vector = $testVector
    k = 5
} -Description "KNN traverse (may fail if no vectors indexed)"

# 7. Statistics
Test-Route -Method GET -Path "/stats" -Description "Get database statistics"

# 8. Graph Comparison
Test-Route -Method GET -Path "/comparison/health" -Description "Comparison service health"
Test-Route -Method POST -Path "/comparison/stats" -Body @{
    graph_id = "A"
} -Description "Get graph statistics"

# 9. Clustering
Test-Route -Method GET -Path "/clustering/algorithms" -Description "Get clustering algorithms"

# 10. Graph Correlation
Test-Route -Method GET -Path "/graph-correlation/types" -Description "Get graph types"
Test-Route -Method POST -Path "/graph-correlation/generate" -Body @{
    graph_type = "call_graph"
    scope = @{
        collections = @("codebase")
        file_patterns = @("*.rs")
    }
} -Description "Generate graph"

# 11. OpenAPI
Test-Route -Method GET -Path "/openapi.json" -Description "Get OpenAPI specification"

# 12. Bulk Ingest
Test-Route -Method POST -Path "/ingest" -Body @{
    nodes = @(
        @{
            labels = @("Person")
            properties = @{
                name = "Charlie"
                age = 35
            }
        }
    )
    relationships = @()
} -Description "Bulk ingest data"

Write-Host "=========================================" -ForegroundColor Cyan
Write-Host "Resumo dos Testes" -ForegroundColor Cyan
Write-Host "=========================================" -ForegroundColor Cyan
Write-Host ""

$passed = ($results | Where-Object { $_.Status -eq "PASS" }).Count
$failed = ($results | Where-Object { $_.Status -like "FAIL*" }).Count
$total = $results.Count

Write-Host "Total de testes: $total" -ForegroundColor White
Write-Host "Passou: $passed" -ForegroundColor Green
Write-Host "Falhou: $failed" -ForegroundColor $(if ($failed -gt 0) { "Red" } else { "Green" })
Write-Host ""

Write-Host "Detalhes:" -ForegroundColor Cyan
$results | Format-Table -AutoSize

