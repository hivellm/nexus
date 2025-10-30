#!/usr/bin/env pwsh
# Test script for fixed data API endpoints (GET/PUT/DELETE /data/nodes)

$baseUrl = "http://127.0.0.1:15474"
$global:testResults = @()

function Test-Endpoint {
    param(
        [string]$Name,
        [string]$Method,
        [string]$Path,
        [hashtable]$Body = $null,
        [string]$Description
    )
    
    Write-Host "`n[TEST] $Name" -ForegroundColor Yellow
    Write-Host "  Desc: $Description" -ForegroundColor Gray
    
    try {
        $uri = "$baseUrl$Path"
        $params = @{
            Uri = $uri
            Method = $Method
            ContentType = "application/json"
            ErrorAction = "Stop"
        }
        
        if ($Body) {
            $params.Body = ($Body | ConvertTo-Json -Compress)
        }
        
        $response = Invoke-RestMethod @params
        
        Write-Host "  Status: ✅ PASS" -ForegroundColor Green
        Write-Host "  Response: $($response | ConvertTo-Json -Compress)" -ForegroundColor Gray
        
        $global:testResults += @{
            Name = $Name
            Status = "PASS"
            Response = $response
        }
        
        return $response
    } catch {
        Write-Host "  Status: ❌ FAIL" -ForegroundColor Red
        Write-Host "  Error: $($_.Exception.Message)" -ForegroundColor Red
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
Write-Host "Testing Fixed Data API Endpoints" -ForegroundColor Cyan
Write-Host "=========================================" -ForegroundColor Cyan

# Step 1: Create a node
Write-Host "`nStep 1: Creating test node..." -ForegroundColor Cyan
$createResponse = Test-Endpoint `
    -Name "CREATE NODE" `
    -Method POST `
    -Path "/data/nodes" `
    -Body @{
        labels = @("TestPerson")
        properties = @{
            name = "Test User"
            age = 25
        }
    } `
    -Description "Create node for testing"

if (-not $createResponse -or -not $createResponse.node_id) {
    Write-Host "❌ Failed to create node. Cannot continue tests." -ForegroundColor Red
    exit 1
}

$nodeId = $createResponse.node_id
Write-Host "  ✅ Created node with ID: $nodeId" -ForegroundColor Green

# Step 2: GET node by ID
Write-Host "`nStep 2: Getting node by ID..." -ForegroundColor Cyan
$getResponse = Test-Endpoint `
    -Name "GET NODE" `
    -Method GET `
    -Path "/data/nodes?id=$nodeId" `
    -Description "Get node by ID"

if ($getResponse -and $getResponse.node) {
    Write-Host "  ✅ Node retrieved:" -ForegroundColor Green
    Write-Host "    - ID: $($getResponse.node.id)" -ForegroundColor White
    Write-Host "    - Labels: $($getResponse.node.labels -join ', ')" -ForegroundColor White
    Write-Host "    - Properties: $($getResponse.node.properties | ConvertTo-Json -Compress)" -ForegroundColor White
} elseif ($getResponse -and $getResponse.error) {
    Write-Host "  ❌ Error retrieving node: $($getResponse.error)" -ForegroundColor Red
}

# Step 3: UPDATE node
Write-Host "`nStep 3: Updating node..." -ForegroundColor Cyan
$updateResponse = Test-Endpoint `
    -Name "UPDATE NODE" `
    -Method PUT `
    -Path "/data/nodes" `
    -Body @{
        node_id = $nodeId
        properties = @{
            name = "Updated User"
            age = 30
            city = "New York"
        }
    } `
    -Description "Update node properties"

if ($updateResponse -and $updateResponse.message -like "*successfully*") {
    Write-Host "  ✅ Update successful!" -ForegroundColor Green
} elseif ($updateResponse -and $updateResponse.error) {
    Write-Host "  ❌ Update failed: $($updateResponse.error)" -ForegroundColor Red
}

# Step 4: Verify update
Write-Host "`nStep 4: Verifying update..." -ForegroundColor Cyan
$verifyResponse = Test-Endpoint `
    -Name "VERIFY UPDATE" `
    -Method GET `
    -Path "/data/nodes?id=$nodeId" `
    -Description "Verify node was updated"

if ($verifyResponse -and $verifyResponse.node) {
    $updated = $verifyResponse.node.properties.name -eq "Updated User"
    if ($updated) {
        Write-Host "  ✅ Update verified! Name: $($verifyResponse.node.properties.name)" -ForegroundColor Green
        Write-Host "    - Age: $($verifyResponse.node.properties.age)" -ForegroundColor White
        Write-Host "    - City: $($verifyResponse.node.properties.city)" -ForegroundColor White
    } else {
        Write-Host "  ⚠️ Update not reflected. Name: $($verifyResponse.node.properties.name)" -ForegroundColor Yellow
    }
}

# Step 5: DELETE node
Write-Host "`nStep 5: Deleting node..." -ForegroundColor Cyan
$deleteResponse = Test-Endpoint `
    -Name "DELETE NODE" `
    -Method DELETE `
    -Path "/data/nodes" `
    -Body @{ node_id = $nodeId } `
    -Description "Delete node"

if ($deleteResponse -and $deleteResponse.message -like "*successfully*") {
    Write-Host "  ✅ Delete successful!" -ForegroundColor Green
} elseif ($deleteResponse -and $deleteResponse.error) {
    Write-Host "  ❌ Delete failed: $($deleteResponse.error)" -ForegroundColor Red
}

# Step 6: Verify delete
Write-Host "`nStep 6: Verifying delete..." -ForegroundColor Cyan
$verifyDeleteResponse = Test-Endpoint `
    -Name "VERIFY DELETE" `
    -Method GET `
    -Path "/data/nodes?id=$nodeId" `
    -Description "Verify node was deleted"

if ($verifyDeleteResponse -and $verifyDeleteResponse.error -like "*not found*") {
    Write-Host "  ✅ Delete verified! Node not found as expected." -ForegroundColor Green
} elseif ($verifyDeleteResponse -and $verifyDeleteResponse.node) {
    Write-Host "  ⚠️ Node still exists after delete!" -ForegroundColor Yellow
} else {
    Write-Host "  ⚠️ Unexpected response: $($verifyDeleteResponse | ConvertTo-Json -Compress)" -ForegroundColor Yellow
}

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
    Write-Host "`n🎉 All tests passed!" -ForegroundColor Green
    exit 0
} else {
    Write-Host "`n⚠️ Some tests failed. Review output above." -ForegroundColor Yellow
    exit 1
}

