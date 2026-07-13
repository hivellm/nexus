#!/usr/bin/env bash
# create-rel-verdict.sh -- dedicated repeated-run measurement for the
# CREATE-relationship contradiction (phase7_benchmark-rebaseline item 1.2).
#
# Prior reports disagree wildly on Nexus CREATE-relationship performance
# vs Neo4j:
#   - BENCHMARK_RESULTS_PHASE9.md (2025-11-20): Nexus 25.38ms, Neo4j
#     3.14ms per op -- Nexus 87.6% SLOWER.
#   - BENCHMARK_NEXUS_VS_NEO4J.md (2025-12-01, v0.12.0, self-flagged
#     stale): Neo4j 121.93ms, Nexus 2.85ms -- Nexus 42.7x FASTER.
# Both numbers came from the nexus-bench harness (native RPC transport)
# on different code + dataset generations that no longer exist in this
# form (the current seed catalogue under crates/nexus-bench/src/scenarios
# has no CREATE-relationship scenario at all -- write.rs only ships
# idempotent write scenarios). This script settles the question with a
# fresh, engine-symmetric, repeated measurement over the REST/HTTP
# transport both engines expose, so the comparison is apples-to-apples
# regardless of which native driver either side ships.
#
# Usage:
#   NEXUS_HTTP_URL=http://127.0.0.1:15474 \
#   NEO4J_HTTP_URL=http://127.0.0.1:7474 \
#   NEO4J_PASSWORD=password \
#   REPS=5 OPS_PER_REP=100 \
#   bash scripts/benchmarks/create-rel-verdict.sh

set -euo pipefail

NEXUS_HTTP_URL="${NEXUS_HTTP_URL:-http://127.0.0.1:15474}"
NEO4J_HTTP_URL="${NEO4J_HTTP_URL:-http://127.0.0.1:7474}"
NEO4J_USER="${NEO4J_USER:-neo4j}"
NEO4J_PASSWORD="${NEO4J_PASSWORD:-password}"
REPS="${REPS:-5}"
OPS_PER_REP="${OPS_PER_REP:-100}"
OUT_DIR="${OUT_DIR:-bench-out}"
OUT_CSV="$OUT_DIR/create-rel-verdict.csv"
OUT_JSON="$OUT_DIR/create-rel-verdict.json"

mkdir -p "$OUT_DIR"

nexus_query='{"query":"CREATE (a:CreateRelBench)-[:REL]->(b:CreateRelBench) RETURN 1 AS ok"}'
neo4j_query='{"statements":[{"statement":"CREATE (a:CreateRelBench)-[:REL]->(b:CreateRelBench) RETURN 1 AS ok"}]}'
neo4j_auth_b64="$(printf '%s:%s' "$NEO4J_USER" "$NEO4J_PASSWORD" | base64 | tr -d '\n')"

time_one_nexus() {
  curl -s -o /dev/null -w '%{time_total}\n' -X POST "$NEXUS_HTTP_URL/cypher" \
    -H 'Content-Type: application/json' -d "$nexus_query"
}

time_one_neo4j() {
  curl -s -o /dev/null -w '%{time_total}\n' -X POST "$NEO4J_HTTP_URL/db/neo4j/tx/commit" \
    -H 'Content-Type: application/json' -H "Authorization: Basic $neo4j_auth_b64" -d "$neo4j_query"
}

median_of() {
  # stdin: one number per line. Prints the median.
  sort -n | awk '{a[NR]=$1} END {if (NR%2==1) print a[(NR+1)/2]; else print (a[NR/2]+a[NR/2+1])/2}'
}

echo "engine,rep,median_ms,min_ms,max_ms,mean_ms" > "$OUT_CSV"

run_engine() {
  local engine="$1" fn="$2"
  local rep_medians=()
  for rep in $(seq 1 "$REPS"); do
    local samples_file
    samples_file="$(mktemp)"
    for _ in $(seq 1 "$OPS_PER_REP"); do
      "$fn" >> "$samples_file"
    done
    # curl %{time_total} is seconds with 6 decimals; convert to ms.
    local median min max mean
    median=$(awk '{print $1*1000}' "$samples_file" | median_of)
    min=$(awk '{print $1*1000}' "$samples_file" | sort -n | head -1)
    max=$(awk '{print $1*1000}' "$samples_file" | sort -n | tail -1)
    mean=$(awk '{sum+=$1*1000; n++} END {print sum/n}' "$samples_file")
    echo "$engine,$rep,$median,$min,$max,$mean" >> "$OUT_CSV"
    printf '[%s] rep %s/%s: median=%sms min=%sms max=%sms mean=%sms\n' \
      "$engine" "$rep" "$REPS" "$median" "$min" "$max" "$mean" >&2
    rep_medians+=("$median")
    rm -f "$samples_file"
  done
  printf '%s\n' "${rep_medians[@]}"
}

echo "=== Nexus: $REPS reps x $OPS_PER_REP CREATE (a)-[:REL]->(b) each ==="
mapfile -t nexus_medians < <(run_engine nexus time_one_nexus)

echo "=== Neo4j: $REPS reps x $OPS_PER_REP CREATE (a)-[:REL]->(b) each ==="
mapfile -t neo4j_medians < <(run_engine neo4j time_one_neo4j)

nexus_grand_median=$(printf '%s\n' "${nexus_medians[@]}" | median_of)
neo4j_grand_median=$(printf '%s\n' "${neo4j_medians[@]}" | median_of)
nexus_spread=$(printf '%s\n' "${nexus_medians[@]}" | sort -n | awk 'NR==1{min=$1} {max=$1} END{print max-min}')
neo4j_spread=$(printf '%s\n' "${neo4j_medians[@]}" | sort -n | awk 'NR==1{min=$1} {max=$1} END{print max-min}')

ratio=$(awk -v n="$nexus_grand_median" -v j="$neo4j_grand_median" 'BEGIN{print j/n}')

python3 - "$OUT_JSON" "$nexus_grand_median" "$neo4j_grand_median" "$nexus_spread" "$neo4j_spread" "$ratio" "$REPS" "$OPS_PER_REP" <<'PYEOF' 2>/dev/null || true
import json, sys
out, nm, jm, ns, js, ratio, reps, ops = sys.argv[1:9]
data = {
    "nexus_median_ms": float(nm),
    "neo4j_median_ms": float(jm),
    "nexus_spread_ms": float(ns),
    "neo4j_spread_ms": float(js),
    "neo4j_over_nexus_ratio": float(ratio),
    "reps": int(reps),
    "ops_per_rep": int(ops),
}
with open(out, "w") as f:
    json.dump(data, f, indent=2)
PYEOF

echo ""
echo "=== VERDICT ==="
printf 'Nexus  grand median: %sms (spread across %s rep-medians: %sms)\n' "$nexus_grand_median" "$REPS" "$nexus_spread"
printf 'Neo4j  grand median: %sms (spread across %s rep-medians: %sms)\n' "$neo4j_grand_median" "$REPS" "$neo4j_spread"
printf 'Neo4j/Nexus ratio: %s (>1 means Nexus is faster)\n' "$ratio"
echo "wrote $OUT_CSV"
[ -f "$OUT_JSON" ] && echo "wrote $OUT_JSON"
exit 0
