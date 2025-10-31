$NexusUri = "http://localhost:15474"

function Invoke-NexusQuery {
    param([string]$Cypher)
    $body = @{ query = $Cypher } | ConvertTo-Json
    Write-Host "`n[QUERY] $Cypher" -ForegroundColor Cyan
    try {
        $result = Invoke-RestMethod -Uri "$NexusUri/cypher" -Method POST -Headers @{"Content-Type" = "application/json"} -Body $body
        Write-Host "[RESULT] Columns: $($result.columns.Count), Rows: $($result.rows.Count)" -ForegroundColor Yellow
        return $result
    } catch {
        Write-Host "[ERROR] $_" -ForegroundColor Red
        return $null
    }
}

Write-Host "`n=== DETACH DELETE DEBUG TEST ===" -ForegroundColor Green

Invoke-NexusQuery -Cypher "MATCH (n) DETACH DELETE n" | Out-Null

Start-Sleep -Seconds 1

Invoke-NexusQuery -Cypher "CREATE (n:Test {name: 'test1'})" | Out-Null

Start-Sleep -Seconds 1

Invoke-NexusQuery -Cypher "MATCH (n:Test) DETACH DELETE n" | Out-Null

Start-Sleep -Seconds 1

$result = Invoke-NexusQuery -Cypher "MATCH (n) RETURN count(*) AS count"
$count = if ($result.rows[0] -is [array]) { $result.rows[0][0] } else { 0 }
Write-Host "`n[FINAL] Total nodes remaining: $count" -ForegroundColor $(if ($count -eq 0) { "Green" } else { "Red" })

