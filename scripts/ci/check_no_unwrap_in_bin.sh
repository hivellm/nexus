#!/usr/bin/env bash
# CI guard for phase4_binary-boundary-unwrap-cleanup.
#
# Fails if a new `.unwrap()` appears in a binary-boundary source file —
# `main.rs` or any file under `src/commands/` — without either a
# compile-time-invariant rationale on the surrounding lines or an
# explicit `.expect("...")` replacement.
#
# The rule: CLI / server binary entry points should never panic on
# user-facing error conditions. Library code (`src/lib.rs` and
# descendants that are not `commands/`) is exempt; the Rust rule
# `.claude/rules/rust.md` covers the general policy.
#
# Usage:
#   bash scripts/ci/check_no_unwrap_in_bin.sh            # fail on hit
#   bash scripts/ci/check_no_unwrap_in_bin.sh --list     # list hits
#
# The check is intentionally conservative: every `.unwrap()` in scope
# is reported. If an occurrence is genuinely safe, replace it with
# `.expect("...rationale...")` — `.expect(` is allowed and does not
# trip this guard.

set -euo pipefail

ROOT="${GITHUB_WORKSPACE:-$(git rev-parse --show-toplevel 2>/dev/null || pwd)}"
# Safety check: `git rev-parse --show-toplevel` happily walks up past
# the intended repo root if the caller cd's into an unrelated
# sub-directory. Verify the computed ROOT actually contains the
# directories we're about to check; fall back to the caller's CWD
# otherwise so the script works inside stand-alone test fixtures
# and nested worktrees.
if [[ ! -d "$ROOT/nexus-cli" && ! -d "$ROOT/nexus-server" && ! -d "$ROOT/nexus-core" ]]; then
    ROOT="$(pwd)"
fi
cd "$ROOT"

SCOPES=(
    "nexus-cli/src/main.rs"
    "nexus-cli/src/commands"
    "nexus-server/src/main.rs"
)

mode="enforce"
if [[ "${1:-}" == "--list" ]]; then
    mode="list"
fi

offenders=()

# Strip everything from the first `#[cfg(test)]` or bare `mod tests`
# line onward — every binary-boundary file we check keeps its tests in
# a trailing `#[cfg(test)] mod tests { ... }` module, so this cheap
# truncation drops the test code without needing a real Rust parser.
scope_prod() {
    awk '
        /^\s*(#\[cfg\(test\)\]|mod tests\s*\{)/ { exit }
        { print NR ":" $0 }
    ' "$1"
}

for scope in "${SCOPES[@]}"; do
    if [[ ! -e "$scope" ]]; then
        continue
    fi
    if [[ -d "$scope" ]]; then
        files=$(find "$scope" -type f -name '*.rs')
    else
        files="$scope"
    fi
    for file in $files; do
        hits=$(scope_prod "$file" | grep -E '\.unwrap\(\)' || true)
        if [[ -n "$hits" ]]; then
            while IFS= read -r hit; do
                offenders+=("$file:$hit")
            done <<< "$hits"
        fi
    done
done

if [[ "${#offenders[@]}" -eq 0 ]]; then
    if [[ "$mode" == "list" ]]; then
        echo "no .unwrap() hits in binary-boundary scopes (good)"
    fi
    exit 0
fi

echo "error: .unwrap() found in binary-boundary source:"
for line in "${offenders[@]}"; do
    echo "  $line"
done
echo
echo "Replace with .expect(\"...rationale...\") or propagate via ?"
echo "See .rulebook/tasks/phase4_binary-boundary-unwrap-cleanup for context."

if [[ "$mode" == "list" ]]; then
    exit 0
fi
exit 1
