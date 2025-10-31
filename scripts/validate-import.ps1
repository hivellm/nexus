#!/usr/bin/env pwsh
# Script to validate imported data matches expected structure
# Validates node/relationship types, property mappings, and optionally compares with Neo4j

$ErrorActionPreference = "Stop"

$NexusUrl = if ($env:NEXUS_URL) { $env:NEXUS_URL } else { "http://127.0.0.1:15474" }
$script:Neo4jUrl = if ($env:NEO4J_URL) { $env:NEO4J_URL } else { "http://127.0.0.1:7474" }
$script:Neo4jUser = if ($env:NEO4J_USER) { $env:NEO4J_USER } else { "neo4j" }
$script:Neo4jPass = if ($env:NEO4J_PASS) { $env:NEO4J_PASS } else { "password" }
$script:Neo4jAvailable = $false

$expectedNodeTypes = @(
    "Document",
    "Module",
    "Class",
    "Function",
    "Interface",
    "Type",
    "Variable",
    "Import",
    "Entity"
)

$expectedRelTypes = @(
    "MENTIONS",
    "IMPORTS",
    "HAS",
    "CONTAINS",
    "EXTENDS",
    "IMPLEMENTS",
    "CALLS",
    "REFERENCES"
)

$errors = @()
$warnings = @()

function Invoke-NexusQuery {
    param(
        [Parameter(Mandatory = $true)][string]$Query,
        [int]$TimeoutSec = 60
    )

    $body = @{ query = $Query } | ConvertTo-Json -Depth 10
    return Invoke-RestMethod -Uri "$NexusUrl/cypher" -Method POST -Body $body -ContentType "application/json" -TimeoutSec $TimeoutSec
}

function Get-RowValues {
    param($Row)

    if ($Row -is [System.Array]) {
        return $Row
    }

    if ($Row -is [PSCustomObject]) {
        return @($Row.PSObject.Properties | ForEach-Object { $_.Value })
    }

    if ($Row -is [System.Collections.IEnumerable] -and -not ($Row -is [string])) {
        $values = @()
        foreach ($item in $Row) {
            $values += $item
        }
        return $values
    }

    if ($null -eq $Row) {
        return @()
    }

    return @($Row)
}

function Invoke-Neo4jQueryInternal {
    param(
        [Parameter(Mandatory = $true)][string]$Query,
        [switch]$Quiet
    )

    $authBytes = [Text.Encoding]::ASCII.GetBytes("$script:Neo4jUser:$script:Neo4jPass")
    $auth = [Convert]::ToBase64String($authBytes)
    $headers = @{ "Authorization" = "Basic $auth"; "Content-Type" = "application/json" }
    $body = @{ statements = @(@{ statement = $Query }) } | ConvertTo-Json -Depth 10

    try {
        return Invoke-RestMethod -Uri "$script:Neo4jUrl/db/neo4j/tx/commit" -Method POST -Headers $headers -Body $body -TimeoutSec 60
    } catch {
        if ($Quiet) {
            return $null
        }
        throw $_
    }
}

function Get-Neo4jRows {
    param($Response)

    $rows = @()
    if ($Response -and $Response.results -and $Response.results.Count -gt 0) {
        foreach ($record in $Response.results[0].data) {
            if ($record.row) {
                $rows += ,$record.row
            }
        }
    }
    return $rows
}

Write-Host "=== Import Validation Script ===" -ForegroundColor Cyan
Write-Host "Validating imported data structure`n" -ForegroundColor Yellow

# 1. Check Nexus server health
Write-Host "1. Checking Nexus server health..." -ForegroundColor Yellow
try {
    $health = Invoke-RestMethod -Uri "$NexusUrl/health" -TimeoutSec 10
    Write-Host "   [OK] Nexus server is online" -ForegroundColor Green
} catch {
    Write-Host "   [ERROR] Nexus server is offline: $_" -ForegroundColor Red
    exit 1
}

