#!/usr/bin/env bash
#
# Apply the LDBC SNB index DDL to a Nexus database, then prove the indexes
# actually registered.
#
# Run against a FRESH database BEFORE loading the CSVs — creating the indexes
# up front lets the loader populate them incrementally instead of paying for a
# full rebuild afterwards.
#
#   ./create-schema.sh                        # localhost:15474
#   ./create-schema.sh --url http://host:15474
#   ./create-schema.sh --no-verify            # skip the coverage proof
#
# Every statement is `IF NOT EXISTS`, so re-running is a no-op.
#
# THERE IS DELIBERATELY NO --database FLAG. Nexus currently ignores the
# `database` field on POST /cypher and never switches the session database, so
# every query lands in the same store no matter what is requested (filed as
# phase0_fix-cypher-database-routing). A flag that silently does nothing is
# worse than no flag, so this harness assumes ONE DATABASE PER SERVER PROCESS:
# point --url at a server started on a dedicated --data-dir.
#
# Statement splitting is deliberately simple: `//` comments are stripped and
# the remainder is split on `;`. indexes.cypher contains no string literals, so
# there is nothing for that to get wrong. Keep it that way.

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
DDL_FILE="$SCRIPT_DIR/indexes.cypher"

URL="http://localhost:15474"
VERIFY=1

die() {
    echo "error: $*" >&2
    exit 1
}

need_value() {
    [ -n "${2:-}" ] || die "$1 requires a value"
}

while [ $# -gt 0 ]; do
    case "$1" in
        --url)       need_value "$1" "${2:-}"; URL="$2"; shift 2 ;;
        --file)      need_value "$1" "${2:-}"; DDL_FILE="$2"; shift 2 ;;
        --no-verify) VERIFY=0; shift ;;
        -h|--help)   sed -n '3,25p' "${BASH_SOURCE[0]}" | sed 's/^# \{0,1\}//'; exit 0 ;;
        *)           die "unknown argument: $1" ;;
    esac
done

[ -f "$DDL_FILE" ] || die "DDL file not found: $DDL_FILE"
command -v curl >/dev/null 2>&1 || die "curl is required but not on PATH"

# Minimal JSON string escaper. Sufficient because the payloads here are Cypher
# statements from a file we control: backslash and double quote are the only
# characters that can appear and need escaping, and newlines are collapsed to
# spaces before the call.
json_escape() {
    printf '%s' "$1" | sed -e 's/\\/\\\\/g' -e 's/"/\\"/g'
}

cypher() {
    curl -s --max-time 120 -X POST "$URL/cypher" \
        -H 'Content-Type: application/json' \
        -d "{\"query\":\"$(json_escape "$1")\"}"
}

# The server reports failures as an `error` field inside a 200 body, so the
# HTTP status alone is not a sufficient check.
cypher_checked() {
    resp="$(cypher "$1")"
    if printf '%s' "$resp" | grep -q '"error"'; then
        printf '%s' "$resp" | sed 's/.*"error":"\([^"]*\)".*/\1/' | head -c 300 >&2
        echo >&2
        return 1
    fi
    printf '%s' "$resp"
}

curl -sf --max-time 10 "$URL/health" >/dev/null 2>&1 ||
    die "no healthy Nexus server at $URL (start one, or pass --url)"

echo "applying $(basename "$DDL_FILE") to $URL"
echo

applied=0
failed=0
# Parallel arrays of the (label, property) pairs the DDL declares, harvested
# from the statements themselves so the probe set can never drift from the DDL.
probe_labels=""
probe_props=""

while IFS= read -r stmt; do
    [ -z "$stmt" ] && continue

    name="$(printf '%s' "$stmt" | sed -n 's/^CREATE INDEX \([A-Za-z0-9_]*\).*/\1/p')"
    [ -n "$name" ] || name="$(printf '%.60s' "$stmt")"

    label="$(printf '%s' "$stmt" | sed -n 's/.*FOR (\([A-Za-z0-9_]*\):\([A-Za-z0-9_]*\)).*/\2/p')"
    prop="$(printf '%s' "$stmt" | sed -n 's/.*ON (\([A-Za-z0-9_]*\)\.\([A-Za-z0-9_]*\)).*/\2/p')"
    if [ -n "$label" ] && [ -n "$prop" ]; then
        probe_labels="$probe_labels $label"
        probe_props="$probe_props $prop"
    fi

    if resp="$(cypher_checked "$stmt" 2>/dev/null)"; then
        echo "ok   $name"
        applied=$((applied + 1))
    else
        echo "FAIL $name"
        cypher "$stmt" | sed 's/.*"error":"\([^"]*\)".*/     \1/' | head -c 300
        echo
        failed=$((failed + 1))
    fi
