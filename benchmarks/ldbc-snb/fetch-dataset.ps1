<#
.SYNOPSIS
    Fetch the pinned LDBC SNB Interactive v1 artifacts for a scale factor.

.DESCRIPTION
    Windows-native twin of fetch-dataset.sh. Both read the same
    dataset-manifest.tsv, so URLs and SHA-256 checksums are pinned in exactly
    one place. Downloads are verified against those checksums and cached
    OUTSIDE the git tree.

    Cache root resolution order: -Cache, $env:LDBC_SNB_CACHE_DIR,
    $HOME\.cache\ldbc-snb

    Cached archives are always re-hashed against the manifest. -Force controls
    re-extraction only; no switch skips checksum verification.

    Layout produced under <cache>\sf<scale>\:
        social_network-sf<scale>-CsvCompositeMergeForeign-LongDateFormatter\
            static\   organisation, place, tag, tagclass
            dynamic\  person, forum, post, comment + the edge files
        substitution_parameters-sf<scale>\     query substitution parameters
        social_network-sf<scale>-numpart-1\    update streams (person + forum)

.EXAMPLE
    .\fetch-dataset.ps1                     # SF0.1 (the smoke scale)

.EXAMPLE
    .\fetch-dataset.ps1 -Scale 1            # SF1 (the reporting scale)

.EXAMPLE
    .\fetch-dataset.ps1 -Scale all -VerifyOnly -NoExtract
#>
[CmdletBinding()]
param(
    [ValidateSet('0.1', '1', 'all')]
    [string]$Scale = '0.1',

    [string]$Cache,

    [switch]$Force,

    [switch]$VerifyOnly,

    [switch]$NoExtract
)

$ErrorActionPreference = 'Stop'
Set-StrictMode -Version Latest

$scriptDir = Split-Path -Parent $MyInvocation.MyCommand.Path
$manifest = Join-Path $scriptDir 'dataset-manifest.tsv'

if (-not (Test-Path $manifest)) {
    throw "manifest not found: $manifest"
}

if (-not $Cache) {
    $Cache = if ($env:LDBC_SNB_CACHE_DIR) {
        $env:LDBC_SNB_CACHE_DIR
    } else {
        Join-Path $HOME '.cache\ldbc-snb'
    }
}

# Extraction needs a zstd decompressor. Prefer the zstd CLI paired with the
# bsdtar that ships with Windows 10+; fall back to Python's zstandard module,
# which is far more commonly installed on Windows than the zstd binary.
function Resolve-Extractor {
    $zstd = Get-Command zstd -ErrorAction SilentlyContinue
    $tar = Get-Command tar -ErrorAction SilentlyContinue
    if ($zstd -and $tar) {
        return @{ Kind = 'tar' }
    }
    foreach ($name in @('python', 'python3')) {
        $py = Get-Command $name -ErrorAction SilentlyContinue
        if (-not $py) { continue }
        & $py.Source -c 'import zstandard' 2>$null
        if ($LASTEXITCODE -eq 0) {
            return @{ Kind = 'python'; Exe = $py.Source }
        }
    }
    throw "no zstd decompressor available. Install the 'zstd' CLI, or 'pip install zstandard'."
}

$pythonExtractor = @'
import sys, tarfile, zstandard

archive, dest = sys.argv[1], sys.argv[2]
with open(archive, "rb") as fh:
    with zstandard.ZstdDecompressor().stream_reader(fh) as reader:
        with tarfile.open(fileobj=reader, mode="r|") as tf:
            # `data` filter rejects absolute paths, `..` traversal and device
            # nodes. Available since Python 3.12; older runtimes fall back to
            # the historical behaviour.
            try:
                tf.extractall(dest, filter="data")
            except TypeError:
                tf.extractall(dest)
'@

