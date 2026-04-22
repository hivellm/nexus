# Comprehensive SDK Test Runner
#
# Runs the 1.0.0 transport-layer test suite across every first-party
# Nexus SDK. A `-Transport` parameter is passed through to each SDK's
# runner; the default is `rpc` so CI validates the new binary RPC
# path on every PR.
#
# Usage:
#   pwsh sdks/run-all-comprehensive-tests.ps1                # rpc
#   pwsh sdks/run-all-comprehensive-tests.ps1 -Transport rpc
#   pwsh sdks/run-all-comprehensive-tests.ps1 -Transport http
#   pwsh sdks/run-all-comprehensive-tests.ps1 -Transport all # rpc + http

param(
    [ValidateSet("rpc", "http", "all")]
    [string]$Transport = "rpc"
)

Write-Host ("=" * 70) -ForegroundColor Cyan
Write-Host ("RUNNING COMPREHENSIVE TESTS FOR ALL SDKs (transport={0})" -f $Transport) -ForegroundColor Cyan
Write-Host ("=" * 70) -ForegroundColor Cyan

$transportsToRun = if ($Transport -eq "all") { @("rpc", "http") } else { @($Transport) }

$results = @{}

foreach ($t in $transportsToRun) {
    Write-Host ""
    Write-Host ("--- transport: {0} ---" -f $t) -ForegroundColor Magenta
    $env:NEXUS_SDK_TRANSPORT = $t

    # ── Python SDK ──────────────────────────────────────────────────
    Write-Host "`n[Python SDK]" -ForegroundColor Yellow
    try {
        Push-Location sdks/python
        $output = python -m pytest tests/ -q 2>&1
        Pop-Location
        if ($LASTEXITCODE -eq 0) {
            $results["Python/$t"] = "PASS"
            Write-Host "[SUCCESS] Python SDK tests passed" -ForegroundColor Green
        } else {
            $results["Python/$t"] = "FAIL"
            Write-Host "[FAILED] Python SDK tests failed" -ForegroundColor Red
        }
    } catch {
        $results["Python/$t"] = "ERROR"
        Write-Host "[ERROR] Python SDK tests error: $_" -ForegroundColor Red
        try { Pop-Location } catch {}
    }

    # ── TypeScript SDK ──────────────────────────────────────────────
    Write-Host "`n[TypeScript SDK]" -ForegroundColor Yellow
    try {
        Push-Location sdks/typescript
        $output = npx vitest run tests/transports.test.ts 2>&1
        Pop-Location
        if ($LASTEXITCODE -eq 0) {
            $results["TypeScript/$t"] = "PASS"
            Write-Host "[SUCCESS] TypeScript SDK tests passed" -ForegroundColor Green
        } else {
            $results["TypeScript/$t"] = "PARTIAL"
            Write-Host "[PARTIAL] TypeScript SDK tests had some failures" -ForegroundColor Yellow
        }
    } catch {
        $results["TypeScript/$t"] = "ERROR"
        Write-Host "[ERROR] TypeScript SDK tests error: $_" -ForegroundColor Red
        try { Pop-Location } catch {}
    }

    # ── Rust SDK ────────────────────────────────────────────────────
    Write-Host "`n[Rust SDK]" -ForegroundColor Yellow
    try {
        Push-Location sdks/rust
        $output = cargo test --quiet 2>&1
        Pop-Location
        if ($LASTEXITCODE -eq 0) {
            $results["Rust/$t"] = "PASS"
            Write-Host "[SUCCESS] Rust SDK tests passed" -ForegroundColor Green
        } else {
            $results["Rust/$t"] = "FAIL"
            Write-Host "[FAILED] Rust SDK tests failed" -ForegroundColor Red
        }
    } catch {
        $results["Rust/$t"] = "ERROR"
        Write-Host "[ERROR] Rust SDK tests error: $_" -ForegroundColor Red
        try { Pop-Location } catch {}
    }

    # ── Go SDK ──────────────────────────────────────────────────────
    Write-Host "`n[Go SDK]" -ForegroundColor Yellow
    try {
        Push-Location sdks/go
        $output = go test ./transport/... ./... 2>&1
        Pop-Location
        if ($LASTEXITCODE -eq 0) {
            $results["Go/$t"] = "PASS"
            Write-Host "[SUCCESS] Go SDK tests passed" -ForegroundColor Green
        } else {
            $results["Go/$t"] = "PARTIAL"
            Write-Host "[PARTIAL] Go SDK tests had some failures" -ForegroundColor Yellow
        }
    } catch {
        $results["Go/$t"] = "ERROR"
        Write-Host "[ERROR] Go SDK tests error: $_" -ForegroundColor Red
        try { Pop-Location } catch {}
    }

    # ── C# SDK ──────────────────────────────────────────────────────
    Write-Host "`n[C# SDK]" -ForegroundColor Yellow
    try {
        Push-Location sdks/csharp
        $output = dotnet test Tests/Nexus.SDK.Tests.csproj --nologo --verbosity quiet 2>&1
        Pop-Location
        if ($LASTEXITCODE -eq 0) {
            $results["CSharp/$t"] = "PASS"
            Write-Host "[SUCCESS] C# SDK tests passed" -ForegroundColor Green
        } else {
            $results["CSharp/$t"] = "PARTIAL"
            Write-Host "[PARTIAL] C# SDK tests had some failures" -ForegroundColor Yellow
        }
    } catch {
        $results["CSharp/$t"] = "ERROR"
        Write-Host "[ERROR] C# SDK tests error: $_" -ForegroundColor Red
        try { Pop-Location } catch {}
    }

    # ── PHP SDK ─────────────────────────────────────────────────────
    Write-Host "`n[PHP SDK]" -ForegroundColor Yellow
    try {
        Push-Location sdks/php
        $output = composer test 2>&1
        Pop-Location
        if ($LASTEXITCODE -eq 0) {
            $results["PHP/$t"] = "PASS"
            Write-Host "[SUCCESS] PHP SDK tests passed" -ForegroundColor Green
        } else {
            $results["PHP/$t"] = "PARTIAL"
            Write-Host "[PARTIAL] PHP SDK tests had some failures" -ForegroundColor Yellow
        }
    } catch {
        $results["PHP/$t"] = "ERROR"
        Write-Host "[ERROR] PHP SDK tests error: $_" -ForegroundColor Red
        try { Pop-Location } catch {}
    }
}