done < <(sed 's&//.*&&' "$DDL_FILE" | tr '\n' ' ' | tr ';' '\n' | sed 's/^[[:space:]]*//;s/[[:space:]]*$//')

[ "$failed" -eq 0 ] || { echo; die "$failed statement(s) failed"; }

if [ "$VERIFY" -eq 0 ]; then
    echo
    echo "done — $applied statement(s) applied (verification skipped)"
    exit 0
fi

# --- Coverage proof ------------------------------------------------------
#
# Nexus has no SHOW INDEXES (filed as phase7_opencypher-gap-closure item 4.6),
# so index registration cannot be read back directly. The available signal is
# the planner's `Nexus.Performance.UnindexedPropertyAccess` notification: it is
# attached when a label+property predicate has to fall back to a label scan,
# and its ABSENCE means the index is registered and being used.
#
# That signal only fires when the label has rows to scan — on an empty database
# the planner short-circuits and emits nothing, which would make this check
# pass vacuously (verified: probing a database with NO indexes at all reported
# every index present). So each label gets one throwaway node carrying every
# probed property, and the nodes are deleted and their removal asserted before
# the script exits.
#
# The sentinel is a large positive integer: negative literals are rejected in
# CREATE property maps today (phase7_opencypher-gap-closure item 4.7).
SENTINEL=999999999999

echo
echo "verifying index coverage (planner notifications, with probe rows)"

labels="$(printf '%s\n' $probe_labels | sort -u)"

# One probe node per label, carrying every property indexed on that label so
# the catalog has each key registered.
for label in $labels; do
    props=""
    set -- $probe_labels
    idx=1
    for l in $probe_labels; do
        p="$(printf '%s\n' $probe_props | sed -n "${idx}p")"
        [ "$l" = "$label" ] && props="$props $p"
        idx=$((idx + 1))
    done
    assignments=""
    for p in $props; do
        [ -n "$assignments" ] && assignments="$assignments, "
        assignments="$assignments$p: $SENTINEL"
    done
    cypher_checked "CREATE (:$label {$assignments, __schema_probe: true})" >/dev/null ||
        die "failed to create probe node for :$label"
done

missing=0
idx=1
for label in $probe_labels; do
    prop="$(printf '%s\n' $probe_props | sed -n "${idx}p")"
    idx=$((idx + 1))
    resp="$(cypher "MATCH (n:$label {$prop: $SENTINEL}) RETURN n LIMIT 1")"
    if printf '%s' "$resp" | grep -q 'UnindexedPropertyAccess'; then
        echo "MISS $label($prop) — planner reports no index"
        missing=$((missing + 1))
    else
        echo "ok   $label($prop)"
    fi
done

# Remove the probe rows and assert they are gone — a leaked probe node would
# corrupt the loader's post-load cardinality verification.
leaked=0
for label in $labels; do
    cypher_checked "MATCH (n:$label {__schema_probe: true}) DELETE n" >/dev/null ||
        die "failed to delete probe node for :$label"
    remaining="$(cypher "MATCH (n:$label {__schema_probe: true}) RETURN count(n)")"
    printf '%s' "$remaining" | grep -q '\[\[0\]\]' || {
        echo "LEAK $label — probe node not removed: $remaining"
        leaked=$((leaked + 1))
    }
done

[ "$leaked" -eq 0 ] || die "$leaked probe node(s) left behind; database is dirty"
[ "$missing" -eq 0 ] || die "$missing index(es) not registered"

echo
echo "done — $applied statement(s) applied, $(printf '%s\n' $probe_labels | wc -l | tr -d ' ') index(es) verified, probe rows removed"
