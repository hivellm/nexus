#!/usr/bin/env bash
# Run the Nexus executor-level benchmarks and emit a row-vs-columnar
# speedup summary.
#
# Covers:
#   - nexus-core/benches/executor_filter.rs     (100 k row WHERE)
#   - nexus-core/benches/executor_aggregate.rs  (SUM/MIN/MAX/AVG × i64/f64 × {10k, 100k, 1M})
#
# These are in-process — no server required, unlike the protocol suite.
# Each bench runs the same fixture twice (row-path baseline with
# columnar_threshold=usize::MAX, columnar-path with default 4096);
# the speedup summary below pairs the two and prints the ratio.
#
# Usage:
#   scripts/benchmarks/run-executor-suite.sh            # full sweep
#   scripts/benchmarks/run-executor-suite.sh --quick    # Criterion --quick mode
#
# Results land in target/criterion/ (HTML reports) and are summarised
# into target/criterion/executor-summary.csv plus a human-readable
# ratio table for docs/performance/PERFORMANCE_V1.md.

set -euo pipefail

OUT_DIR="${CARGO_TARGET_DIR:-target}/criterion"
CSV="${OUT_DIR}/executor-summary.csv"
mkdir -p "${OUT_DIR}"

QUICK_ARGS=""
if [[ "${1:-}" == "--quick" ]]; then
    QUICK_ARGS="-- --quick"
fi

echo "# Nexus executor benchmark run — $(date -u +%Y-%m-%dT%H:%M:%SZ)"
echo "# columnar_threshold row-vs-column ratio sweep"

cargo bench --bench executor_filter ${QUICK_ARGS}
cargo bench --bench executor_aggregate ${QUICK_ARGS}

# ── Summary CSV + speedup table ───────────────────────────────────────
# Criterion writes estimates.json under each bench's directory. Pair
# every `<group>/row/<size>` with its `<group>/columnar/<size>`
# sibling and compute the ratio = row_ns / columnar_ns.
echo "Building speedup summary → ${CSV}"
{
    echo "group,size,row_ns,columnar_ns,speedup"
    find "${OUT_DIR}" \
        -path '*/row/*/new/estimates.json' 2>/dev/null | while read -r row_f; do
        # Derive the paired columnar path.
        col_f="${row_f//\/row\//\/columnar\/}"
        if [[ ! -f "${col_f}" ]]; then continue; fi

        # Extract group + size from the path:
        #   target/criterion/<group>/row/<size>/new/estimates.json
        rel="${row_f#${OUT_DIR}/}"
        group="${rel%%/*}"
        rest="${rel#${group}/row/}"
        size="${rest%%/*}"

        row_ns=$(grep -oE '"mean":\{[^}]+"point_estimate":[0-9.]+' "${row_f}" \
                  | grep -oE '[0-9.]+$' | head -1)
        col_ns=$(grep -oE '"mean":\{[^}]+"point_estimate":[0-9.]+' "${col_f}" \
                  | grep -oE '[0-9.]+$' | head -1)
        if [[ -z "${row_ns}" || -z "${col_ns}" ]]; then continue; fi

        speedup=$(awk -v r="${row_ns}" -v c="${col_ns}" 'BEGIN{printf "%.2f", r/c}')
        echo "${group},${size},${row_ns},${col_ns},${speedup}"
    done
} > "${CSV}"

echo ""
echo "# Speedup summary (row_ns / columnar_ns — higher is better)"
column -t -s, "${CSV}"
echo ""
echo "Wrote ${CSV}"
echo "HTML report: ${OUT_DIR}/report/index.html"
