#!/usr/bin/env bash
# Orchestrator: boots container, runs each load scenario with parallel
# measurement, collects CSVs tagged with the phase label.
#
# Usage: ./run-all.sh <tag>
#   tag: baseline, phase0, phase1, phase1.1, phase2, etc.
#
# Prereqs: docker + docker compose available.

set -euo pipefail

TAG="${1:?tag required}"
SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
REPO_DIR="$(cd "${SCRIPT_DIR}/../.." && pwd)"
COMPOSE_FILE="${REPO_DIR}/docker-compose.memtest.yml"
OUT_DIR="${REPO_DIR}/memtest-output"
mkdir -p "$OUT_DIR"

cd "$REPO_DIR"

log() { echo "[run-all/${TAG}] $*"; }

cleanup() {
    log "stopping container"
    docker compose -f "$COMPOSE_FILE" down -v >/dev/null 2>&1 || true
    # Kill any lingering measure loops
    [ -n "${MEASURE_PID:-}" ] && kill "$MEASURE_PID" 2>/dev/null || true
}
trap cleanup EXIT

run_scenario() {
    local name="$1"
    local script="$2"
    local duration="$3"

    log "scenario=${name} duration=${duration}s"
    bash "${SCRIPT_DIR}/measure.sh" "${TAG}-${name}" "$duration" &
    MEASURE_PID=$!

    # Run the load; capture exit code but don't abort the whole run on load error
    bash "$script" || log "load script ${name} exited non-zero"

    wait "$MEASURE_PID" || true
    MEASURE_PID=""

    # Capture final container state (OOMKilled flag, exit code)
    STATE=$(docker inspect --format '{{json .State}}' nexus-memtest 2>/dev/null || echo '{}')
    echo "$STATE" > "${OUT_DIR}/${TAG}-${name}-state.json"
    OOM=$(echo "$STATE" | jq -r '.OOMKilled // false')
    log "scenario=${name} OOMKilled=${OOM}"

    if [ "$OOM" = "true" ]; then
        log "ABORTING: container OOMKilled during ${name}"
        return 1
    fi
}

log "building + starting container"
docker compose -f "$COMPOSE_FILE" up --build -d

log "waiting for health"
for i in $(seq 1 60); do
    if curl -sf http://localhost:15474/health >/dev/null 2>&1; then
        log "healthy after ${i}s"; break
    fi
    sleep 1
    [ "$i" -eq 60 ] && { log "server never became healthy"; exit 1; }
done

# Record RSS at rest (empty DB)
log "capturing boot baseline (30s idle)"
bash "${SCRIPT_DIR}/measure.sh" "${TAG}-boot" 30 &
MEASURE_PID=$!
wait "$MEASURE_PID"
MEASURE_PID=""

run_scenario "ingest" "${SCRIPT_DIR}/load-ingest.sh" 180 || true
run_scenario "knn"    "${SCRIPT_DIR}/load-knn.sh"    180 || true
run_scenario "gql"    "${SCRIPT_DIR}/load-graphql.sh" 120 || true

log "done — CSVs under ${OUT_DIR}/"
ls -lh "${OUT_DIR}/" | tail -10
