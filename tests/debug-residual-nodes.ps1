# List all nodes to see what's residual

$NexusUri = "http://localhost:15474"

function Invoke-NexusQuery {
    param([string]$Cypher)
    $body = @{ query = $Cypher } | ConvertTo-Json
    try {
        return Invoke-RestMethod -Uri "$NexusUri/cypher" -Method POST -Headers @{"Content-Type" = "application/json"} -Body $body
    } catch {
        Write-Host "[ERROR] $_" -ForegroundColor Red
        return $null
    }
}

Write-Host "`n[DEBUG] Listing all nodes in database`n" -ForegroundColor Cyan

$result = Invoke-NexusQuery -Cypher "MATCH (n) RETURN id(n) AS id, labels(n) AS labels, n.name AS name, n.age AS age"

Write-Host "Total nodes: $($result.rows.Count)" -ForegroundColor Yellow

foreach ($row in $result.rows) {
    $id = if ($row -is [array]) { $row[0] } else { $row.values[0] }
    $labels = if ($row -is [array]) { $row[1] } else { $row.values[1] }
    $name = if ($row -is [array]) { $row[2] } else { $row.values[2] }
    $age = if ($row -is [array]) { $row[3] } else { $row.values[3] }
    
    Write-Host "  ID: $id | Labels: $labels | Name: $name | Age: $age"
}

