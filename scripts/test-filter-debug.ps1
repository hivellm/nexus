# Test filter with debug
Write-Host "Testing filter: MATCH (n:Person) WHERE n.name = 'Alice' RETURN n.name"

$body = @{
    query = "MATCH (n:Person) WHERE n.name = 'Alice' RETURN n.name AS name"
} | ConvertTo-Json -Depth 10

try {
    $response = Invoke-RestMethod -Uri "http://localhost:15474/cypher" `
        -Method POST `
        -Headers @{
            "Content-Type" = "application/json"
        } `
        -Body $body `
        -ErrorAction Stop
    
    Write-Host "Rows: $($response.rows.Count)"
    Write-Host "Response:"
    $response | ConvertTo-Json -Depth 5
} catch {
    Write-Host "Error: $($_.Exception.Message)"
}

Write-Host "`nTesting without filter: MATCH (n:Person) RETURN n.name LIMIT 4"

$body2 = @{
    query = "MATCH (n:Person) RETURN n.name AS name LIMIT 4"
} | ConvertTo-Json -Depth 10

try {
    $response2 = Invoke-RestMethod -Uri "http://localhost:15474/cypher" `
        -Method POST `
        -Headers @{
            "Content-Type" = "application/json"
        } `
        -Body $body2 `
        -ErrorAction Stop
    
    Write-Host "Rows: $($response2.rows.Count)"
    Write-Host "Response:"
    $response2 | ConvertTo-Json -Depth 5
} catch {
    Write-Host "Error: $($_.Exception.Message)"
}

