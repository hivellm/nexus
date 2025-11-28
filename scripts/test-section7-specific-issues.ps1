# Test script for specific Section 7 relationship issues
# Tests the failing queries individually to isolate the problem

param(
    [string]$Neo4jUri = "http://localhost:7474",
    [string]$Neo4jUser = "neo4j",
    [string]$Neo4jPassword = "password",
    [string]$NexusUri = "http://localhost:15474",
    [switch]$Verbose
)

$ErrorActionPreference = "Continue"

Write-Host "+= = = = = = = = = = = = = = = = = = = = = = = = = = = = = = = = = = = = = = = = = = = = = = = = = = = = = = = = = = = = = = ╗" -ForegroundColor Cyan
Write-Host "|  Section 7 Specific Relationship Issues Test                    |" -ForegroundColor Cyan
Write-Host "+= = = = = = = = = = = = = = = = = = = = = = = = = = = = = = = = = = = = = = = = = = = = = = = = = = = = = = = = = = = = = = ╝" -ForegroundColor Cyan
Write-Host ""

# Function to execute query on Neo4j
function Invoke-Neo4jQuery {
    param([string]$Cypher, [hashtable]$Parameters = @{})
    
    $body = @{
        statements = @(
            @{
                statement = $Cypher
                parameters = $Parameters
            }
        )
    } | ConvertTo-Json -Depth 10
    
    $auth = [Convert]::ToBase64String([Text.Encoding]::ASCII.GetBytes("${Neo4jUser}:${Neo4jPassword}"))
    
    try {
        $response = Invoke-RestMethod -Uri "$Neo4jUri/db/neo4j/tx/commit" `
            -Method POST `
            -Headers @{
                "Authorization" = "Basic $auth"
                "Content-Type" = "application/json"
                "Accept" = "application/json"
            } `
            -Body $body `
            -ErrorAction Stop `
            -TimeoutSec 30
        
        if ($response.errors -and $response.errors.Count -gt 0) {
            return @{ error = $response.errors[0].message }
        }
        
        return $response.results[0]
    }
    catch {
        return @{ error = $_.Exception.Message }
    }
}

# Function to execute query on Nexus
function Invoke-NexusQuery {
    param([string]$Cypher, [hashtable]$Parameters = @{})
    
    $body = @{
        query = $Cypher
        parameters = $Parameters
    } | ConvertTo-Json -Depth 10
    
    try {
        $response = Invoke-RestMethod -Uri "$NexusUri/cypher" `
            -Method POST `
            -Headers @{
                "Content-Type" = "application/json"
                "Accept" = "application/json"
            } `
            -Body $body `
            -ErrorAction Stop `
            -TimeoutSec 30
        
        return $response
    }
    catch {
        return @{ error = $_.Exception.Message }
    }
}

# Function to compare results in detail
function Compare-QueryResults {
    param(
        [string]$TestName,
        [string]$Query,
        [object]$Neo4jResult,
        [object]$NexusResult
    )
    
    Write-Host "`n--- $TestName ---" -ForegroundColor Cyan
    Write-Host "Query: $Query" -ForegroundColor Gray
    Write-Host ""
    
    # Check for errors
    if ($Neo4jResult.error) {
        Write-Host "Neo4j ERROR: $($Neo4jResult.error)" -ForegroundColor Red
        return
    }
    
    if ($NexusResult.error) {
        Write-Host "Nexus ERROR: $($NexusResult.error)" -ForegroundColor Red
        return
    }
    
    # Extract rows
    $neo4jRows = if ($Neo4jResult.data) { $Neo4jResult.data } else { @() }
    $nexusRows = if ($NexusResult.rows) { $NexusResult.rows } else { @() }
    
    Write-Host "Neo4j rows: $($neo4jRows.Count)" -ForegroundColor Yellow
    Write-Host "Nexus rows: $($nexusRows.Count)" -ForegroundColor Yellow
    Write-Host ""
    
    if ($neo4jRows.Count -ne $nexusRows.Count) {
        Write-Host "❌ FAILED: Row count mismatch" -ForegroundColor Red
    } else {
        Write-Host "✅ PASSED: Row count matches" -ForegroundColor Green
    }
    
    Write-Host "`nNeo4j Results:" -ForegroundColor Cyan
    for ($i = 0; $i -lt $neo4jRows.Count; $i++) {
        Write-Host "  Row $i : $($neo4jRows[$i] | ConvertTo-Json -Compress)" -ForegroundColor White
    }
    
    Write-Host "`nNexus Results:" -ForegroundColor Cyan
    for ($i = 0; $i -lt $nexusRows.Count; $i++) {
        Write-Host "  Row $i : $($nexusRows[$i] | ConvertTo-Json -Compress)" -ForegroundColor White
    }
    
    Write-Host ""
}

