#!/usr/bin/env bash
# Bound the size of Cargo's `target/` directory (GH #24).
#
# Cargo never garbage-collects `target/`: stale object files, incremental
# caches, and rlibs from old dependency versions accumulate indefinitely.
# This script wraps `cargo-sweep` to delete artifacts that have not been
# accessed in N days WITHOUT breaking incrementality — the hot set you're
# actively rebuilding keeps its mtime and survives the sweep.
#
# Usage:
#   scripts/sweep-target.sh                 # sweep artifacts older than 14 days
#   scripts/sweep-target.sh --time 30       # custom retention window (days)
#   scripts/sweep-target.sh --clean         # full `cargo clean` (reclaim everything)
#   scripts/sweep-target.sh --dry-run       # show what would be removed, delete nothing
#
# cargo-sweep is auto-installed (`cargo install cargo-sweep`) if missing.
#
# See docs/development/rust-target-hygiene.md for the full hygiene policy.
set -euo pipefail

cd "$(dirname "$0")/.."

DAYS=14
DRY_RUN=""
DO_CLEAN=0

while [[ $# -gt 0 ]]; do
  case "$1" in
    --time)
      DAYS="${2:?--time requires a number of days}"
      shift 2
      ;;
    --clean)
      DO_CLEAN=1
      shift
      ;;
    --dry-run)
      DRY_RUN="--dry-run"
      shift
      ;;
    -h|--help)
      sed -n '2,18p' "$0"
      exit 0
      ;;
    *)
      echo "error: unknown argument '$1' (try --help)" >&2
      exit 2
      ;;
  esac
done

if [[ "$DO_CLEAN" -eq 1 ]]; then
  echo "==> cargo clean (full reclaim)"
  cargo clean
  echo "==> done. target/ fully removed; next build is cold."
  exit 0
fi

if ! command -v cargo-sweep >/dev/null 2>&1; then
  echo "==> cargo-sweep not found; installing (one-time)..."
  cargo install cargo-sweep
fi

BEFORE=""
if command -v du >/dev/null 2>&1 && [[ -d target ]]; then
  BEFORE="$(du -sh target 2>/dev/null | cut -f1)"
fi

echo "==> cargo sweep --time ${DAYS} ${DRY_RUN}"
# shellcheck disable=SC2086
cargo sweep --time "$DAYS" $DRY_RUN

if [[ -z "$DRY_RUN" && -n "$BEFORE" ]]; then
  AFTER="$(du -sh target 2>/dev/null | cut -f1)"
  echo "==> target/ size: ${BEFORE} -> ${AFTER}"
fi
echo "==> done."
