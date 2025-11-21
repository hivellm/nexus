# Test predicate parsing
# This will help us understand if the parser is correctly parsing predicates

Write-Host "Testing predicate: n.name = 'Alice'"
$testPredicate = "n.name = 'Alice'"

# We can't directly test the parser, but we can test if the query works
$body = @{
    query = "MATCH (n:Person) WHERE n.name = 'Alice' RETURN n.name AS name"
} | ConvertTo-Json -Depth 10

Write-Host "Query: MATCH (n:Person) WHERE n.name = 'Alice' RETURN n.name AS name"
Write-Host ""

# Note: This requires the server to be running
# For now, we'll just document what we're testing

