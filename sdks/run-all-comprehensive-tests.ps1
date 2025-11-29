# Comprehensive SDK Test Runner
Write-Host "=" * 70 -ForegroundColor Cyan
Write-Host "RUNNING COMPREHENSIVE TESTS FOR ALL SDKs" -ForegroundColor Cyan
Write-Host "=" * 70 -ForegroundColor Cyan

$results = @{}

# Test Python SDK
Write-Host "`n[Python SDK]" -ForegroundColor Yellow
try {
    $output = python sdks/python/test_sdk_simple.py 2>&1
    if ($LASTEXITCODE -eq 0) {
        $results["Python"] = "PASS"
        Write-Host "[SUCCESS] Python SDK tests passed" -ForegroundColor Green
    } else {
        $results["Python"] = "FAIL"
        Write-Host "[FAILED] Python SDK tests failed" -ForegroundColor Red
    }
} catch {
    $results["Python"] = "ERROR"
    Write-Host "[ERROR] Python SDK tests error: $_" -ForegroundColor Red
}

# Test TypeScript SDK
Write-Host "`n[TypeScript SDK]" -ForegroundColor Yellow
try {
    Push-Location sdks/typescript
    $output = npx tsx test-sdk-comprehensive.ts 2>&1
    Pop-Location
    if ($LASTEXITCODE -eq 0) {
        $results["TypeScript"] = "PASS"
        Write-Host "[SUCCESS] TypeScript SDK tests passed" -ForegroundColor Green
    } else {
        $results["TypeScript"] = "PARTIAL"
        Write-Host "[PARTIAL] TypeScript SDK tests had some failures" -ForegroundColor Yellow
    }
} catch {
    $results["TypeScript"] = "ERROR"
    Write-Host "[ERROR] TypeScript SDK tests error: $_" -ForegroundColor Red
    Pop-Location
}

# Test Rust SDK
Write-Host "`n[Rust SDK]" -ForegroundColor Yellow
try {
    Push-Location sdks/rust
    $output = cargo run --example test_sdk --quiet 2>&1
    Pop-Location
    if ($output -match "SUCCESS") {
        $results["Rust"] = "PASS"
        Write-Host "[SUCCESS] Rust SDK tests passed" -ForegroundColor Green
    } else {
        $results["Rust"] = "FAIL"
        Write-Host "[FAILED] Rust SDK tests failed" -ForegroundColor Red
    }
} catch {
    $results["Rust"] = "ERROR"
    Write-Host "[ERROR] Rust SDK tests error: $_" -ForegroundColor Red
    Pop-Location
}

# Test Go SDK
Write-Host "`n[Go SDK]" -ForegroundColor Yellow
try {
    Push-Location sdks/go/test
    $output = go run test_sdk.go 2>&1
    Pop-Location
    if ($LASTEXITCODE -eq 0) {
        $results["Go"] = "PASS"
        Write-Host "[SUCCESS] Go SDK tests passed" -ForegroundColor Green
    } else {
        $results["Go"] = "PARTIAL"
        Write-Host "[PARTIAL] Go SDK tests had some failures" -ForegroundColor Yellow
    }
} catch {
    $results["Go"] = "ERROR"
    Write-Host "[ERROR] Go SDK tests error: $_" -ForegroundColor Red
    Pop-Location
}

# Test C# SDK
Write-Host "`n[C# SDK]" -ForegroundColor Yellow
try {
    Push-Location sdks/TestConsoleSimple
    $output = dotnet run --verbosity quiet 2>&1
    Pop-Location
    if ($output -match "SUCCESS") {
        $results["CSharp"] = "PASS"
        Write-Host "[SUCCESS] C# SDK tests passed" -ForegroundColor Green
    } else {
        $results["CSharp"] = "PARTIAL"
        Write-Host "[PARTIAL] C# SDK tests had some failures" -ForegroundColor Yellow
    }
} catch {
    $results["CSharp"] = "ERROR"
    Write-Host "[ERROR] C# SDK tests error: $_" -ForegroundColor Red
    Pop-Location
}

# Test n8n SDK
Write-Host "`n[n8n SDK]" -ForegroundColor Yellow
try {
    Push-Location sdks/n8n
    $output = npx tsx test-integration.ts 2>&1
    Pop-Location
    if ($output -match "SUCCESS") {
        $results["n8n"] = "PASS"
        Write-Host "[SUCCESS] n8n SDK tests passed" -ForegroundColor Green
    } else {
        $results["n8n"] = "FAIL"
        Write-Host "[FAILED] n8n SDK tests failed" -ForegroundColor Red
    }
} catch {
    $results["n8n"] = "ERROR"
    Write-Host "[ERROR] n8n SDK tests error: $_" -ForegroundColor Red
    Pop-Location
}

# Print Summary
Write-Host "`n" + ("=" * 70) -ForegroundColor Cyan
Write-Host "TEST SUMMARY" -ForegroundColor Cyan
Write-Host ("=" * 70) -ForegroundColor Cyan

$passCount = ($results.Values | Where-Object { $_ -eq "PASS" }).Count
$partialCount = ($results.Values | Where-Object { $_ -eq "PARTIAL" }).Count
$failCount = ($results.Values | Where-Object { $_ -eq "FAIL" }).Count
$errorCount = ($results.Values | Where-Object { $_ -eq "ERROR" }).Count

foreach ($sdk in $results.Keys | Sort-Object) {
    $status = $results[$sdk]
    $color = switch ($status) {
        "PASS" { "Green" }
        "PARTIAL" { "Yellow" }
        "FAIL" { "Red" }
        "ERROR" { "DarkRed" }
    }
    Write-Host ("{0,-15} : {1}" -f $sdk, $status) -ForegroundColor $color
}

Write-Host "`n" + ("=" * 70) -ForegroundColor Cyan
Write-Host "PASSED:  $passCount" -ForegroundColor Green
Write-Host "PARTIAL: $partialCount" -ForegroundColor Yellow
Write-Host "FAILED:  $failCount" -ForegroundColor Red
Write-Host "ERRORS:  $errorCount" -ForegroundColor DarkRed
Write-Host ("=" * 70) -ForegroundColor Cyan

if ($passCount + $partialCount -eq $results.Count) {
    Write-Host "`nOVERALL: ALL SDKs OPERATIONAL" -ForegroundColor Green
    exit 0
} else {
    Write-Host "`nOVERALL: SOME SDKs NEED ATTENTION" -ForegroundColor Yellow
    exit 1
}
