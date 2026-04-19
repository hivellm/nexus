#!/usr/bin/env bash
# Run the Nexus protocol-level benchmarks and emit a CSV summary.
#
# Covers:
#   - Wire-codec throughput (nexus-core/benches/protocol_point_read.rs)
#     — always runnable, no server required.
#   - End-to-end point read / pattern / KNN / bulk ingest — run when
#     NEXUS_BENCH_URL is set to a live nexus-server.
#
# Usage:
#   scripts/benchmarks/run-protocol-suite.sh                    # codec only
#   NEXUS_BENCH_URL=nexus://127.0.0.1:15475 \
#     scripts/benchmarks/run-protocol-suite.sh                  # full matrix
#   NEXUS_BENCH_URL=http://127.0.0.1:15474 \
#     scripts/benchmarks/run-protocol-suite.sh                  # HTTP parity run
#
# Results land in target/criterion/ (HTML reports) and are summarised
# into target/criterion/protocol-summary.csv for docs/PERFORMANCE.md.

set -euo pipefail

OUT_DIR="${CARGO_TARGET_DIR:-target}/criterion"
CSV="${OUT_DIR}/protocol-summary.csv"
mkdir -p "${OUT_DIR}"

echo "# Nexus protocol benchmark run — $(date -u +%Y-%m-%dT%H:%M:%SZ)"
echo "# NEXUS_BENCH_URL=${NEXUS_BENCH_URL:-<codec-only>}"

# ── Always-runnable codec benches ─────────────────────────────────────
cargo bench --bench protocol_point_read -- --save-baseline protocol-run

# ── End-to-end benches (gated on live server) ─────────────────────────
if [[ -n "${NEXUS_BENCH_URL:-}" ]]; then
    echo "Running end-to-end matrix against ${NEXUS_BENCH_URL}"
    # Future benches (rpc_cypher_pattern, rpc_knn_search, rpc_ingest_bulk,
    # http_parity, resp3_parity, pipelining) wire in here as they land
    # via phase3_rpc-protocol-docs-benchmarks §4.3–§4.9. Each follows
    # the same `cargo bench --bench <name>` form and writes into
    # target/criterion/<name>/report/.
    echo "# Pending benches: rpc_cypher_pattern rpc_knn_search rpc_ingest_bulk http_parity resp3_parity pipelining"
else
    echo "# NEXUS_BENCH_URL not set — skipping end-to-end matrix."
fi

# ── Summary CSV ───────────────────────────────────────────────────────
# Criterion writes estimates.json under each bench's directory; extract
# the median / mean into a flat CSV a reviewer can paste into
# docs/PERFORMANCE.md.
{
    echo "bench,metric,point_estimate_ns"
    find "${OUT_DIR}" -name estimates.json -path '*new*' | while read -r f; do
        bench=$(echo "${f}" | sed -E "s|.*criterion/([^/]+)/.*|\\1|")
        median=$(grep -oE '"median":\{[^}]+"point_estimate":[0-9.]+' "${f}" \
                  | grep -oE '[0-9.]+$' | head -1 || echo "")
        mean=$(grep -oE '"mean":\{[^}]+"point_estimate":[0-9.]+' "${f}" \
                  | grep -oE '[0-9.]+$' | head -1 || echo "")
        if [[ -n "${median}" ]]; then
            echo "${bench},median_ns,${median}"
        fi
        if [[ -n "${mean}" ]]; then
            echo "${bench},mean_ns,${mean}"
        fi
    done
} > "${CSV}"

echo "Wrote ${CSV}"
echo "HTML report: ${OUT_DIR}/report/index.html"
