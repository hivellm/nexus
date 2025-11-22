# Script to test UNION and DISTINCT queries and capture logs

$NEXUS_URL = "http://localhost:15474/api/v1/query"

Write-Host "=== Setting up test data ===" -ForegroundColor Cyan
Invoke-RestMethod -Uri "$NEXUS_URL" -Method Post -Body (@{ query = "MATCH (n) DETACH DELETE n" } | ConvertTo-Json) -ContentType "application/json" | Out-Null
Invoke-RestMethod -Uri "$NEXUS_URL" -Method Post -Body (@{ query = "CREATE (a:Person {name: 'Alice', age: 30, city: 'NYC'}), (b:Person {name: 'Bob', age: 25, city: 'LA'}), (c:Person {name: 'Charlie', age: 35, city: 'NYC'}), (d:Person {name: 'David', age: 28, city: 'LA'})" } | ConvertTo-Json) -ContentType "application/json" | Out-Null
Invoke-RestMethod -Uri "$NEXUS_URL" -Method Post -Body (@{ query = "CREATE (c1:Company {name: 'Acme'}), (c2:Company {name: 'TechCorp'})" } | ConvertTo-Json) -ContentType "application/json" | Out-Null

Write-Host ""
Write-Host "=== Testing UNION Query (10.01) ===" -ForegroundColor Yellow
Write-Host "Query: MATCH (n:Person) RETURN n.name AS name UNION MATCH (n:Company) RETURN n.name AS name"
Write-Host "Expected: 6 rows (Alice, Bob, Charlie, David, Acme, TechCorp)"
try {
    $result = Invoke-RestMethod -Uri "$NEXUS_URL" -Method Post -Body (@{ query = "MATCH (n:Person) RETURN n.name AS name UNION MATCH (n:Company) RETURN n.name AS name" } | ConvertTo-Json) -ContentType "application/json"
    $rowCount = if ($result.rows) { $result.rows.Count } else { 0 }
    Write-Host "Got: $rowCount rows" -ForegroundColor $(if ($rowCount -eq 6) { "Green" } else { "Red" })
    if ($result.rows) {
        $result.rows | ForEach-Object { Write-Host "  - $($_.name)" }
    }
} catch {
    Write-Host "Error: $_" -ForegroundColor Red
}
Write-Host ""

Write-Host "=== Testing DISTINCT Query (2.20) ===" -ForegroundColor Yellow
Write-Host "Query: MATCH (n:Person) RETURN DISTINCT n.city AS city"
Write-Host "Expected: 2 rows (NYC, LA)"
try {
    $result = Invoke-RestMethod -Uri "$NEXUS_URL" -Method Post -Body (@{ query = "MATCH (n:Person) RETURN DISTINCT n.city AS city" } | ConvertTo-Json) -ContentType "application/json"
    $rowCount = if ($result.rows) { $result.rows.Count } else { 0 }
    Write-Host "Got: $rowCount rows" -ForegroundColor $(if ($rowCount -eq 2) { "Green" } else { "Red" })
    if ($result.rows) {
        $result.rows | ForEach-Object { Write-Host "  - $($_.city)" }
    }
} catch {
    Write-Host "Error: $_" -ForegroundColor Red
}
Write-Host ""

Write-Host "=== Testing UNION with WHERE (10.05) ===" -ForegroundColor Yellow
Write-Host "Query: MATCH (n:Person) WHERE n.age > 30 RETURN n.name AS name UNION MATCH (n:Company) RETURN n.name AS name"
Write-Host "Expected: 4 rows (Alice, Charlie, Acme, TechCorp)"
try {
    $result = Invoke-RestMethod -Uri "$NEXUS_URL" -Method Post -Body (@{ query = "MATCH (n:Person) WHERE n.age > 30 RETURN n.name AS name UNION MATCH (n:Company) RETURN n.name AS name" } | ConvertTo-Json) -ContentType "application/json"
    $rowCount = if ($result.rows) { $result.rows.Count } else { 0 }
    Write-Host "Got: $rowCount rows" -ForegroundColor $(if ($rowCount -eq 4) { "Green" } else { "Red" })
    if ($result.rows) {
        $result.rows | ForEach-Object { Write-Host "  - $($_.name)" }
    }
} catch {
    Write-Host "Error: $_" -ForegroundColor Red
}
Write-Host ""

Write-Host "=== Testing UNION empty results (10.08) ===" -ForegroundColor Yellow
Write-Host "Query: MATCH (n:NonExistent) RETURN n.name AS name UNION MATCH (n:Person) RETURN n.name AS name"
Write-Host "Expected: 4 rows (Alice, Bob, Charlie, David)"
try {
    $result = Invoke-RestMethod -Uri "$NEXUS_URL" -Method Post -Body (@{ query = "MATCH (n:NonExistent) RETURN n.name AS name UNION MATCH (n:Person) RETURN n.name AS name" } | ConvertTo-Json) -ContentType "application/json"
    $rowCount = if ($result.rows) { $result.rows.Count } else { 0 }
    Write-Host "Got: $rowCount rows" -ForegroundColor $(if ($rowCount -eq 4) { "Green" } else { "Red" })
    if ($result.rows) {
        $result.rows | ForEach-Object { Write-Host "  - $($_.name)" }
    }
} catch {
    Write-Host "Error: $_" -ForegroundColor Red
}

Write-Host ""
Write-Host "=== Checking server logs ===" -ForegroundColor Cyan
Write-Host "Log file: server-debug.log"
if (Test-Path "server-debug.log") {
    Write-Host "Last 50 lines with UNION/DISTINCT/Project/NodeByLabel:" -ForegroundColor Gray
    Get-Content "server-debug.log" -Tail 200 | Select-String -Pattern "UNION|DISTINCT|Project|NodeByLabel" | Select-Object -Last 50
} else {
    Write-Host "Log file not found" -ForegroundColor Yellow
}

