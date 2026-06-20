<#
.SYNOPSIS
    Bound the size of Cargo's `target/` directory (GH #24).

.DESCRIPTION
    Cargo never garbage-collects `target/`: stale object files, incremental
    caches, and rlibs from old dependency versions accumulate indefinitely.
    This script wraps `cargo-sweep` to delete artifacts that have not been
    accessed in N days WITHOUT breaking incrementality — the hot set you're
    actively rebuilding keeps its mtime and survives the sweep.

    cargo-sweep is auto-installed (`cargo install cargo-sweep`) if missing.

    See docs/development/rust-target-hygiene.md for the full hygiene policy.

.PARAMETER Days
    Retention window in days (default 14). Artifacts not accessed within the
    window are removed.

.PARAMETER Clean
    Run a full `cargo clean` (reclaim everything) instead of a sweep.

.PARAMETER DryRun
    Show what would be removed without deleting anything.

.EXAMPLE
    scripts\sweep-target.ps1
    Sweep artifacts older than 14 days.

.EXAMPLE
    scripts\sweep-target.ps1 -Days 30

.EXAMPLE
    scripts\sweep-target.ps1 -Clean
#>
[CmdletBinding()]
param(
    [int]$Days = 14,
    [switch]$Clean,
    [switch]$DryRun
)

$ErrorActionPreference = 'Stop'
Set-Location (Join-Path $PSScriptRoot '..')

if ($Clean) {
    Write-Host '==> cargo clean (full reclaim)'
    cargo clean
    Write-Host '==> done. target/ fully removed; next build is cold.'
    return
}

if (-not (Get-Command cargo-sweep -ErrorAction SilentlyContinue)) {
    Write-Host '==> cargo-sweep not found; installing (one-time)...'
    cargo install cargo-sweep
}

$before = $null
if (Test-Path target) {
    $before = (Get-ChildItem target -Recurse -File -ErrorAction SilentlyContinue |
        Measure-Object -Property Length -Sum).Sum
}

$sweepArgs = @('sweep', '--time', "$Days")
if ($DryRun) { $sweepArgs += '--dry-run' }
Write-Host "==> cargo $($sweepArgs -join ' ')"
& cargo @sweepArgs

if ((-not $DryRun) -and ($null -ne $before) -and (Test-Path target)) {
    $after = (Get-ChildItem target -Recurse -File -ErrorAction SilentlyContinue |
        Measure-Object -Property Length -Sum).Sum
    $beforeGb = [math]::Round($before / 1GB, 2)
    $afterGb = [math]::Round($after / 1GB, 2)
    Write-Host "==> target/ size: ${beforeGb} GB -> ${afterGb} GB"
}
Write-Host '==> done.'
