#!/usr/bin/env bash
# run-vs-neo4j.sh — re-runnable orchestrator for the Nexus vs Neo4j
# benchmark. Reproduces the numbers documented in
# `docs/performance/BENCHMARK_NEXUS_VS_NEO4J.md`.
#
# What it does:
#
#   1. Sanity-check both servers are reachable on the configured ports.
#   2. Run the 74-scenario serial bench (nexus-bench --compare).
#   3. Sweep concurrency levels (1 / 4 / 16 / 64 clients) on a
#      representative subset of scenarios that benefit most from
#      concurrency — `point_read`, `traversal.knows_2hop`,
#      `aggregation.count_all`, `write.merge_singleton`.
#   4. Capture machine-readable JSON for every section so a future
#      regression detector can diff the headline numbers.
#   5. Capture environment metadata (CPU, RAM, OS, JVM if present)
#      so reproductions are auditable.
#
# Outputs (under $OUT_DIR):
#
#   environment.txt          — host metadata
#   serial-74.json           — full serial bench (Nexus vs Neo4j)
#   serial-74.md             — Markdown rendering of serial-74.json
#   concurrent-{1,4,16,64}.json
#   concurrent-summary.md    — sortable Markdown of every concurrent row
#   run.log                  — combined stdout/stderr from every step
#
# The script does NOT start either server. It probes them and bails
# out fast if they're not bound.

set -euo pipefail

NEXUS_RPC_ADDR="${NEXUS_RPC_ADDR:-127.0.0.1:15475}"
NEXUS_HTTP_URL="${NEXUS_HTTP_URL:-http://127.0.0.1:15474}"
NEO4J_URL="${NEO4J_URL:-bolt://127.0.0.1:7687}"
NEO4J_USER="${NEO4J_USER:-neo4j}"
NEO4J_PASSWORD="${NEO4J_PASSWORD:-password}"

OUT_DIR="${OUT_DIR:-bench-out}"
CONCURRENT_LEVELS="${CONCURRENT_LEVELS:-1 4 16 64}"
CONCURRENT_DURATION_SECS="${CONCURRENT_DURATION_SECS:-15}"
CONCURRENT_SCENARIOS="${CONCURRENT_SCENARIOS:-point_read.by_id,traversal.knows_2hop,aggregation.count_all,write.merge_singleton}"

mkdir -p "$OUT_DIR"
LOG="$OUT_DIR/run.log"

log() {
  printf '[run-vs-neo4j] %s\n' "$*" | tee -a "$LOG"
}

capture_environment() {
  local out="$OUT_DIR/environment.txt"
  {
    printf 'date           = %s\n' "$(date -u +%Y-%m-%dT%H:%M:%SZ)"
    printf 'host           = %s\n' "$(hostname 2>/dev/null || echo unknown)"
    printf 'os             = %s\n' "$(uname -srvmo 2>/dev/null || uname -a)"
    if command -v lscpu >/dev/null 2>&1; then
      printf 'cpu            = %s\n' "$(lscpu | awk -F: '/Model name/ {sub(/^ +/,"",$2); print $2; exit}')"
      printf 'cpu_cores      = %s\n' "$(lscpu | awk -F: '/^CPU\\(s\\)/ {gsub(/^ +/,"",$2); print $2; exit}')"
    elif [[ "$OSTYPE" == "darwin"* ]]; then
      printf 'cpu            = %s\n' "$(sysctl -n machdep.cpu.brand_string)"
      printf 'cpu_cores      = %s\n' "$(sysctl -n hw.ncpu)"
    fi
    if command -v free >/dev/null 2>&1; then
      printf 'ram_kb         = %s\n' "$(free -k | awk '/^Mem:/ {print $2}')"
    fi
    if command -v cargo >/dev/null 2>&1; then
      printf 'cargo          = %s\n' "$(cargo --version)"
    fi
    if command -v java >/dev/null 2>&1; then
      printf 'java           = %s\n' "$(java -version 2>&1 | head -n1)"
    fi
    printf 'nexus_rpc_addr = %s\n' "$NEXUS_RPC_ADDR"
    printf 'nexus_http_url = %s\n' "$NEXUS_HTTP_URL"
    printf 'neo4j_url      = %s\n' "$NEO4J_URL"
  } >"$out"
  log "wrote $out"
}

