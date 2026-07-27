#!/usr/bin/env pwsh
# Reproducible entry point for the openCypher TCK conformance baseline.
#
# Runs the vendored upstream openCypher TCK corpus
# (crates/nexus-core/tests/tck/opencypher/features) through the Nexus engine
# and regenerates docs/compatibility/OPENCYPHER_TCK_REPORT.md with per-category
# pass/fail/skip counts. This is a CONFORMANCE measurement against the
# specification, distinct from the differential Neo4j suite
# (test-neo4j-nexus-compatibility-200.ps1), which needs a live Neo4j.
#
# The runner is gated behind NEXUS_TCK=1 so a plain `cargo test` does not pay
# for the ~1600 isolated-engine scenarios; this script sets it for you.

$ErrorActionPreference = "Stop"

# Repo root is two levels up from scripts/compatibility.
$repoRoot = Resolve-Path (Join-Path $PSScriptRoot "..\..")
Push-Location $repoRoot
try {
    Write-Host "Running openCypher TCK conformance baseline..." -ForegroundColor Cyan
    Write-Host "(vendored corpus -> docs/compatibility/OPENCYPHER_TCK_REPORT.md)" -ForegroundColor DarkGray

    $env:NEXUS_TCK = "1"
    & cargo +nightly test -p nexus-core --test tck_opencypher --all-features
    $exit = $LASTEXITCODE

    if ($exit -ne 0) {
        Write-Host "Runner exited with code $exit." -ForegroundColor Yellow
    } else {
        Write-Host "Done. See docs/compatibility/OPENCYPHER_TCK_REPORT.md" -ForegroundColor Green
    }
    exit $exit
}
finally {
    Remove-Item Env:\NEXUS_TCK -ErrorAction SilentlyContinue
    Pop-Location
}
