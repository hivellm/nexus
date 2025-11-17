# Run compatibility test and save results
$scriptPath = Join-Path $PSScriptRoot "scripts\test-neo4j-nexus-compatibility-200.ps1"
$outputFile = Join-Path $PSScriptRoot "scripts\compatibility_results.txt"

Write-Host "Running compatibility tests..." -ForegroundColor Cyan
& $scriptPath *> $outputFile

Write-Host "Results saved to: $outputFile" -ForegroundColor Green
Get-Content $outputFile | Select-Object -Last 50

