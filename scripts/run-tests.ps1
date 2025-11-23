# Simple wrapper to run compatibility tests
$ErrorActionPreference = "Continue"
cd "$PSScriptRoot\.."
& ".\scripts\test-neo4j-nexus-compatibility-200.ps1"
