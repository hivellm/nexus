# PowerShell script to run complex advanced features tests
# Usage: .\test-complex-runner.ps1

$SERVER_URL = if ($env:SERVER_URL) { $env:SERVER_URL } else { "http://localhost:3000" }
$CYPHER_ENDPOINT = "$SERVER_URL/cypher"

Write-Host "=== Testing Advanced Cypher Features (Complex Scenarios) ===" -ForegroundColor Cyan
Write-Host "Server: $CYPHER_ENDPOINT" -ForegroundColor Yellow
Write-Host ""

function Execute-Query {
    param(
        [string]$Query,
        [string]$Description
    )
    
    Write-Host "Test: $Description" -ForegroundColor Green
    Write-Host "Query: $Query" -ForegroundColor Gray
    
    $body = @{
        query = $Query
    } | ConvertTo-Json
    
    try {
        $response = Invoke-RestMethod -Uri $CYPHER_ENDPOINT -Method Post -Body $body -ContentType "application/json"
        
        Write-Host "Response: $($response | ConvertTo-Json -Depth 10)" -ForegroundColor Gray
        
        if ($response.error) {
            Write-Host "❌ FAILED: $Description" -ForegroundColor Red
            Write-Host ""
            return $false
        } else {
            Write-Host "✅ PASSED: $Description" -ForegroundColor Green
            Write-Host ""
            return $true
        }
    } catch {
        Write-Host "❌ ERROR: $($_.Exception.Message)" -ForegroundColor Red
        Write-Host ""
        return $false
    }
}

# Setup
$setupQuery = @"
CREATE 
  (alice:Person {name: 'Alice', age: 30, scores: [85, 90, 78, 92], city: 'New York'}),
  (bob:Person {name: 'Bob', age: 25, scores: [70, 75, 80], city: 'Boston'}),
  (charlie:Person {name: 'Charlie', age: 35, scores: [95, 88, 91], city: 'New York'}),
  (diana:Person {name: 'Diana', age: 28, scores: [82, 87, 90], city: 'Boston'}),
  (alice)-[:KNOWS {since: 2020}]->(bob),
  (alice)-[:KNOWS {since: 2018}]->(charlie),
  (bob)-[:KNOWS {since: 2021}]->(diana),
  (charlie)-[:KNOWS {since: 2019}]->(diana),
  (alice)-[:WORKS_AT {role: 'Engineer'}]->(:Company {name: 'TechCorp'}),
  (bob)-[:WORKS_AT {role: 'Manager'}]->(:Company {name: 'TechCorp'}),
  (charlie)-[:WORKS_AT {role: 'Director'}]->(:Company {name: 'BigCorp'})
"@

Execute-Query -Query $setupQuery -Description "Setup: Create complex graph"

# Complex tests
$tests = @(
    @{
        Query = "MATCH (p:Person) RETURN p.name, CASE WHEN p.age < 25 THEN 'Junior' WHEN p.age < 30 THEN CASE WHEN p.city = 'New York' THEN 'Mid-Level NYC' ELSE 'Mid-Level' END WHEN p.age < 35 THEN 'Senior' ELSE 'Executive' END AS category LIMIT 5"
        Description = "CASE: Nested CASE expressions"
    },
    @{
        Query = "MATCH (p:Person) RETURN p.name, [x IN p.scores WHERE x >= 85 | [y IN [1, 2, 3] WHERE y <= x / 30 | y * 10]] AS nested_scores LIMIT 3"
        Description = "List Comprehension: Nested comprehensions"
    },
    @{
        Query = "MATCH (p:Person) RETURN p { .name, .age, isSenior: CASE WHEN p.age >= 30 THEN true ELSE false END, scoreAvg: REDUCE(sum = 0, score IN p.scores | sum + score) / SIZE(p.scores) } AS person_info LIMIT 3"
        Description = "Map Projection: Virtual keys with CASE and functions"
    },
    @{
        Query = "MATCH (p:Person {name: 'Alice'}) MATCH (p)-[:KNOWS]->(friend:Person) RETURN p.name, [(p)-[:KNOWS]->(f:Person) WHERE f.age > p.age | f.name] AS older_friends LIMIT 3"
        Description = "Pattern Comprehension: With WHERE clause"
    },
    @{
        Query = "MATCH (p:Person) RETURN p { .name, category: CASE WHEN p.age < 25 THEN 'Junior' WHEN p.age < 30 THEN 'Mid' ELSE 'Senior' END, topScores: [s IN p.scores WHERE s >= 90 | s], scoreCount: SIZE([s IN p.scores WHERE s >= 85]) } AS person_summary LIMIT 3"
        Description = "Combined: CASE + List + Map Projection"
    },
    @{
        Query = "MATCH (p:Person) WHERE EXISTS { (p)-[:KNOWS]->(:Person)-[:KNOWS]->(:Person) } RETURN p.name LIMIT 5"
        Description = "EXISTS: Multi-hop pattern"
    },
    @{
        Query = "MATCH (p:Person {name: 'Alice'}) FOREACH (score IN p.scores | CREATE (s:Score {value: score, person: p.name})) RETURN COUNT(s) AS score_count"
        Description = "FOREACH: Create nodes from list"
    },
    @{
        Query = "MATCH (p:Person) RETURN p.name, [score IN p.scores | CASE WHEN score >= 90 THEN 'A' WHEN score >= 80 THEN 'B' WHEN score >= 70 THEN 'C' ELSE 'D' END] AS grades LIMIT 3"
        Description = "List Comprehension: With CASE transformation"
    },
    @{
        Query = "MATCH (p:Person {name: 'Alice'}) RETURN p.name, [(p)-[:KNOWS]->(f:Person)-[:KNOWS]->(ff:Person) | ff.name] AS friends_of_friends LIMIT 3"
        Description = "Pattern Comprehension: Multi-hop patterns"
    },
    @{
        Query = "MATCH (p:Person) RETURN p { .name, friends: [(p)-[:KNOWS]->(f:Person) | f.name], friendAges: [(p)-[:KNOWS]->(f:Person) | f.age], avgFriendAge: REDUCE(sum = 0, age IN [(p)-[:KNOWS]->(f:Person) | f.age] | sum + age) / SIZE([(p)-[:KNOWS]->(f:Person) | f]) } AS person_network LIMIT 3"
        Description = "Combined: Pattern + List + Map with aggregation"
    }
)

$passed = 0
$failed = 0

foreach ($test in $tests) {
    if (Execute-Query -Query $test.Query -Description $test.Description) {
        $passed++
    } else {
        $failed++
    }
    Start-Sleep -Milliseconds 500
}

Write-Host "=== Test Summary ===" -ForegroundColor Cyan
Write-Host "Passed: $passed" -ForegroundColor Green
Write-Host "Failed: $failed" -ForegroundColor Red
Write-Host "Total: $($passed + $failed)" -ForegroundColor Yellow

