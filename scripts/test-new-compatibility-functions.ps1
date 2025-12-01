#!/usr/bin/env pwsh
# Script de teste de compatibilidade para novas funções Neo4j
# Testa todas as 42 novas funções implementadas

Write-Host "========================================" -ForegroundColor Cyan
Write-Host "Teste de Compatibilidade - Novas Funções" -ForegroundColor Cyan
Write-Host "========================================" -ForegroundColor Cyan
Write-Host ""

$serverUrl = "http://localhost:15474"
$passed = 0
$failed = 0
$skipped = 0

# Função auxiliar para executar query Cypher
function Invoke-CypherQuery {
    param(
        [string]$Query,
        [string]$Description
    )

    try {
        $body = @{
            query = $Query
        } | ConvertTo-Json

        $response = Invoke-RestMethod -Uri "$serverUrl/cypher" -Method Post -Body $body -ContentType "application/json" -ErrorAction Stop

        return @{
            Success = $true
            Result = $response
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

# Função para verificar se o servidor está rodando
Write-Host "Verificando servidor Nexus..." -ForegroundColor Yellow
try {
    $health = Invoke-RestMethod -Uri "$serverUrl/health" -Method Get -ErrorAction Stop
    Write-Host "✓ Servidor está rodando" -ForegroundColor Green
    Write-Host ""
}
catch {
    Write-Host "✗ Servidor não está respondendo em $serverUrl" -ForegroundColor Red
    Write-Host "Por favor, inicie o servidor com: ./target/release/nexus-server" -ForegroundColor Yellow
    exit 1
}

# ============================================================================
# FUNÇÕES TEMPORAIS - Extração de Componentes
# ============================================================================

Write-Host "========================================" -ForegroundColor Cyan
Write-Host "1. FUNÇÕES TEMPORAIS - Extração de Componentes" -ForegroundColor Cyan
Write-Host "========================================" -ForegroundColor Cyan

$temporalTests = @(
    @{
        Name = 'year()'
        Query = "RETURN year(date('2025-03-15')) AS result"
        Expected = 2025
    },
    @{
        Name = 'month()'
        Query = "RETURN month(date('2025-03-15')) AS result"
        Expected = 3
    },
    @{
        Name = 'day()'
        Query = "RETURN day(date('2025-03-15')) AS result"
        Expected = 15
    },
    @{
        Name = 'hour()'
        Query = "RETURN hour(datetime('2025-03-15T14:30:45Z')) AS result"
        Expected = 14
    },
    @{
        Name = 'minute()'
        Query = "RETURN minute(datetime('2025-03-15T14:30:45Z')) AS result"
        Expected = 30
    },
    @{
        Name = 'second()'
        Query = "RETURN second(datetime('2025-03-15T14:30:45Z')) AS result"
        Expected = 45
    },
    @{
        Name = 'quarter() - Q1'
        Query = "RETURN quarter(date('2025-03-15')) AS result"
        Expected = 1
    },
    @{
        Name = 'quarter() - Q4'
        Query = "RETURN quarter(date('2025-11-15')) AS result"
        Expected = 4
    },
    @{
        Name = 'week()'
        Query = "RETURN week(date('2025-03-15')) AS result"
        ExpectedRange = @(1, 53)
    },
    @{
        Name = 'dayOfWeek()'
        Query = "RETURN dayOfWeek(date('2025-03-15')) AS result"
        ExpectedRange = @(1, 7)
    },
    @{
        Name = 'dayOfYear()'
        Query = "RETURN dayOfYear(date('2025-03-15')) AS result"
        Expected = 74
    }
)

foreach ($test in $temporalTests) {
    $result = Invoke-CypherQuery -Query $test.Query -Description $test.Name

    if ($result.Success) {
        $value = $result.Result.rows[0][0]

        if ($test.Expected -ne $null) {
            if ($value -eq $test.Expected) {
                Write-Host "  ✓ $($test.Name): $value" -ForegroundColor Green
                $passed++
            }
            else {
                Write-Host "  ✗ $($test.Name): esperado $($test.Expected), obtido $value" -ForegroundColor Red
                $failed++
            }
        }
        elseif ($test.ExpectedRange -ne $null) {
            if ($value -ge $test.ExpectedRange[0] -and $value -le $test.ExpectedRange[1]) {
                Write-Host "  ✓ $($test.Name): $value (range válido)" -ForegroundColor Green
                $passed++
            }
            else {
                Write-Host "  ✗ $($test.Name): esperado entre $($test.ExpectedRange[0])-$($test.ExpectedRange[1]), obtido $value" -ForegroundColor Red
                $failed++
            }
        }
    }
    else {
        Write-Host "  ✗ $($test.Name): ERRO - $($result.Error)" -ForegroundColor Red
        $failed++
    }
}

Write-Host ""

# ============================================================================
# FUNÇÕES TEMPORAIS AVANÇADAS
# ============================================================================

Write-Host "========================================" -ForegroundColor Cyan
Write-Host "2. FUNÇÕES TEMPORAIS AVANÇADAS" -ForegroundColor Cyan
Write-Host "========================================" -ForegroundColor Cyan

$advancedTemporalTests = @(
    @{
        Name = 'localtime() - atual"
        Query = "RETURN localtime() AS result"
        CheckPattern = "^\d{2}:\d{2}:\d{2}$"
    },
    @{
        Name = 'localtime() - from string"
        Query = "RETURN localtime('14:30:45') AS result"
        Expected = "14:30:45"
    },
    @{
        Name = 'localdatetime() - atual"
        Query = "RETURN localdatetime() AS result"
        CheckPattern = "^\d{4}-\d{2}-\d{2}T\d{2}:\d{2}:\d{2}$"
    },
    @{
        Name = 'localdatetime() - from string"
        Query = "RETURN localdatetime('2025-03-15T14:30:45') AS result"
        Expected = "2025-03-15T14:30:45"
    }
)

foreach ($test in $advancedTemporalTests) {
    $result = Invoke-CypherQuery -Query $test.Query -Description $test.Name

    if ($result.Success) {
        $value = $result.Result.rows[0][0]

        if ($test.Expected -ne $null) {
            if ($value -eq $test.Expected) {
                Write-Host "  ✓ $($test.Name): $value" -ForegroundColor Green
                $passed++
            }
            else {
                Write-Host "  ✗ $($test.Name): esperado $($test.Expected), obtido $value" -ForegroundColor Red
                $failed++
            }
        }
        elseif ($test.CheckPattern -ne $null) {
            if ($value -match $test.CheckPattern) {
                Write-Host "  ✓ $($test.Name): $value (formato válido)" -ForegroundColor Green
                $passed++
            }
            else {
                Write-Host "  ✗ $($test.Name): formato inválido: $value" -ForegroundColor Red
                $failed++
            }
        }
    }
    else {
        Write-Host "  ✗ $($test.Name): ERRO - $($result.Error)" -ForegroundColor Red
        $failed++
    }
}

Write-Host ""

# ============================================================================
# FUNÇÕES DE STRING AVANÇADAS
# ============================================================================

Write-Host "========================================" -ForegroundColor Cyan
Write-Host "3. FUNÇÕES DE STRING AVANÇADAS" -ForegroundColor Cyan
Write-Host "========================================" -ForegroundColor Cyan

$stringTests = @(
    @{
        Name = 'left()"
        Query = "RETURN left('Hello World', 5) AS result"
        Expected = "Hello"
    },
    @{
        Name = 'right()"
        Query = "RETURN right('Hello World', 5) AS result"
        Expected = "World"
    },
    @{
        Name = 'left() - length > string"
        Query = "RETURN left('Hi', 10) AS result"
        Expected = "Hi"
    },
    @{
        Name = 'right() - length > string"
        Query = "RETURN right('Hi', 10) AS result"
        Expected = "Hi"
    }
)

foreach ($test in $stringTests) {
    $result = Invoke-CypherQuery -Query $test.Query -Description $test.Name

    if ($result.Success) {
        $value = $result.Result.rows[0][0]

        if ($value -eq $test.Expected) {
            Write-Host "  ✓ $($test.Name): '$value'" -ForegroundColor Green
            $passed++
        }
        else {
            Write-Host "  ✗ $($test.Name): esperado '$($test.Expected)', obtido '$value'" -ForegroundColor Red
            $failed++
        }
    }
    else {
        Write-Host "  ✗ $($test.Name): ERRO - $($result.Error)" -ForegroundColor Red
        $failed++
    }
}

Write-Host ""

# ============================================================================
# FUNÇÕES DE LISTA
# ============================================================================

Write-Host "========================================" -ForegroundColor Cyan
Write-Host "4. FUNÇÕES DE LISTA" -ForegroundColor Cyan
Write-Host "========================================" -ForegroundColor Cyan

$listTests = @(
    @{
        Name = 'flatten()"
        Query = "RETURN flatten([[1, 2], [3, 4], [5]]) AS result"
        ExpectedLength = 5
        CheckFirst = 1
    },
    @{
        Name = 'flatten() - mixed"
        Query = "RETURN flatten([[1, 2], 3, [4, 5]]) AS result"
        ExpectedLength = 5
    },
    @{
        Name = 'zip()"
        Query = "RETURN zip([1, 2, 3], ['a', 'b', 'c']) AS result"
        ExpectedLength = 3
        CheckType = "array of arrays"
    }
)

foreach ($test in $listTests) {
    $result = Invoke-CypherQuery -Query $test.Query -Description $test.Name

    if ($result.Success) {
        $value = $result.Result.rows[0][0]

        if ($test.ExpectedLength -ne $null) {
            if ($value.Count -eq $test.ExpectedLength) {
                Write-Host "  ✓ $($test.Name): length = $($value.Count)" -ForegroundColor Green
                $passed++
            }
            else {
                Write-Host "  ✗ $($test.Name): esperado length $($test.ExpectedLength), obtido $($value.Count)" -ForegroundColor Red
                $failed++
            }
        }
    }
    else {
        Write-Host "  ✗ $($test.Name): ERRO - $($result.Error)" -ForegroundColor Red
        $failed++
    }
}

Write-Host ""

# ============================================================================
# FUNÇÕES MATEMÁTICAS
# ============================================================================

Write-Host "========================================" -ForegroundColor Cyan
Write-Host "5. FUNÇÕES MATEMÁTICAS" -ForegroundColor Cyan
Write-Host "========================================" -ForegroundColor Cyan

$mathTests = @(
    @{
        Name = 'pi()"
        Query = "RETURN pi() AS result"
        ExpectedApprox = [Math]::PI
        Tolerance = 0.0001
    },
    @{
        Name = 'e()"
        Query = "RETURN e() AS result"
        ExpectedApprox = [Math]::E
        Tolerance = 0.0001
    },
    @{
        Name = 'radians(180)"
        Query = "RETURN radians(180) AS result"
        ExpectedApprox = [Math]::PI
        Tolerance = 0.0001
    },
    @{
        Name = 'degrees(3.14159)"
        Query = "RETURN degrees(3.14159265359) AS result"
        ExpectedApprox = 180
        Tolerance = 0.01
    },
    @{
        Name = 'log10(100)"
        Query = "RETURN log10(100) AS result"
        ExpectedApprox = 2
        Tolerance = 0.0001
    },
    @{
        Name = 'log(e)"
        Query = "RETURN log(2.71828182846) AS result"
        ExpectedApprox = 1
        Tolerance = 0.0001
    },
    @{
        Name = 'exp(1)"
        Query = "RETURN exp(1) AS result"
        ExpectedApprox = [Math]::E
        Tolerance = 0.0001
    },
    @{
        Name = 'asin(0.5)"
        Query = "RETURN asin(0.5) AS result"
        ExpectedApprox = 0.5236
        Tolerance = 0.001
    },
    @{
        Name = 'acos(0.5)"
        Query = "RETURN acos(0.5) AS result"
        ExpectedApprox = 1.0472
        Tolerance = 0.001
    },
    @{
        Name = 'atan(1)"
        Query = "RETURN atan(1) AS result"
        ExpectedApprox = [Math]::PI / 4
        Tolerance = 0.0001
    },
    @{
        Name = 'atan2(1, 1)"
        Query = "RETURN atan2(1, 1) AS result"
        ExpectedApprox = [Math]::PI / 4
        Tolerance = 0.0001
    }
)

foreach ($test in $mathTests) {
    $result = Invoke-CypherQuery -Query $test.Query -Description $test.Name

    if ($result.Success) {
        $value = $result.Result.rows[0][0]

        if ($test.ExpectedApprox -ne $null) {
            $diff = [Math]::Abs($value - $test.ExpectedApprox)
            if ($diff -lt $test.Tolerance) {
                Write-Host "  ✓ $($test.Name): $value" -ForegroundColor Green
                $passed++
            }
            else {
                Write-Host "  ✗ $($test.Name): esperado ~$($test.ExpectedApprox), obtido $value (diff: $diff)" -ForegroundColor Red
                $failed++
            }
        }
    }
    else {
        Write-Host "  ✗ $($test.Name): ERRO - $($result.Error)" -ForegroundColor Red
        $failed++
    }
}

Write-Host ""

# ============================================================================
# FUNÇÕES DE DURAÇÃO
# ============================================================================

Write-Host "========================================" -ForegroundColor Cyan
Write-Host "6. FUNÇÕES DE DURAÇÃO" -ForegroundColor Cyan
Write-Host "========================================" -ForegroundColor Cyan

$durationTests = @(
    @{
        Name = 'years(duration)"
        Query = "RETURN years(duration({years: 5, months: 3})) AS result"
        Expected = 5
    },
    @{
        Name = 'months(duration)"
        Query = "RETURN months(duration({years: 5, months: 3})) AS result"
        Expected = 3
    },
    @{
        Name = 'days(duration)"
        Query = "RETURN days(duration({days: 10, hours: 5})) AS result"
        Expected = 10
    },
    @{
        Name = 'hours(duration)"
        Query = "RETURN hours(duration({hours: 12, minutes: 30})) AS result"
        Expected = 12
    },
    @{
        Name = 'minutes(duration)"
        Query = "RETURN minutes(duration({hours: 12, minutes: 30})) AS result"
        Expected = 30
    },
    @{
        Name = 'seconds(duration)"
        Query = "RETURN seconds(duration({minutes: 5, seconds: 45})) AS result"
        Expected = 45
    }
)

foreach ($test in $durationTests) {
    $result = Invoke-CypherQuery -Query $test.Query -Description $test.Name

    if ($result.Success) {
        $value = $result.Result.rows[0][0]

        if ($value -eq $test.Expected) {
            Write-Host "  ✓ $($test.Name): $value" -ForegroundColor Green
            $passed++
        }
        else {
            Write-Host "  ✗ $($test.Name): esperado $($test.Expected), obtido $value" -ForegroundColor Red
            $failed++
        }
    }
    else {
        Write-Host "  ✗ $($test.Name): ERRO - $($result.Error)" -ForegroundColor Red
        $failed++
    }
}

Write-Host ""

# ============================================================================
# TESTE DE NULL HANDLING
# ============================================================================

Write-Host "========================================" -ForegroundColor Cyan
Write-Host "7. TESTE DE NULL HANDLING" -ForegroundColor Cyan
Write-Host "========================================" -ForegroundColor Cyan

$nullTests = @(
    "RETURN year(null) AS result",
    "RETURN left(null, 5) AS result",
    "RETURN asin(null) AS result",
    "RETURN pi() AS result",  # Should NOT be null
    "RETURN years(null) AS result",
    "RETURN localtime(null) AS result"
)

$nullTestsExpectNull = @($true, $true, $true, $false, $true, $true)

for ($i = 0; $i -lt $nullTests.Count; $i++) {
    $result = Invoke-CypherQuery -Query $nullTests[$i] -Description "NULL test $i"

    if ($result.Success) {
        $value = $result.Result.rows[0][0]
        $isNull = $value -eq $null

        if ($isNull -eq $nullTestsExpectNull[$i]) {
            Write-Host "  ✓ NULL test $($i+1): correto (null=$isNull)" -ForegroundColor Green
            $passed++
        }
        else {
            Write-Host "  ✗ NULL test $($i+1): esperado null=$($nullTestsExpectNull[$i]), obtido null=$isNull" -ForegroundColor Red
            $failed++
        }
    }
    else {
        Write-Host "  ✗ NULL test $($i+1): ERRO - $($result.Error)" -ForegroundColor Red
        $failed++
    }
}

Write-Host ""

# ============================================================================
# RESUMO
# ============================================================================

Write-Host "========================================" -ForegroundColor Cyan
Write-Host "RESUMO DOS TESTES" -ForegroundColor Cyan
Write-Host "========================================" -ForegroundColor Cyan

$total = $passed + $failed + $skipped

Write-Host "Total de testes: $total" -ForegroundColor White
Write-Host "  ✓ Passou: $passed" -ForegroundColor Green
if ($failed -gt 0) {
    Write-Host "  ✗ Falhou: $failed" -ForegroundColor Red
}
if ($skipped -gt 0) {
    Write-Host "  ⊘ Pulado: $skipped" -ForegroundColor Yellow
}

Write-Host ""

if ($failed -eq 0) {
    Write-Host "SUCESSO! Todos os testes passaram! 🎉" -ForegroundColor Green
    exit 0
}
else {
    $percentage = [math]::Round(($passed / $total) * 100, 2)
    Write-Host "ATENÇÃO: $failed teste(s) falharam ($percentage% de sucesso)" -ForegroundColor Yellow
    exit 1
}