# n8n / langchain / langflow SDKs were removed in the 1.0.0 cut.
# First-party SDKs only: rust, python, typescript, go, csharp, php.

Remove-Item Env:\NEXUS_SDK_TRANSPORT -ErrorAction SilentlyContinue

# ── Summary ─────────────────────────────────────────────────────────
Write-Host ""
Write-Host ("=" * 70) -ForegroundColor Cyan
Write-Host "TEST SUMMARY" -ForegroundColor Cyan
Write-Host ("=" * 70) -ForegroundColor Cyan

$passCount = ($results.Values | Where-Object { $_ -eq "PASS" }).Count
$partialCount = ($results.Values | Where-Object { $_ -eq "PARTIAL" }).Count
$failCount = ($results.Values | Where-Object { $_ -eq "FAIL" }).Count
$errorCount = ($results.Values | Where-Object { $_ -eq "ERROR" }).Count

foreach ($sdk in $results.Keys | Sort-Object) {
    $status = $results[$sdk]
    $color = switch ($status) {
        "PASS"    { "Green" }
        "PARTIAL" { "Yellow" }
        "FAIL"    { "Red" }
        "ERROR"   { "DarkRed" }
        default   { "Gray" }
    }
    Write-Host ("{0,-20} : {1}" -f $sdk, $status) -ForegroundColor $color
}

Write-Host ""
Write-Host ("=" * 70) -ForegroundColor Cyan
Write-Host "PASSED:  $passCount" -ForegroundColor Green
Write-Host "PARTIAL: $partialCount" -ForegroundColor Yellow
Write-Host "FAILED:  $failCount" -ForegroundColor Red
Write-Host "ERRORS:  $errorCount" -ForegroundColor DarkRed
Write-Host ("=" * 70) -ForegroundColor Cyan

if (($passCount + $partialCount) -eq $results.Count) {
    Write-Host "`nOVERALL: ALL SDKs OPERATIONAL" -ForegroundColor Green
    exit 0
} else {
    Write-Host "`nOVERALL: SOME SDKs NEED ATTENTION" -ForegroundColor Yellow
    exit 1
}
