$nexusUrl = "http://127.0.0.1:15474"

Write-Host "=== Testing COUNT After Fix ===" -ForegroundColor Cyan

$testQueries = @(
    "MATCH (d:Document) RETURN count(d) AS total",
    "MATCH (m:Module) RETURN count(m) AS total",
    "MATCH (c:Class) RETURN count(c) AS total",
    "MATCH (f:Function) RETURN count(f) AS total",
    "MATCH ()-[r:MENTIONS]->() RETURN count(r) AS total"
)

foreach ($query in $testQueries) {
    Write-Host "`nTesting: $query" -ForegroundColor Yellow
    try {
        $body = @{ query = $query } | ConvertTo-Json
        $res = Invoke-RestMethod -Uri "$nexusUrl/cypher" -Method POST -Body $body -ContentType "application/json"
        
        $count = if ($res.rows -and $res.rows.Count -gt 0) {
            $res.rows[0]
        } else {
            "No result"
        }
        
        $status = if ($count -is [Int64] -or $count -is [Int32] -or $count -is [Double]) {
            if ($count -gt 0) { "OK" } else { "ZERO" }
        } else {
            "UNKNOWN ($count)"
        }
        
        Write-Host "  Result: $count $status" -ForegroundColor $(if ($count -gt 0) { "Green" } else { "Yellow" })
    } catch {
        Write-Host "  ERROR: $_" -ForegroundColor Red
    }
}

Write-Host ""