# 2. Get Nexus stats
Write-Host "`n2. Getting Nexus database stats..." -ForegroundColor Yellow
try {
    $stats = Invoke-RestMethod -Uri "$NexusUrl/stats" -TimeoutSec 10

    $nodeCount = if ($stats.catalog.node_count) { $stats.catalog.node_count } else { 0 }
    $relCount = if ($stats.catalog.rel_count) { $stats.catalog.rel_count } else { 0 }
    $labelCount = if ($stats.catalog.label_count) { $stats.catalog.label_count } else { 0 }
    $typeCount = if ($stats.catalog.type_count) { $stats.catalog.type_count } else { 0 }

    Write-Host "   Nodes: $nodeCount" -ForegroundColor $(if ($nodeCount -gt 1000) { "Green" } else { "Yellow" })
    Write-Host "   Relationships: $relCount" -ForegroundColor $(if ($relCount -gt 1000) { "Green" } else { "Yellow" })
    Write-Host "   Labels: $labelCount" -ForegroundColor Green
    Write-Host "   Relationship Types: $typeCount" -ForegroundColor Green

    if ($nodeCount -lt 1000) {
        $warnings += "Node count ($nodeCount) is lower than expected (~11,132)"
    }
    if ($relCount -lt 1000) {
        $warnings += "Relationship count ($relCount) is lower than expected (~3,640)"
    }
} catch {
    Write-Host "   [ERROR] Failed to get Nexus stats: $_" -ForegroundColor Red
    $errors += "Failed to retrieve Nexus database stats"
}

# 3. Check Neo4j availability (optional)
Write-Host "`n3. Checking Neo4j availability for comparison..." -ForegroundColor Yellow
$neoTest = Invoke-Neo4jQueryInternal -Query "RETURN 1" -Quiet
if ($neoTest -ne $null) {
    $script:Neo4jAvailable = $true
    Write-Host "   [OK] Neo4j comparison enabled" -ForegroundColor Green
} else {
    Write-Host "   [INFO] Neo4j comparison disabled (set NEO4J_URL/NEO4J_USER/NEO4J_PASS and ensure Neo4j is running to enable)" -ForegroundColor DarkYellow
}

# 4. Verify node types
Write-Host "`n4. Verifying node types..." -ForegroundColor Yellow
$foundNodeTypes = @()

foreach ($nodeType in $expectedNodeTypes) {
    try {
        $query = "MATCH (n:$nodeType) RETURN count(n) AS count"
        $result = Invoke-NexusQuery -Query $query

        $count = 0
        if ($result.rows -and $result.rows.Count -gt 0) {
            # Direct access to first row, first column
            $firstRow = $result.rows[0]
            if ($firstRow -is [System.Array] -and $firstRow.Length -gt 0) {
                $count = [int]$firstRow[0]
            } elseif ($firstRow -is [PSCustomObject]) {
                $count = [int]($firstRow.PSObject.Properties.Value | Select-Object -First 1)
            } else {
                $count = [int]$firstRow
            }
        }

        if ($count -gt 0) {
            $foundNodeTypes += $nodeType
            Write-Host "   [OK] $nodeType : $count nodes" -ForegroundColor Green
        } else {
            Write-Host "   [WARN] $nodeType : 0 nodes" -ForegroundColor Yellow
            $warnings += "Node type '$nodeType' has no nodes"
        }
    } catch {
        Write-Host "   [ERROR] Failed to verify node type '$nodeType': $_" -ForegroundColor Red
        $errors += "Failed to verify node type '$nodeType'"
    }
}

$missingNodeTypes = $expectedNodeTypes | Where-Object { $foundNodeTypes -notcontains $_ }
if ($missingNodeTypes.Count -gt 0) {
    Write-Host "   Missing node types:" -ForegroundColor Red
    foreach ($missing in $missingNodeTypes) {
        Write-Host "      - $missing" -ForegroundColor Red
        $errors += "Missing node type: $missing"
    }
}

# 5. Verify relationship types
Write-Host "`n5. Verifying relationship types..." -ForegroundColor Yellow
$foundRelTypes = @()

foreach ($relType in $expectedRelTypes) {
    try {
        $query = "MATCH ()-[r:$relType]->() RETURN count(r) AS count"
        $result = Invoke-NexusQuery -Query $query

        $count = 0
        if ($result.rows -and $result.rows.Count -gt 0) {
            # Direct access to first row, first column
            $firstRow = $result.rows[0]
            if ($firstRow -is [System.Array] -and $firstRow.Length -gt 0) {
                $count = [int]$firstRow[0]
            } elseif ($firstRow -is [PSCustomObject]) {
                $count = [int]($firstRow.PSObject.Properties.Value | Select-Object -First 1)
            } else {
                $count = [int]$firstRow
            }
        }

        if ($count -gt 0) {
            $foundRelTypes += $relType
            Write-Host "   [OK] $relType : $count relationships" -ForegroundColor Green
        } else {
            Write-Host "   [WARN] $relType : 0 relationships" -ForegroundColor Yellow
            $warnings += "Relationship type '$relType' has no relationships"
        }
    } catch {
        Write-Host "   [ERROR] Failed to verify relationship type '$relType': $_" -ForegroundColor Red
        $errors += "Failed to verify relationship type '$relType'"
    }
}