function Expand-Archive-Zst {
    param(
        [Parameter(Mandatory)][hashtable]$Extractor,
        [Parameter(Mandatory)][string]$Archive,
        [Parameter(Mandatory)][string]$Destination
    )

    if ($Extractor.Kind -eq 'tar') {
        & tar --use-compress-program=zstd -xf $Archive -C $Destination
        if ($LASTEXITCODE -ne 0) { throw "tar failed to extract $Archive" }
        return
    }

    $tmp = Join-Path ([System.IO.Path]::GetTempPath()) "ldbc-extract-$PID.py"
    try {
        Set-Content -Path $tmp -Value $pythonExtractor -Encoding UTF8
        & $Extractor.Exe $tmp $Archive $Destination
        if ($LASTEXITCODE -ne 0) { throw "python failed to extract $Archive" }
    } finally {
        Remove-Item $tmp -ErrorAction SilentlyContinue
    }
}

$extractor = $null
if (-not $NoExtract) {
    $extractor = Resolve-Extractor
}

$archiveDir = Join-Path $Cache 'archives'
New-Item -ItemType Directory -Force -Path $archiveDir | Out-Null

Write-Host "LDBC SNB Interactive v1 - scale $Scale"
Write-Host "cache root: $Cache"
Write-Host ''

$total = 0
foreach ($line in Get-Content $manifest) {
    if ($line -match '^\s*(#|$)') { continue }

    $fields = $line -split "`t"
    if ($fields.Count -lt 6) {
        throw "malformed manifest line (expected 6 tab-separated fields): $line"
    }
    $mScale, $mKind, $mFile, $mUrl, $mSha, $mBytes = $fields

    if ($Scale -ne 'all' -and $mScale -ne $Scale) { continue }

    $total++
    $archive = Join-Path $archiveDir $mFile
    $targetDir = Join-Path $Cache "sf$mScale"
    New-Item -ItemType Directory -Force -Path $targetDir | Out-Null

    # Cached archives are ALWAYS re-hashed. -Force controls re-extraction
    # only; no flag may skip checksum verification.
    if (Test-Path $archive) {
        $actual = (Get-FileHash -Path $archive -Algorithm SHA256).Hash.ToLowerInvariant()
        if ($actual -eq $mSha) {
            Write-Host "cached   $mFile ($mKind)"
        } else {
            if ($VerifyOnly) {
                throw "checksum mismatch for $mFile (expected $mSha, got $actual)"
            }
            Write-Host "stale    $mFile - checksum mismatch, re-downloading"
            Remove-Item $archive -Force
        }
    }

    if (-not (Test-Path $archive)) {
        if ($VerifyOnly) {
            throw "missing cached archive $mFile (-VerifyOnly)"
        }
        $mb = [int]([int64]$mBytes / 1MB)
        Write-Host "download $mFile ($mb MiB)"
        # Download to a temporary name so an interrupted transfer never leaves
        # a truncated file that later looks cached.
        $part = "$archive.part"
        try {
            # Progress rendering makes Invoke-WebRequest an order of magnitude
            # slower on large downloads; suppressing it is the documented fix.
            $oldProgress = $ProgressPreference
            $ProgressPreference = 'SilentlyContinue'
            Invoke-WebRequest -Uri $mUrl -OutFile $part -MaximumRetryCount 3 -RetryIntervalSec 2
        } finally {
            $ProgressPreference = $oldProgress
        }
        $actual = (Get-FileHash -Path $part -Algorithm SHA256).Hash.ToLowerInvariant()
        if ($actual -ne $mSha) {
            Remove-Item $part -Force -ErrorAction SilentlyContinue
            throw "checksum mismatch for ${mFile}: expected $mSha, got $actual"
        }
        Move-Item -Path $part -Destination $archive -Force
        Write-Host "verified $mFile"
    }

    if (-not $NoExtract) {
        $stem = $mFile -replace '\.tar\.zst$', ''
        $stemPath = Join-Path $targetDir $stem
        if ((Test-Path $stemPath) -and -not $Force) {
            Write-Host "         already extracted -> sf$mScale\$stem"
        } else {
            if (Test-Path $stemPath) { Remove-Item $stemPath -Recurse -Force }
            Write-Host "         extracting -> sf$mScale\$stem"
            Expand-Archive-Zst -Extractor $extractor -Archive $archive -Destination $targetDir
        }
    }
}

if ($total -eq 0) {
    throw "no manifest entries matched scale '$Scale'"
}

Write-Host ''
Write-Host "done - $total artifact(s) ready under $Cache"
Write-Host "`$env:LDBC_SNB_CACHE_DIR = '$Cache'"
