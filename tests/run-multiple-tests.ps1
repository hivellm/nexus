Write-Host "`n=============================================================" -ForegroundColor Cyan
Write-Host "      RUNNING COMPATIBILITY TEST 3 TIMES FOR VALIDATION" -ForegroundColor Cyan
Write-Host "=============================================================`n" -ForegroundColor Cyan

$results = @()

for ($i = 1; $i -le 3; $i++) {
    Write-Host "`n==== EXECUTION #$i ====`n" -ForegroundColor Yellow
    
    $output = & "F:\Node\hivellm\nexus\tests\cross-compatibility\test-compatibility.ps1" 2>&1 | Out-String
    
    if ($output -match "Pass Rate: (\d+)%") {
        $passRate = $matches[1]
        $results += $passRate
        Write-Host "  Result: $passRate% pass rate`n" -ForegroundColor $(if ($passRate -eq 100) { "Green" } else { "Yellow" })
    } else {
        Write-Host "  Result: Could not parse`n" -ForegroundColor Red
    }
    
    Start-Sleep -Seconds 1
}

Write-Host "`n=============================================================" -ForegroundColor Cyan
Write-Host "                  FINAL RESULTS" -ForegroundColor Cyan
Write-Host "=============================================================`n" -ForegroundColor Cyan

Write-Host "Execution 1: $($results[0])%" -ForegroundColor Green
Write-Host "Execution 2: $($results[1])%" -ForegroundColor Green
Write-Host "Execution 3: $($results[2])%" -ForegroundColor Green

$avgRate = ($results | Measure-Object -Average).Average
Write-Host "`nAverage Pass Rate: $avgRate%" -ForegroundColor $(if ($avgRate -eq 100) { "Green" } else { "Yellow" })

if ($results[0] -eq $results[1] -and $results[1] -eq $results[2] -and $results[0] -eq 100) {
    Write-Host "`nPERFECT! 100% CONSISTENT ACROSS ALL 3 RUNS!`n" -ForegroundColor Green
} elseif ($avgRate -eq 100) {
    Write-Host "`nEXCELLENT! Average 100% compatibility!`n" -ForegroundColor Green
} elseif ($avgRate -ge 95) {
    Write-Host "`nVERY GOOD! Over 95% average compatibility!`n" -ForegroundColor Yellow
} else {
    Write-Host "`nSome variance in results`n" -ForegroundColor Yellow
}
