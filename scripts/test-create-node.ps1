# Test creating a node and then querying it
$createBody = @{
    query = "CREATE (n:Person {name: 'TestAlice', age: 30, city: 'NYC'}) RETURN n"
} | ConvertTo-Json -Depth 10

Write-Host "Creating node..."
try {
    $createResponse = Invoke-RestMethod -Uri "http://localhost:15474/cypher" `
        -Method POST `
        -Headers @{
            "Content-Type" = "application/json"
        } `
        -Body $createBody `
        -ErrorAction Stop
    
    Write-Host "Create response:"
    $createResponse | ConvertTo-Json -Depth 10
} catch {
    Write-Host "Error creating: $($_.Exception.Message)"
}

Write-Host "`nQuerying node..."
$queryBody = @{
    query = "MATCH (n:Person {name: 'TestAlice'}) RETURN n"
} | ConvertTo-Json -Depth 10

try {
    $queryResponse = Invoke-RestMethod -Uri "http://localhost:15474/cypher" `
        -Method POST `
        -Headers @{
            "Content-Type" = "application/json"
        } `
        -Body $queryBody `
        -ErrorAction Stop
    
    Write-Host "Query response:"
    $queryResponse | ConvertTo-Json -Depth 10
} catch {
    Write-Host "Error querying: $($_.Exception.Message)"
}

