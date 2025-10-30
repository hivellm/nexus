#!/usr/bin/env pwsh
# Test Nexus integration similar to classify Neo4j tests
# Tests CREATE, MERGE, and relationship creation like classify does

$baseUrl = "http://127.0.0.1:15474"
$global:testResults = @()

function Test-Cypher {
    param(
        [string]$Name,
        [string]$Query,
        [string]$Description
    )
    
    Write-Host "`n[TEST] $Name" -ForegroundColor Yellow
    Write-Host "  Desc: $Description" -ForegroundColor Gray
    Write-Host "  Query: $Query" -ForegroundColor DarkGray
    
    try {
        $body = @{
            query = $Query
        } | ConvertTo-Json
        
        $response = Invoke-RestMethod -Uri "$baseUrl/cypher" -Method POST -Body $body -ContentType "application/json" -ErrorAction Stop
        
        Write-Host "  Status: ✅ PASS" -ForegroundColor Green
        if ($response.rows -and $response.rows.Count -gt 0) {
            Write-Host "  Rows: $($response.rows.Count)" -ForegroundColor Cyan
        }
        if ($response.execution_time_ms) {
            Write-Host "  Time: $($response.execution_time_ms)ms" -ForegroundColor Cyan
        }
        
        $global:testResults += @{
            Name = $Name
            Status = "PASS"
            Response = $response
        }
        
        return $response
    } catch {
        Write-Host "  Status: ❌ FAIL" -ForegroundColor Red
        Write-Host "  Error: $($_.Exception.Message)" -ForegroundColor timelinesRed
        if ($_.ErrorDetails.Message) {
            Write-Host "  Details: $($_.ErrorDetails.Message)" -ForegroundColor Yellow
        }
        
        $global:testResults += @{
            Name = $Name
            Status = "FAIL"
            Error = $_.Exception.Message
        }
        
        return $null
    }
}

Write-Host "=========================================" -ForegroundColor Cyan
Write-Host "Nexus Classify Integration Test" -ForegroundColor Cyan
Write-Host "Similar to classify Neo4j tests" -ForegroundColor Cyan
Write-Host "=========================================" -ForegroundColor Cyan

# Step 1: Create Document node (similar to classify)
Write-Host "`nStep 1: Creating Document node..." -ForegroundColor Cyan
$docHash = "abc123def456"
$docCypher = @"
CREATE (doc:Document {
  file_hash: "$docHash",
  source_file: "test/document.ts",
  classified_at: "2025-10-29T00:00:00Z",
  id: "Test Document",
  title: "Test Document",
  domain: "software",
  doc_type: "code_documentation"
})
RETURN doc
"@.Trim()

Test-Cypher -Name "CREATE DOCUMENT" -Query $docCypher -Description "Create Document node like classify"

# Step 2: Create Entity nodes (Module, Function, etc.)
Write-Host "`nStep 2: Creating Entity nodes..." -ForegroundColor Cyan
$moduleCypher = @"
CREATE (e0:Module {name: "pg", description: "PostgreSQL client"})
RETURN e0
"@.Trim()

Test-Cypher -Name "CREATE MODULE" -Query $moduleCypher -Description "Create Module entity"

$functionCypher = @"
CREATE (e1:Function {name: "connect", description: "Connect to database"})
RETURN e1
"@.Trim()

Test-Cypher -Name "CREATE FUNCTION" -Query $functionCypher -Description "Create Function entity"

# Step 3: Create relationships
Write-Host "`nStep 3: Creating relationships..." -ForegroundColor Cyan
$relCypher = @"
MATCH (doc:Document {file_hash: "$docHash"})
MATCH (e0:Module {name: "pg"})
CREATE (doc)-[:MENTIONS]->(e0)
RETURN doc, e0
"@.Trim()

Test-Cypher -Name "CREATE RELATIONSHIP" -Query $relCypher -Description "Create MENTIONS relationship"

# Step 4: Test MERGE (avoid duplicates)
Write-Host "`nStep 4: Testing MERGE (avoid duplicates)..." -ForegroundColor Cyan
$mergeCypher = @"
MERGE (doc:Document {file_hash: "$docHash"})
RETURN doc
"@.Trim()

Test-Cypher -Name "MERGE DOCUMENT" -Query $mergeCypher -Description "MERGE existing document (should not create duplicate)"

# Step 5: Query all documents
Write-Host "`nStep 5: Querying all documents..." -ForegroundColor Cyan
Test-Cypher -Name "MATCH DOCUMENTS" -Query "MATCH (d:Document) RETURN d LIMIT 10" -Description "Find all documents"

# Step 6: Query relationships
Write-Host "`nStep 6: Querying relationships..." -ForegroundColor Cyan
$queryRelCypher = @"
MATCH (doc:Document)-[r:MENTIONS]->(entity)
RETURN doc, r, entity
LIMIT 10
"@.Trim()

Test-Cypher -Name "MATCH RELATIONSHIPS" -Query $queryRelCypher -Description "Find Document-Entity relationships"

# Step 7: Complex query (like classify would use)
Write-Host "`nStep 7: Complex classification query..." -ForegroundColor Cyan
$complexCypher = @"
MATCH (doc:Document {domain: "software"})
MATCH (doc)-[:MENTIONS]->(entity)
WHERE entity.name = "pg"
RETURN doc.title, entity.name, entity.description
LIMIT 10
"@.Trim()

Test-Cypher -Name "COMPLEX QUERY" -Query $complexCypher -Description "Complex query similar to classify usage"

# Summary
Write-Host "`n=========================================" -ForegroundColor Cyan
Write-Host "Test Summary" -ForegroundColor Cyan
Write-Host "=========================================" -ForegroundColor Cyan

$passed = ($global:testResults | Where-Object { $_.Status -eq "PASS" }).Count
$failed = ($global:testResults | Where-Object { $_.Status -eq "FAIL" }).Count
$total = $global:testResults.Count

Write-Host "Total tests: $total" -ForegroundColor White
Write-Host "Passed: $passed ✅" -ForegroundColor Green
Write-Host "Failed: $failed ❌" -ForegroundColor $(if ($failed -eq 0) { "Green" } else { "Red" })

Write-Host "`nDetailed results:" -ForegroundColor Cyan
foreach ($result in $global:testResults) {
    $status = if ($result.Status -eq "PASS") { "✅" } else { "❌" }
    Write-Host "  $status $($result.Name)" -ForegroundColor $(if ($result.Status -eq "PASS") { "Green" } else { "Red" })
}

if ($failed -eq 0) {
    Write-Host "`n🎉 All tests passed! Nexus is ready for classify integration!" -ForegroundColor Green
    Write-Host "`nNext steps:" -ForegroundColor Cyan
    Write-Host "  1. Update classify to support Nexus URL (similar to Neo4j)" -ForegroundColor White
    Write-Host "  2. Use POST /cypher endpoint instead of Neo4j transaction API" -ForegroundColor White
    Write-Host "  3. Test with real classify output`n" -ForegroundColor White
    exit 0
} else {
    Write-Host "`n⚠️ Some tests failed. Review output above." -ForegroundColor Yellow
    exit 1
}

