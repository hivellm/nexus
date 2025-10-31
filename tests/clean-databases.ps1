# Clean both Neo4j and Nexus databases

param(
    [string]$Neo4jUri = "http://localhost:7474",
    [string]$Neo4jUser = "neo4j",
    [string]$Neo4jPassword = "password",
    [string]$NexusUri = "http://localhost:15474"
)

Write-Host "[CLEAN] Cleaning databases..." -ForegroundColor Cyan

# Clean Neo4j
Write-Host "  Cleaning Neo4j..." -ForegroundColor Yellow
$body = @{
    statements = @(
        @{
            statement = "MATCH (n) DETACH DELETE n"
        }
    )
} | ConvertTo-Json -Depth 3

$auth = [Convert]::ToBase64String([Text.Encoding]::ASCII.GetBytes("${Neo4jUser}:${Neo4jPassword}"))

try {
    Invoke-RestMethod -Uri "$Neo4jUri/db/neo4j/tx/commit" `
        -Method POST `
        -Headers @{
            "Authorization" = "Basic $auth"
            "Content-Type" = "application/json"
        } `
        -Body $body `
        -ErrorAction Stop | Out-Null
    Write-Host "  [OK] Neo4j cleaned" -ForegroundColor Green
}
catch {
    Write-Host "  [ERROR] Neo4j cleanup failed: $_" -ForegroundColor Red
}

# Clean Nexus
Write-Host "  Cleaning Nexus..." -ForegroundColor Yellow
$nexusBody = @{
    query = "MATCH (n) DETACH DELETE n"
} | ConvertTo-Json

try {
    Invoke-RestMethod -Uri "$NexusUri/cypher" `
        -Method POST `
        -Headers @{"Content-Type" = "application/json"} `
        -Body $nexusBody `
        -ErrorAction Stop | Out-Null
    Write-Host "  [OK] Nexus cleaned" -ForegroundColor Green
}
catch {
    Write-Host "  [ERROR] Nexus cleanup failed: $_" -ForegroundColor Red
}

Write-Host "`n[DONE] Database cleanup complete!" -ForegroundColor Cyan

