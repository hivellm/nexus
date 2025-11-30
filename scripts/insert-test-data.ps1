# Script to insert test data into Nexus
# Usage: .\scripts\insert-test-data.ps1

$baseUrl = "http://localhost:15474"

Write-Host "Inserting test data into Nexus..." -ForegroundColor Green

# Create nodes
Write-Host "Creating nodes..." -ForegroundColor Yellow

# Create Person nodes
$persons = @(
    @{ name = "Alice"; age = 30; email = "alice@example.com" },
    @{ name = "Bob"; age = 25; email = "bob@example.com" },
    @{ name = "Charlie"; age = 35; email = "charlie@example.com" },
    @{ name = "Diana"; age = 28; email = "diana@example.com" }
)

foreach ($person in $persons) {
    $body = @{
        labels = @("Person")
        properties = $person
    } | ConvertTo-Json
    
    try {
        $response = Invoke-RestMethod -Uri "$baseUrl/data/nodes" -Method POST -Body $body -ContentType "application/json"
        Write-Host "Created node: $($person.name)" -ForegroundColor Green
    } catch {
        Write-Host "Error creating node $($person.name): $_" -ForegroundColor Red
    }
}

# Create Company nodes
$companies = @(
    @{ name = "Acme Corp"; founded = 2020; industry = "Technology" },
    @{ name = "TechStart"; founded = 2022; industry = "Software" }
)

foreach ($company in $companies) {
    $body = @{
        labels = @("Company")
        properties = $company
    } | ConvertTo-Json
    
    try {
        $response = Invoke-RestMethod -Uri "$baseUrl/data/nodes" -Method POST -Body $body -ContentType "application/json"
        Write-Host "Created company: $($company.name)" -ForegroundColor Green
    } catch {
        Write-Host "Error creating company $($company.name): $_" -ForegroundColor Red
    }
}

# Create relationships using Cypher
Write-Host "Creating relationships..." -ForegroundColor Yellow

$relationships = @(
    "MATCH (a:Person {name: 'Alice'}), (b:Person {name: 'Bob'}) CREATE (a)-[:KNOWS {since: 2020}]->(b)",
    "MATCH (a:Person {name: 'Bob'}), (c:Person {name: 'Charlie'}) CREATE (a)-[:KNOWS {since: 2021}]->(c)",
    "MATCH (a:Person {name: 'Alice'}), (d:Person {name: 'Diana'}) CREATE (a)-[:KNOWS {since: 2019}]->(d)",
    "MATCH (a:Person {name: 'Alice'}), (c:Company {name: 'Acme Corp'}) CREATE (a)-[:WORKS_FOR {role: 'Engineer', since: 2021}]->(c)",
    "MATCH (b:Person {name: 'Bob'}), (c:Company {name: 'TechStart'}) CREATE (b)-[:WORKS_FOR {role: 'Developer', since: 2022}]->(c)"
)

foreach ($query in $relationships) {
    $body = @{
        query = $query
        params = @{}
    } | ConvertTo-Json
    
    try {
        $response = Invoke-RestMethod -Uri "$baseUrl/cypher" -Method POST -Body $body -ContentType "application/json"
        Write-Host "Created relationship" -ForegroundColor Green
    } catch {
        Write-Host "Error creating relationship: $_" -ForegroundColor Red
    }
}

Write-Host "`nTest data inserted successfully!" -ForegroundColor Green
Write-Host "You can now query the data using:" -ForegroundColor Cyan
Write-Host "  MATCH (n) RETURN n LIMIT 10" -ForegroundColor White

