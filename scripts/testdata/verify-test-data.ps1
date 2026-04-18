# Script para verificar se os dados de teste estão duplicados no banco
# Verifica tanto Neo4j quanto Nexus

param(
    [string]$Neo4jUri = "http://localhost:7474",
    [string]$Neo4jUser = "neo4j",
    [string]$Neo4jPassword = "password",
    [string]$NexusUri = "http://localhost:15474"
)

Write-Host "================================================================================" -ForegroundColor Cyan
Write-Host "  Verificacao de Dados de Teste no Banco" -ForegroundColor Cyan
Write-Host "================================================================================" -ForegroundColor Cyan
Write-Host ""

# Function to execute query on Neo4j
function Invoke-Neo4jQuery {
    param([string]$Cypher)
    
    $body = @{
        statements = @(
            @{
                statement = $Cypher
                parameters = @{}
            }
        )
    } | ConvertTo-Json -Depth 10
    
    $auth = [Convert]::ToBase64String([Text.Encoding]::ASCII.GetBytes("${Neo4jUser}:${Neo4jPassword}"))
    
    try {
        $response = Invoke-RestMethod -Uri "$Neo4jUri/db/neo4j/tx/commit" `
            -Method POST `
            -Headers @{
                "Authorization" = "Basic $auth"
                "Content-Type" = "application/json"
                "Accept" = "application/json"
            } `
            -Body $body `
            -ErrorAction Stop `
            -TimeoutSec 30
        
        if ($response.errors -and $response.errors.Count -gt 0) {
            return @{ error = $response.errors[0].message }
        }
        
        return $response.results[0]
    }
    catch {
        return @{ error = $_.Exception.Message }
    }
}

# Function to execute query on Nexus
function Invoke-NexusQuery {
    param([string]$Cypher)
    
    $body = @{
        query = $Cypher
        parameters = @{}
    } | ConvertTo-Json -Depth 10
    
    try {
        $response = Invoke-RestMethod -Uri "$NexusUri/cypher" `
            -Method POST `
            -Headers @{
                "Content-Type" = "application/json"
                "Accept" = "application/json"
            } `
            -Body $body `
            -ErrorAction Stop `
            -TimeoutSec 30
        
        return $response
    }
    catch {
        return @{ error = $_.Exception.Message }
    }
}

Write-Host "=== Verificando Neo4j ===" -ForegroundColor Yellow
Write-Host ""

# Verificar total de nodes Person
$query = "MATCH (n:Person) RETURN count(n) AS total"
$result = Invoke-Neo4jQuery -Cypher $query
if ($result.error) {
    Write-Host "ERRO Neo4j: $($result.error)" -ForegroundColor Red
} else {
    $total = $result.data[0][0]
    Write-Host "Total de nodes Person no Neo4j: $total" -ForegroundColor Green
}

# Verificar nodes Person por nome
$query = "MATCH (n:Person) RETURN n.name AS name, count(n) AS count ORDER BY name"
$result = Invoke-Neo4jQuery -Cypher $query
if ($result.error) {
    Write-Host "ERRO Neo4j: $($result.error)" -ForegroundColor Red
} else {
    Write-Host ""
    Write-Host "Nodes Person por nome (Neo4j):" -ForegroundColor Cyan
    foreach ($row in $result.data) {
        $name = $row[0]
        $count = $row[1]
        if ($count -gt 1) {
            Write-Host "  [DUPLICADO] $name : $count nodes" -ForegroundColor Red
        } else {
            Write-Host "  [OK] $name : $count node" -ForegroundColor Green
        }
    }
}

# Verificar nodes Person por propriedades
$query = "MATCH (n:Person {name: 'Alice'}) RETURN n.name AS name, id(n) AS id, n.age AS age, n.city AS city"
$result = Invoke-Neo4jQuery -Cypher $query
if ($result.error) {
    Write-Host "ERRO Neo4j: $($result.error)" -ForegroundColor Red
} else {
    Write-Host ""
    Write-Host "Detalhes dos nodes Alice no Neo4j:" -ForegroundColor Cyan
    foreach ($row in $result.data) {
        $name = $row[0]
        $id = $row[1]
        $age = $row[2]
        $city = $row[3]
        Write-Host "  ID: $id, Name: $name, Age: $age, City: $city" -ForegroundColor White
    }
    if ($result.data.Count -gt 1) {
        Write-Host "  [DUPLICADO] ENCONTRADOS $($result.data.Count) nodes Alice" -ForegroundColor Red
    }
}

$query = "MATCH (n:Person {name: 'Bob'}) RETURN n.name AS name, id(n) AS id, n.age AS age, n.city AS city"
$result = Invoke-Neo4jQuery -Cypher $query
if ($result.error) {
    Write-Host "ERRO Neo4j: $($result.error)" -ForegroundColor Red
} else {
    Write-Host ""
    Write-Host "Detalhes dos nodes Bob no Neo4j:" -ForegroundColor Cyan
    foreach ($row in $result.data) {
        $name = $row[0]
        $id = $row[1]
        $age = $row[2]
        $city = $row[3]
        Write-Host "  ID: $id, Name: $name, Age: $age, City: $city" -ForegroundColor White
    }
    if ($result.data.Count -gt 1) {
        Write-Host "  [DUPLICADO] ENCONTRADOS $($result.data.Count) nodes Bob" -ForegroundColor Red
    }
}