$missingRelTypes = $expectedRelTypes | Where-Object { $foundRelTypes -notcontains $_ }
if ($missingRelTypes.Count -gt 0) {
    Write-Host "   Missing relationship types:" -ForegroundColor Red
    foreach ($missing in $missingRelTypes) {
        Write-Host "      - $missing" -ForegroundColor Red
        $errors += "Missing relationship type: $missing"
    }
}

# 6. Verify node property mappings
Write-Host "`n6. Verifying node property mappings..." -ForegroundColor Yellow
$nodePropertyMap = @{}

foreach ($label in $expectedNodeTypes) {
    try {
        $query = "MATCH (n:`$label) WITH n LIMIT 250 UNWIND keys(n) AS key RETURN DISTINCT key ORDER BY key"
        $result = Invoke-NexusQuery -Query $query

        $keys = @()
        if ($result.rows) {
            foreach ($row in $result.rows) {
                $values = Get-RowValues $row
                if ($values.Count -gt 0 -and $null -ne $values[0] -and $values[0] -ne "") {
                    $keys += [string]$values[0]
                }
            }
        }
        $keys = $keys | Sort-Object -Unique
        $nodePropertyMap[$label] = $keys

        if ($keys.Count -gt 0) {
            Write-Host "   [OK] $label : $($keys.Count) properties" -ForegroundColor Green
        } else {
            Write-Host "   [WARN] $label : no properties found" -ForegroundColor Yellow
            $warnings += "No properties found for label '$label' in Nexus"
        }

        if ($script:Neo4jAvailable) {
            $neoQuery = "MATCH (n:`$label) WITH n LIMIT 250 UNWIND keys(n) AS key RETURN DISTINCT key ORDER BY key"
            $neoResponse = Invoke-Neo4jQueryInternal -Query $neoQuery -Quiet
            if ($neoResponse -ne $null) {
                $neoRows = Get-Neo4jRows $neoResponse
                $neoKeys = @()
                foreach ($row in $neoRows) {
                    $values = Get-RowValues $row
                    if ($values.Count -gt 0 -and $null -ne $values[0] -and $values[0] -ne "") {
                        $neoKeys += [string]$values[0]
                    }
                }
                $neoKeys = $neoKeys | Sort-Object -Unique

                $onlyInNexus = $keys | Where-Object { $_ -notin $neoKeys }
                $onlyInNeo = $neoKeys | Where-Object { $_ -notin $keys }

                if (($onlyInNexus.Count -eq 0) -and ($onlyInNeo.Count -eq 0)) {
                    Write-Host "      [MATCH] Property mapping matches Neo4j" -ForegroundColor Green
                } else {
                    Write-Host "      [DIFF] Property mismatch detected" -ForegroundColor Red
                    if ($onlyInNexus.Count -gt 0) {
                        Write-Host "         Only in Nexus: $($onlyInNexus -join ', ')" -ForegroundColor Red
                    }
                    if ($onlyInNeo.Count -gt 0) {
                        Write-Host "         Only in Neo4j: $($onlyInNeo -join ', ')" -ForegroundColor Red
                    }
                    $errors += "Property mismatch for label '$label' between Nexus and Neo4j"
                }
            } else {
                Write-Host "      [WARN] Unable to retrieve Neo4j properties for '$label'" -ForegroundColor Yellow
                $warnings += "Unable to retrieve Neo4j properties for label '$label'"
            }
        }
    } catch {
        Write-Host "   [ERROR] Failed to verify properties for label '$label': $_" -ForegroundColor Red
        $errors += "Failed to verify properties for label '$label'"
    }
}

