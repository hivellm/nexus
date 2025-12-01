#!/usr/bin/env pwsh
# Script de comparação de compatibilidade entre Neo4j e Nexus
# Executa as mesmas queries em ambos os bancos e compara os resultados

param(
    [string]$NexusUrl = "http://localhost:15474",
    [string]$Neo4jUrl = "http://localhost:7474",
    [string]$Neo4jUser = "neo4j",
    [string]$Neo4jPassword = "password"
)

Write-Host "========================================" -ForegroundColor Cyan
Write-Host "Teste de Comparação Neo4j vs Nexus" -ForegroundColor Cyan
Write-Host "========================================" -ForegroundColor Cyan
Write-Host ""

$passed = 0
$failed = 0
$skipped = 0

# Função para executar query no Nexus
function Invoke-NexusQuery {
    param([string]$Query)

    try {
        $body = @{ query = $Query } | ConvertTo-Json
        $response = Invoke-RestMethod -Uri "$NexusUrl/cypher" -Method Post -Body $body -ContentType "application/json" -ErrorAction Stop
        return @{
            Success = $true
            Result = $response.rows[0][0]
            Error = $null
        }
    }
    catch {
        return @{
            Success = $false
            Result = $null
            Error = $_.Exception.Message
        }
    }
}

# Função para executar query no Neo4j
function Invoke-Neo4jQuery {
    param([string]$Query)

    try {
        $auth = [Convert]::ToBase64String([Text.Encoding]::ASCII.GetBytes("${Neo4jUser}:${Neo4jPassword}"))
        $headers = @{
            "Authorization" = "Basic $auth"
            "Content-Type" = "application/json"
        }

        $body = @{
            statements = @(
                @{
                    statement = $Query
                }
            )
        } | ConvertTo-Json -Depth 10

        $response = Invoke-RestMethod -Uri "$Neo4jUrl/db/neo4j/tx/commit" -Method Post -Headers $headers -Body $body -ErrorAction Stop

        if ($response.results.Count -gt 0 -and $response.results[0].data.Count -gt 0) {
            return @{
                Success = $true
                Result = $response.results[0].data[0].row[0]
                Error = $null
            }
        }
        else {
            return @{
                Success = $true
                Result = $null
                Error = $null
            }
        }
    }
    catch {
        return @{
            Success = $false
            Result = $null
            Error = $_.Exception.Message
        }
    }
}

# Verificar conectividade
Write-Host "Verificando conectividade..." -ForegroundColor Yellow

try {
    $nexusHealth = Invoke-RestMethod -Uri "$NexusUrl/health" -Method Get -ErrorAction Stop
    Write-Host "  ✓ Nexus está rodando em $NexusUrl" -ForegroundColor Green
}
catch {
    Write-Host "  ✗ Nexus não está acessível em $NexusUrl" -ForegroundColor Red
    Write-Host "    Inicie com: ./target/release/nexus-server" -ForegroundColor Yellow
    $skipped = 999
}

# Tentar Neo4j (opcional)
$neo4jAvailable = $false
try {
    $auth = [Convert]::ToBase64String([Text.Encoding]::ASCII.GetBytes("${Neo4jUser}:${Neo4jPassword}"))
    $headers = @{ "Authorization" = "Basic $auth" }
    $neo4jTest = Invoke-RestMethod -Uri "$Neo4jUrl/db/neo4j/tx/commit" -Method Post -Headers $headers -ErrorAction Stop
    Write-Host "  ✓ Neo4j está rodando em $Neo4jUrl" -ForegroundColor Green
    $neo4jAvailable = $true
}
catch {
    Write-Host "  ⊘ Neo4j não está acessível em $Neo4jUrl (testes de comparação serão pulados)" -ForegroundColor Yellow
}

Write-Host ""

