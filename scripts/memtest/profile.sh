#!/usr/bin/env bash
# Memory profiling workflow against a running nexus-memtest container.
#
# Prerequisites:
#   - Container was built via docker-compose.memtest.yml (Dockerfile.memtest
#     compiles with `--features memory-profiling` and sets MALLOC_CONF so
#     jemalloc heap profiling is on from process start).
#   - The /output directory inside the container is bind-mounted to
#     ./memtest-output/heap on the host.
#
# Usage:
#   bash scripts/memtest/profile.sh baseline   # capture an idle snapshot
#   bash scripts/memtest/profile.sh loaded     # capture after you ran load

set -euo pipefail

TAG="${1:?tag required (e.g. baseline, loaded)}"
HOST="${HOST:-http://localhost:15474}"
OUT_DIR="$(cd "$(dirname "$0")/../.." && pwd)/memtest-output/heap"
mkdir -p "$OUT_DIR"

log() { echo "[profile/${TAG}] $*"; }

# 1. Current allocator stats (JSON).
log "GET /debug/memory"
STATS=$(curl -sf "${HOST}/debug/memory" || echo '{}')
echo "$STATS" | jq . > "${OUT_DIR}/${TAG}-stats.json" 2>/dev/null || echo "$STATS" > "${OUT_DIR}/${TAG}-stats.json"
echo "$STATS" | jq -r '.mib // .error' 2>/dev/null || echo "$STATS"

# 2. Trigger a heap profile dump.
log "POST /debug/heap/dump"
DUMP=$(curl -sf -X POST "${HOST}/debug/heap/dump" || echo '{}')
echo "$DUMP"

# 3. List the new .heap files produced under the mounted output dir.
log "latest heap files on host:"
ls -lth "${OUT_DIR}"/*.heap 2>/dev/null | head -5 || log "no .heap files yet — is MALLOC_CONF.prof_active=true?"

cat <<EOF

[profile/${TAG}] done. To turn a .heap file into something browsable:

  # Pull the matching nexus-server binary out of the container
  docker cp nexus-memtest:/usr/local/bin/nexus-server ${OUT_DIR}/nexus-server

  # Generate an SVG callgraph (requires jeprof from libjemalloc-dev)
  jeprof --svg ${OUT_DIR}/nexus-server ${OUT_DIR}/jeprof.<PID>.<N>.f.heap > ${OUT_DIR}/${TAG}.svg

  # Or diff two snapshots to see what grew between them
  jeprof --base ${OUT_DIR}/jeprof.<PID>.<A>.f.heap \\
         --svg  ${OUT_DIR}/nexus-server ${OUT_DIR}/jeprof.<PID>.<B>.f.heap > diff.svg
EOF