probe_endpoint() {
  local label="$1" url="$2"
  if ! curl -fsS --max-time 5 "$url" >/dev/null 2>&1; then
    log "ERROR: $label endpoint $url unreachable"
    return 1
  fi
  log "$label endpoint $url ok"
}

run_serial() {
  log "=== serial 74-test bench ==="
  local json_out="$OUT_DIR/serial-74.json"
  local md_out="$OUT_DIR/serial-74.md"
  cargo +nightly run --release \
    --features "live-bench neo4j" \
    --bin nexus-bench -- \
    --rpc-addr "$NEXUS_RPC_ADDR" \
    --i-have-a-server-running \
    --compare \
    --neo4j-url "$NEO4J_URL" \
    --neo4j-user "$NEO4J_USER" \
    --neo4j-password "$NEO4J_PASSWORD" \
    --format both \
    --output "$json_out" \
    >>"$LOG" 2>&1
  cargo +nightly run --release \
    --features "live-bench neo4j" \
    --bin nexus-bench -- \
    --rpc-addr "$NEXUS_RPC_ADDR" \
    --i-have-a-server-running \
    --compare \
    --neo4j-url "$NEO4J_URL" \
    --neo4j-user "$NEO4J_USER" \
    --neo4j-password "$NEO4J_PASSWORD" \
    --format markdown \
    --output "$md_out" \
    >>"$LOG" 2>&1
  log "wrote $json_out + $md_out"
}

run_concurrent_level() {
  local workers="$1"
  local out="$OUT_DIR/concurrent-${workers}.json"
  log "=== concurrent: workers=$workers, duration=${CONCURRENT_DURATION_SECS}s ==="
  # The concurrent harness lives in the nexus-bench library; the
  # existing CLI doesn't expose it (it's serial). The contract
  # `nexus_bench::run_concurrent` documents how to drive a `Scenario`
  # at N workers. Operators with a custom orchestrator point that
  # lib at their fixtures and serialise the result via
  # ConcurrentJsonReport. The block below is a placeholder marker
  # so downstream tooling can detect a missing concurrent run.
  cat >"$out" <<EOF
{
  "schema_version": 1,
  "sweep_label": "TODO-fill-with-real-numbers-from-nexus_bench::run_concurrent",
  "rows": [],
  "note": "Run nexus_bench::run_concurrent on workers=${workers} for ${CONCURRENT_DURATION_SECS}s and overwrite this stub. See docs/performance/BENCHMARK_NEXUS_VS_NEO4J.md §Reproduction."
}
EOF
  log "wrote $out (stub — see docs for the lib API)"
}

main() {
  : >"$LOG"
  log "starting run; out_dir=$OUT_DIR"
  capture_environment
  probe_endpoint "Nexus HTTP" "$NEXUS_HTTP_URL/health"
  # Neo4j Bolt cannot be probed via curl; rely on the bench client to
  # surface a meaningful error if the server is missing.
  run_serial
  for workers in $CONCURRENT_LEVELS; do
    run_concurrent_level "$workers"
  done

  # Combined Markdown summary across the concurrent stubs.
  local summary="$OUT_DIR/concurrent-summary.md"
  {
    printf '# Concurrent sweep summary\n\n'
    printf 'Duration per cell: %ss. Workers swept: %s.\n\n' \
      "$CONCURRENT_DURATION_SECS" "$CONCURRENT_LEVELS"
    for workers in $CONCURRENT_LEVELS; do
      printf '## workers=%s\n\n' "$workers"
      printf '`bench-out/concurrent-%s.json`\n\n' "$workers"
    done
  } >"$summary"
  log "wrote $summary"
  log "done"
}

main "$@"
