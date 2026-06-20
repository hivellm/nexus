#!/usr/bin/env bash
# Thin wrapper around update-parity.py — §4.1 of
# phase6_bench-neo4j-docker-harness.
#
# Reads a nexus-bench report.json and rewrites the "Benchmark
# Parity" section of docs/compatibility/NEO4J_COMPATIBILITY_REPORT.md
# in place. Safe to invoke from CI or by hand — the Python script
# touches only the region between two HTML markers, so any
# surrounding copy is preserved.
#
# Usage:
#   ./scripts/bench/update-parity.sh <report.json> [doc-path]

set -euo pipefail

HERE="$(cd "$(dirname "$0")" && pwd)"

if ! command -v python3 >/dev/null 2>&1; then
    echo "error: python3 not found on PATH" >&2
    exit 127
fi

exec python3 "$HERE/update-parity.py" "$@"
