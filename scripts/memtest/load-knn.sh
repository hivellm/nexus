#!/usr/bin/env bash
# Load scenario: KNN.
# Populates N_VECTORS 128-dim vectors then issues N_QUERIES KNN traversals.
# Exercises HNSW memory growth.

set -euo pipefail

HOST="${HOST:-http://localhost:15474}"
N_VECTORS="${N_VECTORS:-10000}"
N_QUERIES="${N_QUERIES:-1000}"
DIM="${DIM:-128}"

echo "[knn] target: ${N_VECTORS} vectors × ${DIM}d + ${N_QUERIES} queries"

for i in $(seq 1 30); do
    if curl -sf "${HOST}/health" >/dev/null 2>&1; then break; fi
    sleep 1
done

# Deterministic vector generator (bash-only: sin-wave pseudo-random).
gen_vector() {
    local seed="$1"
    local dim="$2"
    local vec="["
    local first=1
    for k in $(seq 0 $((dim - 1))); do
        # float in [-1, 1] without invoking python/awk per element (faster)
        local raw=$(( (seed * 1103515245 + k * 12345 + 1013904223) % 2000 ))
        local f=$(awk -v r="$raw" 'BEGIN{printf "%.4f", (r-1000)/1000.0}')
        if [ $first -eq 1 ]; then vec="${vec}${f}"; first=0; else vec="${vec},${f}"; fi
    done
    echo "${vec}]"
}

echo "[knn] inserting vectors"
for i in $(seq 1 "$N_VECTORS"); do
    V=$(gen_vector "$i" "$DIM")
    Q="CREATE (n:Vec {id: ${i}, embedding: ${V}})"
    curl -sf -X POST "${HOST}/cypher" \
        -H "Content-Type: application/json" \
        -d "{\"query\": \"${Q}\"}" >/dev/null || true
    [ $((i % 1000)) -eq 0 ] && echo "[knn]   vectors=${i}"
done

echo "[knn] running ${N_QUERIES} KNN queries"
for i in $(seq 1 "$N_QUERIES"); do
    V=$(gen_vector "$((i * 7919))" "$DIM")
    curl -sf -X POST "${HOST}/knn_traverse" \
        -H "Content-Type: application/json" \
        -d "{\"label\": \"Vec\", \"vector\": ${V}, \"k\": 10}" >/dev/null || true
    [ $((i % 100)) -eq 0 ] && echo "[knn]   queries=${i}"
done

echo "[knn] done"
