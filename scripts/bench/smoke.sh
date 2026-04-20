#!/usr/bin/env bash
# End-to-end smoke for the bench docker-compose — §1.5 of
# phase6_bench-neo4j-docker-harness. Exercises the three
# lifecycle moving parts in sequence:
#
#   1. `neo4j-up.sh`   — compose up + readiness poll
#   2. Bolt PING       — `cypher-shell RETURN 1` inside the
#                        container (no extra host tooling)
#   3. `neo4j-down.sh` — compose down + volume drop
#
# Total wall time is expected to stay comfortably under 30 s on a
# warm image pull; on a cold pull the first run is dominated by
# the image download and a hard 30 s bound doesn't hold. The
# script therefore reports the elapsed time at the end and exits
# non-zero if it drifts past 60 s (a loose guard against a
# silently wedged container), leaving the 30 s target to CI.

set -euo pipefail

HERE="$(cd "$(dirname "$0")" && pwd)"
CONTAINER="nexus-bench-neo4j"

# Always tear down, even on failure. Without this the next run
# sees a stuck container and neo4j-up.sh's "already running"
# fast-path hides a broken state.
cleanup() {
    "$HERE/neo4j-down.sh" >/dev/null 2>&1 || true
}
trap cleanup EXIT

start=$(date +%s)

echo "[1/3] bringing Neo4j up..."
"$HERE/neo4j-up.sh"

echo "[2/3] bolt PING via cypher-shell (inside the container)..."
# `NEO4J_AUTH=none` in the compose file means cypher-shell does
# not need credentials; `--format plain --non-interactive` keeps
# the output a single line per row so the PING cannot hang on a
# prompt. Wrapped in `timeout` so a stuck handshake never
# extends this past the harness's safety window.
timeout 15 docker exec "$CONTAINER" \
    cypher-shell -a bolt://localhost:7687 --format plain --non-interactive "RETURN 1 AS n;"

echo "[3/3] tearing Neo4j down..."
"$HERE/neo4j-down.sh"

# Clear the trap — cleanup already ran cleanly.
trap - EXIT

elapsed=$(( $(date +%s) - start ))
echo "smoke OK (${elapsed}s)"

if [ "$elapsed" -gt 60 ]; then
    echo "!! smoke drifted past 60 s — investigate (target is <=30 s on a warm image)" >&2
    exit 2
fi
