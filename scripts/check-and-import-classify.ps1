$nexusUrl = "http://127.0.0.1:15474"

Write-Host "=== Check and Import Classify Data ===" -ForegroundColor Cyan

# Check current status
Write-Host "`nChecking current Nexus database status..." -ForegroundColor Yellow
try {
    $stats = Invoke-RestMethod -Uri "$nexusUrl/stats"
    $nodeCount = $stats.catalog.node_count
    $relCount = $stats.catalog.rel_count
    
    Write-Host "  Current nodes: $nodeCount" -ForegroundColor $(if ($nodeCount -gt 1000) { "Green" } else { "Yellow" })
    Write-Host "  Current relationships: $relCount" -ForegroundColor $(if ($relCount -gt 1000) { "Green" } else { "Yellow" })
    
    if ($nodeCount -lt 1000) {
        Write-Host "`nWARNING: Database appears to have insufficient data!" -ForegroundColor Red
        Write-Host "Expected: ~11,132 nodes and ~3,640 relationships" -ForegroundColor Yellow
        Write-Host "`nTo import classify data, run:" -ForegroundColor Cyan
        Write-Host "  cd classify" -ForegroundColor White
        Write-Host "  npx tsx ../nexus/scripts/import-classify-to-nexus.ts" -ForegroundColor White
        Write-Host "`nOr from nexus directory:" -ForegroundColor Gray
        Write-Host "  cd ../classify && npx tsx ../nexus/scripts/import-classify-to-nexus.ts" -ForegroundColor White
    } else {
        Write-Host "`nDatabase appears to have sufficient data!" -ForegroundColor Green
        Write-Host "You can now run comprehensive comparison tests." -ForegroundColor Green
    }
} catch {
    Write-Host "ERROR: Failed to get stats - $_" -ForegroundColor Red
    Write-Host "`nCheck if Nexus server is running:" -ForegroundColor Yellow
    Write-Host "  cd nexus-server && cargo run --release" -ForegroundColor White
}

Write-Host ""




















