#!/usr/bin/env bash
# scripts/ci/check_version_consistency.sh — phase7_reconcile-version-strings
#
# Enforces that the public-facing version surfaces all read the same
# X.Y.Z string. Drift is a hard failure. Mirrors the canonical policy
# in docs/development/RELEASE_PROCESS.md.
#
# Surfaces validated:
#   1. Workspace crate version: `Cargo.toml [workspace.package].version`
#   2. README badge: line 8, the `status-vX.Y.Z` shield
#   3. CHANGELOG top heading: `## [X.Y.Z] — YYYY-MM-DD`
#
# Run locally before committing a version bump:
#   bash scripts/ci/check_version_consistency.sh
#
# Wired into `.github/workflows/rust-lint.yml` so every push and PR
# fails fast on drift.

set -euo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
cd "$repo_root"

# 1) Workspace version (canonical).
workspace_version=$(awk '
  /^\[workspace\.package\]/ { in_block = 1; next }
  /^\[/ && in_block         { in_block = 0 }
  in_block && /^version[[:space:]]*=/ {
    gsub(/[^0-9.]/, "", $0); print; exit
  }
' Cargo.toml)

if [[ -z "$workspace_version" ]]; then
  echo "ERROR: failed to parse workspace version from Cargo.toml" >&2
  exit 1
fi

# 2) README badge: `status-vX.Y.Z`.
readme_version=$(grep -oE 'status-v[0-9]+\.[0-9]+\.[0-9]+' README.md | head -n1 | sed 's/^status-v//')
if [[ -z "$readme_version" ]]; then
  echo "ERROR: failed to parse README status badge version" >&2
  exit 1
fi

# 3) CHANGELOG top heading: `## [X.Y.Z]`.
changelog_version=$(grep -oE '^## \[[0-9]+\.[0-9]+\.[0-9]+\]' CHANGELOG.md | head -n1 | sed -E 's/^## \[([0-9]+\.[0-9]+\.[0-9]+)\]/\1/')
if [[ -z "$changelog_version" ]]; then
  echo "ERROR: failed to parse top CHANGELOG heading" >&2
  exit 1
fi

echo "Version consistency check"
echo "  Cargo.toml workspace : $workspace_version"
echo "  README status badge  : $readme_version"
echo "  CHANGELOG top heading: $changelog_version"

if [[ "$workspace_version" != "$readme_version" ]] \
   || [[ "$workspace_version" != "$changelog_version" ]]; then
  echo "" >&2
  echo "FAIL: version surfaces drift" >&2
  echo "  Cargo.toml workspace : $workspace_version" >&2
  echo "  README status badge  : $readme_version" >&2
  echo "  CHANGELOG top heading: $changelog_version" >&2
  echo "" >&2
  echo "Fix per docs/development/RELEASE_PROCESS.md §Bumping." >&2
  exit 1
fi

echo "OK: all surfaces report $workspace_version"
