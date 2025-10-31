# Test CREATE in isolation

$NexusUri = "http://localhost:15474"

function Invoke-NexusQuery {
    param([string]$Cypher)
    
    $body = @{ query = $Cypher } | ConvertTo-Json
    
    try {
        $response = Invoke-RestMethod -Uri "$NexusUri/cypher" `
            -Method POST `
            -Headers @{"Content-Type" = "application/json"} `
            -Body $body
        return $response
    }
    catch {
        Write-Host "[ERROR] $_" -ForegroundColor Red
        return $null
    }
}

function Get-Count {
    $result = Invoke-NexusQuery -Cypher "MATCH (n) RETURN count(*) AS count"
    if ($null -eq $result -or $null -eq $result.rows -or $result.rows.Count -eq 0) {
        return 0
    }
    if ($result.rows[0] -is [array]) {
        return $result.rows[0][0]
    } else {
        return 0
    }
}

Write-Host "`n[TEST] CREATE Duplication Test`n" -ForegroundColor Cyan

# Clean
Write-Host "0. Cleaning database..." -ForegroundColor Yellow
Invoke-NexusQuery -Cypher "MATCH (n) DETACH DELETE n" | Out-Null
$count0 = Get-Count
Write-Host "   Count: $count0 (expected: 0)" -ForegroundColor $(if ($count0 -eq 0) { "Green" } else { "Red" })

# Create 1 node
Write-Host "`n1. CREATE (p:Person {name: 'Alice'})" -ForegroundColor Yellow
Invoke-NexusQuery -Cypher "CREATE (p:Person {name: 'Alice', age: 30})" | Out-Null
$count1 = Get-Count
Write-Host "   Count: $count1 (expected: 1)" -ForegroundColor $(if ($count1 -eq 1) { "Green" } else { "Red" })

# Create another
Write-Host "`n2. CREATE (p:Person {name: 'Bob'})" -ForegroundColor Yellow
Invoke-NexusQuery -Cypher "CREATE (p:Person {name: 'Bob', age: 25})" | Out-Null
$count2 = Get-Count
Write-Host "   Count: $count2 (expected: 2)" -ForegroundColor $(if ($count2 -eq 2) { "Green" } else { "Red" })

# Create third
Write-Host "`n3. CREATE (p:Person {name: 'Charlie'})" -ForegroundColor Yellow
Invoke-NexusQuery -Cypher "CREATE (p:Person {name: 'Charlie', age: 35})" | Out-Null
$count3 = Get-Count
Write-Host "   Count: $count3 (expected: 3)" -ForegroundColor $(if ($count3 -eq 3) { "Green" } else { "Red" })

if ($count1 -eq 1 -and $count2 -eq 2 -and $count3 -eq 3) {
    Write-Host "`n[PASS] CREATE works correctly - no duplication!" -ForegroundColor Green
} else {
    Write-Host "`n[FAIL] CREATE is duplicating nodes!" -ForegroundColor Red
}