# 7. Verify relationship property mappings
Write-Host "`n7. Verifying relationship property mappings..." -ForegroundColor Yellow
foreach ($relType in $expectedRelTypes) {
    try {
        $query = "MATCH ()-[r:`$relType]->() WITH r LIMIT 250 UNWIND keys(r) AS key RETURN DISTINCT key ORDER BY key"
        $result = Invoke-NexusQuery -Query $query

        $keys = @()
        if ($result.rows) {
            foreach ($row in $result.rows) {
                $values = Get-RowValues $row
                if ($values.Count -gt 0 -and $null -ne $values[0] -and $values[0] -ne "") {
                    $keys += [string]$values[0]
                }
            }
        }
        $keys = $keys | Sort-Object -Unique

        if ($keys.Count -gt 0) {
            Write-Host "   [OK] $relType : $($keys.Count) properties" -ForegroundColor Green
        } else {
            Write-Host "   [INFO] $relType : no properties found" -ForegroundColor DarkGray
        }

        if ($script:Neo4jAvailable) {
            $neoQuery = "MATCH ()-[r:`$relType]->() WITH r LIMIT 250 UNWIND keys(r) AS key RETURN DISTINCT key ORDER BY key"
            $neoResponse = Invoke-Neo4jQueryInternal -Query $neoQuery -Quiet
            if ($neoResponse -ne $null) {
                $neoRows = Get-Neo4jRows $neoResponse
                $neoKeys = @()
                foreach ($row in $neoRows) {
                    $values = Get-RowValues $row
                    if ($values.Count -gt 0 -and $null -ne $values[0] -and $values[0] -ne "") {
                        $neoKeys += [string]$values[0]
                    }
                }
                $neoKeys = $neoKeys | Sort-Object -Unique

                $onlyInNexus = $keys | Where-Object { $_ -notin $neoKeys }
                $onlyInNeo = $neoKeys | Where-Object { $_ -notin $keys }

                if (($onlyInNexus.Count -eq 0) -and ($onlyInNeo.Count -eq 0)) {
                    Write-Host "      [MATCH] Relationship properties match Neo4j" -ForegroundColor Green
                } elseif (($keys.Count -eq 0) -and ($neoKeys.Count -eq 0)) {
                    Write-Host "      [OK] No relationship properties on either system" -ForegroundColor Green
                } else {
                    Write-Host "      [DIFF] Relationship property mismatch detected" -ForegroundColor Red
                    if ($onlyInNexus.Count -gt 0) {
                        Write-Host "         Only in Nexus: $($onlyInNexus -join ', ')" -ForegroundColor Red
                    }
                    if ($onlyInNeo.Count -gt 0) {
                        Write-Host "         Only in Neo4j: $($onlyInNeo -join ', ')" -ForegroundColor Red
                    }
                    $errors += "Relationship property mismatch for type '$relType' between Nexus and Neo4j"
                }
            } else {
                Write-Host "      [WARN] Unable to retrieve Neo4j relationship properties for '$relType'" -ForegroundColor Yellow
                $warnings += "Unable to retrieve Neo4j relationship properties for '$relType'"
            }
        }
    } catch {
        Write-Host "   [ERROR] Failed to verify properties for relationship '$relType': $_" -ForegroundColor Red
        $errors += "Failed to verify properties for relationship '$relType'"
    }
}

# Summary
Write-Host "`n=== Validation Summary ===" -ForegroundColor Cyan

if ($errors.Count -eq 0 -and $warnings.Count -eq 0) {
    Write-Host "✅ All validations passed!" -ForegroundColor Green
    exit 0
} elseif ($errors.Count -eq 0) {
    Write-Host "⚠️  Validation completed with warnings:" -ForegroundColor Yellow
    foreach ($warningMsg in $warnings | Sort-Object -Unique) {
        Write-Host "   - $warningMsg" -ForegroundColor Yellow
    }
    exit 0
} else {
    Write-Host "❌ Validation failed with errors:" -ForegroundColor Red
    foreach ($errorMsg in $errors | Sort-Object -Unique) {
        Write-Host "   - $errorMsg" -ForegroundColor Red
    }
    if ($warnings.Count -gt 0) {
        Write-Host "`nWarnings:" -ForegroundColor Yellow
        foreach ($warningMsg in $warnings | Sort-Object -Unique) {
            Write-Host "   - $warningMsg" -ForegroundColor Yellow
        }
    }
    exit 1
}