# Setup function to create test data
function Setup-TestData {
    try {
        # Delete existing test nodes
        Invoke-Neo4jQuery -Cypher "MATCH (n) WHERE n.name IN ['Alice', 'Bob', 'Acme', 'TechCorp'] DETACH DELETE n" | Out-Null
        Invoke-NexusQuery -Cypher "MATCH (n) WHERE n.name IN ['Alice', 'Bob', 'Acme', 'TechCorp'] DETACH DELETE n" | Out-Null
        
        # Create Person and Company nodes with relationships
        Invoke-Neo4jQuery -Cypher "CREATE (p1:Person {name: 'Alice'}), (p2:Person {name: 'Bob'}), (c1:Company {name: 'Acme'}), (c2:Company {name: 'TechCorp'})" | Out-Null
        Invoke-Neo4jQuery -Cypher "MATCH (p1:Person {name: 'Alice'}), (c1:Company {name: 'Acme'}) CREATE (p1)-[:WORKS_AT {since: 2020}]->(c1)" | Out-Null
        Invoke-Neo4jQuery -Cypher "MATCH (p1:Person {name: 'Alice'}), (c2:Company {name: 'TechCorp'}) CREATE (p1)-[:WORKS_AT {since: 2021}]->(c2)" | Out-Null
        Invoke-Neo4jQuery -Cypher "MATCH (p2:Person {name: 'Bob'}), (c1:Company {name: 'Acme'}) CREATE (p2)-[:WORKS_AT {since: 2019}]->(c1)" | Out-Null
        Invoke-Neo4jQuery -Cypher "MATCH (p1:Person {name: 'Alice'}), (p2:Person {name: 'Bob'}) CREATE (p1)-[:KNOWS]->(p2)" | Out-Null
        
        Invoke-NexusQuery -Cypher "CREATE (p1:Person {name: 'Alice'}), (p2:Person {name: 'Bob'}), (c1:Company {name: 'Acme'}), (c2:Company {name: 'TechCorp'})" | Out-Null
        Invoke-NexusQuery -Cypher "MATCH (p1:Person {name: 'Alice'}), (c1:Company {name: 'Acme'}) CREATE (p1)-[:WORKS_AT {since: 2020}]->(c1)" | Out-Null
        Invoke-NexusQuery -Cypher "MATCH (p1:Person {name: 'Alice'}), (c2:Company {name: 'TechCorp'}) CREATE (p1)-[:WORKS_AT {since: 2021}]->(c2)" | Out-Null
        Invoke-NexusQuery -Cypher "MATCH (p2:Person {name: 'Bob'}), (c1:Company {name: 'Acme'}) CREATE (p2)-[:WORKS_AT {since: 2019}]->(c1)" | Out-Null
        Invoke-NexusQuery -Cypher "MATCH (p1:Person {name: 'Alice'}), (p2:Person {name: 'Bob'}) CREATE (p1)-[:KNOWS]->(p2)" | Out-Null
        
        Start-Sleep -Milliseconds 500
    } catch {
        Write-Host "WARN Warning: Setup data creation failed: $($_.Exception.Message)" -ForegroundColor Yellow
    }
}

# Cleanup function
function Clear-Databases {
    try {
        Invoke-Neo4jQuery -Cypher "MATCH (n) DETACH DELETE n" | Out-Null
        Invoke-NexusQuery -Cypher "MATCH (n) DETACH DELETE n" | Out-Null
        Start-Sleep -Milliseconds 200
    } catch {
        Write-Host "WARN Warning: Cleanup failed: $($_.Exception.Message)" -ForegroundColor Yellow
    }
}

