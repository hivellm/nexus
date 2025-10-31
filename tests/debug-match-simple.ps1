# Test simple MATCH with inline properties

$NexusUri = "http://localhost:15474"

function Invoke-NexusQuery {
    param([string]$Cypher, [switch]$ShowResult)
    
    $body = @{ query = $Cypher } | ConvertTo-Json
    
    try {
        $response = Invoke-RestMethod -Uri "$NexusUri/cypher" `
            -Method POST `
            -Headers @{"Content-Type" = "application/json"} `
            -Body $body `
            -ErrorAction Stop
        
        if ($ShowResult) {
            Write-Host "   Rows: $($response.rows.Count)" -ForegroundColor Cyan
            Write-Host "   Columns: $($response.columns -join ', ')" -ForegroundColor Cyan
        }
        
        return $response
    }
    catch {
        Write-Host "[ERROR] $_" -ForegroundColor Red
        return $null
    }
}

# Clean first
Write-Host "[CLEAN] Cleaning Nexus..." -ForegroundColor Yellow
Invoke-NexusQuery -Cypher "MATCH (n) DETACH DELETE n" | Out-Null

# Create Alice
Write-Host "`n[CREATE] Creating Alice..." -ForegroundColor Yellow
Invoke-NexusQuery -Cypher "CREATE (p:Person {name: 'Alice', age: 30})" | Out-Null

# Create Bob
Write-Host "[CREATE] Creating Bob..." -ForegroundColor Yellow
Invoke-NexusQuery -Cypher "CREATE (p:Person {name: 'Bob', age: 25})" | Out-Null

# Count
Write-Host "`n[COUNT] After creating 2 nodes..." -ForegroundColor Yellow
Invoke-NexusQuery -Cypher "MATCH (n) RETURN count(*) AS count" -ShowResult

# Test MATCH with inline properties
Write-Host "`n[MATCH] MATCH (p1:Person {name: 'Alice'}), (p2:Person {name: 'Bob'}) RETURN p1, p2..." -ForegroundColor Yellow
$result = Invoke-NexusQuery -Cypher "MATCH (p1:Person {name: 'Alice'}), (p2:Person {name: 'Bob'}) RETURN p1, p2" -ShowResult

Write-Host "`nExpected: 1 row with 2 columns (p1, p2)"
Write-Host "Actual: $($result.rows.Count) rows"

if ($result.rows.Count -ne 1) {
    Write-Host "[BUG] MATCH returned $($result.rows.Count) rows instead of 1!" -ForegroundColor Red
}

