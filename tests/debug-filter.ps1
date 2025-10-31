# Test if inline property filters work

$NexusUri = "http://localhost:15474"

function Invoke-NexusQuery {
    param([string]$Cypher)
    
    $body = @{ query = $Cypher } | ConvertTo-Json
    
    try {
        $response = Invoke-RestMethod -Uri "$NexusUri/cypher" `
            -Method POST `
            -Headers @{"Content-Type" = "application/json"} `
            -Body $body `
            -ErrorAction Stop
        
        Write-Host "   Rows: $($response.rows.Count)" -ForegroundColor Cyan
        return $response
    }
    catch {
        Write-Host "[ERROR] $_" -ForegroundColor Red
        return $null
    }
}

# Clean
Invoke-NexusQuery -Cypher "MATCH (n) DETACH DELETE n" | Out-Null

# Create nodes
Invoke-NexusQuery -Cypher "CREATE (p:Person {name: 'Alice', age: 30})" | Out-Null
Invoke-NexusQuery -Cypher "CREATE (p:Person {name: 'Bob', age: 25})" | Out-Null

Write-Host "`n[TEST] MATCH (p:Person {name: 'Alice'}) RETURN p" -ForegroundColor Yellow
$r1 = Invoke-NexusQuery -Cypher "MATCH (p:Person {name: 'Alice'}) RETURN p"
Write-Host "Expected: 1 row, Actual: $($r1.rows.Count)" -ForegroundColor $(if ($r1.rows.Count -eq 1) { "Green" } else { "Red" })

Write-Host "`n[TEST] MATCH (p:Person {name: 'Bob'}) RETURN p" -ForegroundColor Yellow
$r2 = Invoke-NexusQuery -Cypher "MATCH (p:Person {name: 'Bob'}) RETURN p"
Write-Host "Expected: 1 row, Actual: $($r2.rows.Count)" -ForegroundColor $(if ($r2.rows.Count -eq 1) { "Green" } else { "Red" })

Write-Host "`n[TEST] MATCH (p1:Person), (p2:Person) RETURN p1.name, p2.name" -ForegroundColor Yellow
$r3 = Invoke-NexusQuery -Cypher "MATCH (p1:Person), (p2:Person) RETURN p1.name, p2.name"
Write-Host "Expected: 4 rows (Cartesian product), Actual: $($r3.rows.Count)" -ForegroundColor $(if ($r3.rows.Count -eq 4) { "Green" } else { "Red" })

Write-Host "`n[TEST] MATCH (p1:Person {name: 'Alice'}), (p2:Person) RETURN p1.name, p2.name" -ForegroundColor Yellow
$r4 = Invoke-NexusQuery -Cypher "MATCH (p1:Person {name: 'Alice'}), (p2:Person) RETURN p1.name, p2.name"
Write-Host "Expected: 2 rows (Alice x Alice, Alice x Bob), Actual: $($r4.rows.Count)" -ForegroundColor $(if ($r4.rows.Count -eq 2) { "Green" } else { "Red" })

Write-Host "`n[TEST] MATCH (p1:Person {name: 'Alice'}), (p2:Person {name: 'Bob'}) RETURN p1.name, p2.name" -ForegroundColor Yellow
$r5 = Invoke-NexusQuery -Cypher "MATCH (p1:Person {name: 'Alice'}), (p2:Person {name: 'Bob'}) RETURN p1.name, p2.name"
Write-Host "Expected: 1 row (Alice x Bob), Actual: $($r5.rows.Count)" -ForegroundColor $(if ($r5.rows.Count -eq 1) { "Green" } else { "Red" })