# Setup: Clean databases completely
Write-Host "🔧 Setting up test environment..." -ForegroundColor Cyan
Write-Host "Cleaning Neo4j database..." -ForegroundColor Yellow
try {
    Invoke-Neo4jQuery -Cypher "MATCH (n) DETACH DELETE n" | Out-Null
    Invoke-Neo4jQuery -Cypher "MATCH ()-[r]->() DELETE r" | Out-Null
    Write-Host "  Neo4j cleaned" -ForegroundColor Green
} catch {
    Write-Host "  Warning: Neo4j cleanup failed: $($_.Exception.Message)" -ForegroundColor Yellow
}

Write-Host "Cleaning Nexus database..." -ForegroundColor Yellow
try {
    Invoke-NexusQuery -Cypher "MATCH (n) DETACH DELETE n" | Out-Null
    Invoke-NexusQuery -Cypher "MATCH ()-[r]->() DELETE r" | Out-Null
    Write-Host "  Nexus cleaned" -ForegroundColor Green
} catch {
    Write-Host "  Warning: Nexus cleanup failed: $($_.Exception.Message)" -ForegroundColor Yellow
}

Start-Sleep -Milliseconds 500

Write-Host "Creating test data..." -ForegroundColor Yellow
Setup-TestData
Write-Host "OK Test data created`n" -ForegroundColor Green

# Test the specific failing queries
Write-Host "`n+-----------------------------------------------------+ " -ForegroundColor Yellow
Write-Host '| Testing Specific Failing Queries                    |' -ForegroundColor Yellow
Write-Host "+-----------------------------------------------------+ " -ForegroundColor Yellow

$neo4jResult = Invoke-Neo4jQuery -Cypher 'MATCH ()-[r]->() RETURN type(r) AS t, count(r) AS cnt ORDER BY t'
$nexusResult = Invoke-NexusQuery -Cypher 'MATCH ()-[r]->() RETURN type(r) AS t, count(r) AS cnt ORDER BY t'
Compare-QueryResults -TestName "7.07 Count relationships by type" -Query 'MATCH ()-[r]->() RETURN type(r) AS t, count(r) AS cnt ORDER BY t' -Neo4jResult $neo4jResult -NexusResult $nexusResult

$neo4jResult = Invoke-Neo4jQuery -Cypher "MATCH (a)-[r:KNOWS]->(b) RETURN a.name AS source"
$nexusResult = Invoke-NexusQuery -Cypher "MATCH (a)-[r:KNOWS]->(b) RETURN a.name AS source"
Compare-QueryResults -TestName "7.11 Return source node" -Query "MATCH (a)-[r:KNOWS]->(b) RETURN a.name AS source" -Neo4jResult $neo4jResult -NexusResult $nexusResult

$neo4jResult = Invoke-Neo4jQuery -Cypher "MATCH (a)-[r:KNOWS]->(b) RETURN b.name AS target"
$nexusResult = Invoke-NexusQuery -Cypher "MATCH (a)-[r:KNOWS]->(b) RETURN b.name AS target"
Compare-QueryResults -TestName "7.12 Return target node" -Query "MATCH (a)-[r:KNOWS]->(b) RETURN b.name AS target" -Neo4jResult $neo4jResult -NexusResult $nexusResult

$neo4jResult = Invoke-Neo4jQuery -Cypher "MATCH (a)-[r:KNOWS]->(b) RETURN a.name AS src, b.name AS dst"
$nexusResult = Invoke-NexusQuery -Cypher "MATCH (a)-[r:KNOWS]->(b) RETURN a.name AS src, b.name AS dst"
Compare-QueryResults -TestName "7.13 Return both nodes" -Query "MATCH (a)-[r:KNOWS]->(b) RETURN a.name AS src, b.name AS dst" -Neo4jResult $neo4jResult -NexusResult $nexusResult

$neo4jResult = Invoke-Neo4jQuery -Cypher 'MATCH ()-[r]->() RETURN DISTINCT type(r) AS t ORDER BY t'
$nexusResult = Invoke-NexusQuery -Cypher 'MATCH ()-[r]->() RETURN DISTINCT type(r) AS t ORDER BY t'
Compare-QueryResults -TestName "7.29 Return distinct rel types" -Query 'MATCH ()-[r]->() RETURN DISTINCT type(r) AS t ORDER BY t' -Neo4jResult $neo4jResult -NexusResult $nexusResult

Write-Host "`n+= = = = = = = = = = = = = = = = = = = = = = = = = = = = = = = = = = = = = = = = = = = = = = = = = = = = = = = = = = = = = = +" -ForegroundColor Cyan
Write-Host "Test completed!" -ForegroundColor Green