Write-Host ""
Write-Host "=== Verificando Nexus ===" -ForegroundColor Yellow
Write-Host ""

# Verificar total de nodes Person
$query = "MATCH (n:Person) RETURN count(n) AS total"
$result = Invoke-NexusQuery -Cypher $query
if ($result.error) {
    Write-Host "ERRO Nexus: $($result.error)" -ForegroundColor Red
} else {
    if ($result.rows -and $result.rows.Count -gt 0) {
        $total = $result.rows[0][0]
        Write-Host "Total de nodes Person no Nexus: $total" -ForegroundColor Green
    } elseif ($result.data -and $result.data.Count -gt 0) {
        $total = $result.data[0].total
        Write-Host "Total de nodes Person no Nexus: $total" -ForegroundColor Green
    } else {
        Write-Host "Total de nodes Person no Nexus: 0 (nenhum dado)" -ForegroundColor Yellow
    }
}

# Verificar nodes Person por nome
$query = "MATCH (n:Person) RETURN n.name AS name, count(n) AS count ORDER BY name"
$result = Invoke-NexusQuery -Cypher $query
if ($result.error) {
    Write-Host "ERRO Nexus: $($result.error)" -ForegroundColor Red
} else {
    Write-Host ""
    Write-Host "Nodes Person por nome (Nexus):" -ForegroundColor Cyan
    $rows = $null
    if ($result.rows) {
        $rows = $result.rows
    } elseif ($result.data) {
        $rows = $result.data
    }
    if ($rows) {
        foreach ($row in $rows) {
            if ($row -is [array]) {
                $name = $row[0]
                $count = $row[1]
            } elseif ($row.PSObject.Properties.Name -contains 'name') {
                $name = $row.name
                $count = $row.count
            } else {
                continue
            }
            if ($count -gt 1) {
                Write-Host "  [DUPLICADO] $name : $count nodes" -ForegroundColor Red
            } else {
                Write-Host "  [OK] $name : $count node" -ForegroundColor Green
            }
        }
    }
}

# Verificar nodes Person por propriedades
$query = "MATCH (n:Person {name: 'Alice'}) RETURN n.name AS name, id(n) AS id, n.age AS age, n.city AS city"
$result = Invoke-NexusQuery -Cypher $query
if ($result.error) {
    Write-Host "ERRO Nexus: $($result.error)" -ForegroundColor Red
} else {
    Write-Host ""
    Write-Host "Detalhes dos nodes Alice no Nexus:" -ForegroundColor Cyan
    $rows = $null
    if ($result.rows) {
        $rows = $result.rows
    } elseif ($result.data) {
        $rows = $result.data
    }
    if ($rows) {
        foreach ($row in $rows) {
            if ($row -is [array]) {
                $name = $row[0]
                $id = $row[1]
                $age = $row[2]
                $city = $row[3]
            } elseif ($row.PSObject.Properties.Name -contains 'name') {
                $name = $row.name
                $id = $row.id
                $age = $row.age
                $city = $row.city
            } else {
                continue
            }
            Write-Host "  ID: $id, Name: $name, Age: $age, City: $city" -ForegroundColor White
        }
        if ($rows.Count -gt 1) {
            Write-Host "  [DUPLICADO] ENCONTRADOS $($rows.Count) nodes Alice" -ForegroundColor Red
        }
    }
}

$query = "MATCH (n:Person {name: 'Bob'}) RETURN n.name AS name, id(n) AS id, n.age AS age, n.city AS city"
$result = Invoke-NexusQuery -Cypher $query
if ($result.error) {
    Write-Host "ERRO Nexus: $($result.error)" -ForegroundColor Red
} else {
    Write-Host ""
    Write-Host "Detalhes dos nodes Bob no Nexus:" -ForegroundColor Cyan
    $rows = $null
    if ($result.rows) {
        $rows = $result.rows
    } elseif ($result.data) {
        $rows = $result.data
    }
    if ($rows) {
        foreach ($row in $rows) {
            if ($row -is [array]) {
                $name = $row[0]
                $id = $row[1]
                $age = $row[2]
                $city = $row[3]
            } elseif ($row.PSObject.Properties.Name -contains 'name') {
                $name = $row.name
                $id = $row.id
                $age = $row.age
                $city = $row.city
            } else {
                continue
            }
            Write-Host "  ID: $id, Name: $name, Age: $age, City: $city" -ForegroundColor White
        }
        if ($rows.Count -gt 1) {
            Write-Host "  [DUPLICADO] ENCONTRADOS $($rows.Count) nodes Bob" -ForegroundColor Red
        }
    }
}

Write-Host ""
Write-Host "=== Verificacao Completa ===" -ForegroundColor Yellow
Write-Host "Se houver duplicatas acima, o problema esta nos dados de teste, nao no codigo." -ForegroundColor Cyan
Write-Host "Se nao houver duplicatas, o problema esta no codigo de execucao de queries." -ForegroundColor Cyan

