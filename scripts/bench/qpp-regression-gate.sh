#!/usr/bin/env bash
# QPP regression gate — slice-3b §9.4 of
# `phase6_opencypher-quantified-path-patterns`.
#
# Runs the `qpp_benchmark` Criterion suite, parses the JSON output,
# and asserts that the slice-3a `QuantifiedExpand` operator
# (named-body shape) stays within 1.3× the legacy `*m..n`
# operator on the same fixture. Exits non-zero on regression so
# CI can gate releases on it.
#
# Usage:
#
#   bash scripts/bench/qpp-regression-gate.sh
#
# The 1.3× budget mirrors the design-doc table in
# `.rulebook/tasks/phase6_opencypher-quantified-path-patterns/design.md`
# (§9.4). Picking 1.3× rather than 1.0× leaves headroom for the
# extra per-frame bookkeeping the operator does (per-position node
# lists, per-hop relationship lists) — bookkeeping that the legacy
# operator does not have.

set -euo pipefail

CRATE_DIR="${CRATE_DIR:-crates/nexus-core}"
BENCH_NAME="${BENCH_NAME:-qpp_benchmark}"
BUDGET_RATIO="${BUDGET_RATIO:-1.3}"

echo "▶ Running Criterion suite: $BENCH_NAME"
cargo +nightly bench -p nexus-core --bench "$BENCH_NAME" -- --quiet

# Criterion drops `target/criterion/<group>/<bench>/new/estimates.json`
# for every bench. We pull the median ns from each.
read_median_ns() {
  local path="$1"
  if [ ! -f "$path" ]; then
    echo "::error::missing Criterion estimate at $path"
    exit 2
  fi
  python3 -c "import json,sys; d=json.load(open(sys.argv[1])); print(d['median']['point_estimate'])" "$path"
}

LEGACY_PATH="target/criterion/qpp_legacy_var_length/knows_*1..5/new/estimates.json"
NAMED_PATH="target/criterion/qpp_named_body/knows_{1,5}_named_inner/new/estimates.json"

# Bench group names contain `/` and `*` etc.; let bash glob-expand
# those literally via `compgen` so we fail loud when the directory
# layout changes upstream.
shopt -s globstar nullglob
LEGACY_FILES=( $LEGACY_PATH )
NAMED_FILES=( $NAMED_PATH )

if [ ${#LEGACY_FILES[@]} -eq 0 ] || [ ${#NAMED_FILES[@]} -eq 0 ]; then
  echo "::error::Criterion did not emit the expected estimate files."
  echo "Looked for:"
  echo "  $LEGACY_PATH"
  echo "  $NAMED_PATH"
  exit 2
fi

LEGACY_NS=$(read_median_ns "${LEGACY_FILES[0]}")
NAMED_NS=$(read_median_ns "${NAMED_FILES[0]}")

echo "  legacy  *1..5         median = ${LEGACY_NS} ns"
echo "  named   {1,5} body    median = ${NAMED_NS} ns"

RATIO=$(python3 -c "print(${NAMED_NS} / ${LEGACY_NS})")
echo "  ratio (named / legacy) = ${RATIO}"
echo "  budget                 = ${BUDGET_RATIO}"

OVER=$(python3 -c "print(1 if ${RATIO} > ${BUDGET_RATIO} else 0)")
if [ "$OVER" -eq 1 ]; then
  echo "::error::QPP named-body operator is ${RATIO}× legacy *m..n — exceeds the ${BUDGET_RATIO}× budget."
  echo "::error::Either narrow the named-body filter cost or raise BUDGET_RATIO with a justification in the task."
  exit 1
fi

echo "✓ QPP named-body operator within ${BUDGET_RATIO}× legacy budget."