# Testes de compatibilidade
$comparisonTests = @(
    @{
        Category = "Funções Temporais"
        Tests = @(
            @{ Name = "year()"; Query = "RETURN year(date('2025-03-15')) AS result" },
            @{ Name = "month()"; Query = "RETURN month(date('2025-03-15')) AS result" },
            @{ Name = "day()"; Query = "RETURN day(date('2025-03-15')) AS result" },
            @{ Name = "quarter()"; Query = "RETURN quarter(date('2025-03-15')) AS result" },
            @{ Name = "dayOfYear()"; Query = "RETURN dayOfYear(date('2025-03-15')) AS result" }
        )
    },
    @{
        Category = "Funções de String"
        Tests = @(
            @{ Name = "left()"; Query = "RETURN left('Hello World', 5) AS result" },
            @{ Name = "right()"; Query = "RETURN right('Hello World', 5) AS result" }
        )
    },
    @{
        Category = "Funções Matemáticas"
        Tests = @(
            @{ Name = "pi()"; Query = "RETURN pi() AS result"; Tolerance = 0.0001 },
            @{ Name = "e()"; Query = "RETURN e() AS result"; Tolerance = 0.0001 },
            @{ Name = "radians(180)"; Query = "RETURN radians(180) AS result"; Tolerance = 0.0001 },
            @{ Name = "degrees(pi)"; Query = "RETURN degrees(3.14159265359) AS result"; Tolerance = 0.01 },
            @{ Name = "log10(100)"; Query = "RETURN log10(100) AS result"; Tolerance = 0.0001 },
            @{ Name = "exp(1)"; Query = "RETURN exp(1) AS result"; Tolerance = 0.0001 },
            @{ Name = "asin(0.5)"; Query = "RETURN asin(0.5) AS result"; Tolerance = 0.001 },
            @{ Name = "acos(0.5)"; Query = "RETURN acos(0.5) AS result"; Tolerance = 0.001 },
            @{ Name = "atan(1)"; Query = "RETURN atan(1) AS result"; Tolerance = 0.0001 }
        )
    },
    @{
        Category = "Funções de Lista"
        Tests = @(
            @{ Name = "flatten()"; Query = "RETURN size(flatten([[1, 2], [3, 4], [5]])) AS result" }
        )
    }
)

foreach ($category in $comparisonTests) {
    Write-Host "========================================" -ForegroundColor Cyan
    Write-Host $category.Category -ForegroundColor Cyan
    Write-Host "========================================" -ForegroundColor Cyan

    foreach ($test in $category.Tests) {
        $nexusResult = Invoke-NexusQuery -Query $test.Query

        if ($neo4jAvailable) {
            $neo4jResult = Invoke-Neo4jQuery -Query $test.Query

            if ($nexusResult.Success -and $neo4jResult.Success) {
                $match = $false

                if ($test.Tolerance -ne $null) {
                    # Comparação numérica com tolerância
                    $diff = [Math]::Abs([double]$nexusResult.Result - [double]$neo4jResult.Result)
                    $match = $diff -lt $test.Tolerance
                }
                else {
                    # Comparação exata
                    $match = $nexusResult.Result -eq $neo4jResult.Result
                }

                if ($match) {
                    Write-Host "  ✓ $($test.Name): Nexus=$($nexusResult.Result), Neo4j=$($neo4jResult.Result)" -ForegroundColor Green
                    $passed++
                }
                else {
                    Write-Host "  ✗ $($test.Name): DIVERGÊNCIA - Nexus=$($nexusResult.Result), Neo4j=$($neo4jResult.Result)" -ForegroundColor Red
                    $failed++
                }
            }
            elseif (!$nexusResult.Success) {
                Write-Host "  ✗ $($test.Name): ERRO no Nexus - $($nexusResult.Error)" -ForegroundColor Red
                $failed++
            }
            elseif (!$neo4jResult.Success) {
                Write-Host "  ⊘ $($test.Name): ERRO no Neo4j (função pode não estar disponível)" -ForegroundColor Yellow
                $skipped++
            }
        }
        else {
            # Apenas testar se o Nexus funciona
            if ($nexusResult.Success) {
                Write-Host "  ✓ $($test.Name): $($nexusResult.Result) (Neo4j não disponível para comparação)" -ForegroundColor Green
                $passed++
            }
            else {
                Write-Host "  ✗ $($test.Name): ERRO - $($nexusResult.Error)" -ForegroundColor Red
                $failed++
            }
        }
    }

    Write-Host ""
}

# ============================================================================
# RESUMO
# ============================================================================

Write-Host "========================================" -ForegroundColor Cyan
Write-Host "RESUMO" -ForegroundColor Cyan
Write-Host "========================================" -ForegroundColor Cyan

$total = $passed + $failed + $skipped

Write-Host "Total de testes: $total" -ForegroundColor White
Write-Host "  ✓ Compatíveis: $passed" -ForegroundColor Green
if ($failed -gt 0) {
    Write-Host "  ✗ Divergências: $failed" -ForegroundColor Red
}
if ($skipped -gt 0) {
    Write-Host "  ⊘ Pulados: $skipped" -ForegroundColor Yellow
}

Write-Host ""

if ($neo4jAvailable) {
    if ($failed -eq 0) {
        Write-Host "SUCESSO! 100% de compatibilidade com Neo4j! 🎉" -ForegroundColor Green
        exit 0
    }
    else {
        $percentage = [math]::Round(($passed / ($passed + $failed)) * 100, 2)
        Write-Host "Taxa de compatibilidade: $percentage%" -ForegroundColor Yellow
        exit 1
    }
}
else {
    Write-Host "Testes executados apenas no Nexus (Neo4j não disponível para comparação)" -ForegroundColor Yellow
    if ($failed -eq 0) {
        Write-Host "SUCESSO! Todas as funções funcionam no Nexus! 🎉" -ForegroundColor Green
        exit 0
    }
    else {
        exit 1
    }
}
