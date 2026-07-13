#!/usr/bin/env bash
# ============================================================================
# Per-transport Cypher write-path parity harness — HTTP vs RPC vs RESP3.
# ============================================================================
#
# WHY THIS EXISTS
#
# The 300-test Neo4j diff suite (test-neo4j-nexus-compatibility-200.*) runs
# every query over ONE transport (HTTP). Nexus exposes the SAME Cypher
# engine over three TCP front-ends — HTTP/JSON (15474), native binary RPC
# (15475), and RESP3 (15476) — and the 2.4.0-era write path forked per
# transport, producing silent data-loss bugs the single-path suite could
# never see (docs/nexus/02-bug-inventory.md, bugs B1/B2/B3/B6). This script
# is the permanent regression net: it runs the SAME battery of write-path
# queries over all three transports against one running server and fails
# loudly the moment any transport's result diverges from the others.
#
# PREREQUISITES (this script never manages server lifecycle — start/stop it
# yourself, e.g. `./target/release/nexus-server`):
#
#   - A Nexus server ALREADY running with:
#       HTTP   listener on NEXUS_HTTP_ADDR  (default 127.0.0.1:15474)
#       RPC    listener on NEXUS_RPC_ADDR   (default 127.0.0.1:15475)
#       RESP3  listener on NEXUS_RESP3_HOST:NEXUS_RESP3_PORT
#              (default host.docker.internal:15476 — reachable from inside
#              a Docker container; override NEXUS_RESP3_HOST to 127.0.0.1
#              if you run this script itself inside the same container/net
#              namespace as the server, e.g. in CI with --network host)
#   - `curl`   — health-check probe before running anything.
#   - `docker` — used for two disposable, cached images so the host needs
#                no native install:
#       * `redis:7-alpine`       speaks RESP3 to the CYPHER/CYPHER.WITH
#                                 commands via `redis-cli --json` (no local
#                                 redis-cli required).
#       * `ghcr.io/jqlang/jq`    normalises each transport's JSON envelope
#                                 down to `{columns, rows}` (drops
#                                 transport-specific `stats`/
#                                 `execution_time_ms` fields) for a fair
#                                 diff (no local jq required).
#   - The `nexus` CLI binary already built at
#       target/release/nexus(.exe)   (cargo build --release --package nexus-cli)
#     — speaks HTTP and native RPC with the SAME --json output shape, so no
#     hand-rolled HTTP/RPC client code lives in this script. Override the
#     path via NEXUS_CLI if you built it elsewhere.
#
# USAGE
#
#   ./scripts/compatibility/test-transport-parity.sh
#
# Exit code: 0 when every case agrees across all three transports, 1 when
# any case diverges (or a prerequisite is missing).
#
# BATTERY SCOPE
#
# A representative subset targeting the confirmed write-path bugs, not the
# full 300-query suite (see "Deliberate exclusions" below):
#
#   TC1  parameterized CREATE round-trip     — regression net for B6 ($param
#                                               dropped on RPC/RESP3 writes)
#   TC2  MERGE-rel with inline props         — regression net for B1 (MERGE
#                                               created 0 relationships over
#                                               HTTP)
#   TC3  CREATE ... RETURN r.prop            — regression net for B3 (rel
#       (same statement)                       property projected null in the
#                                               creating statement)
#   TC4  SET on a relationship variable      — regression net for B2 (SET
#                                               r.k silently dropped over HTTP)
#   TC5  aggregation, non-alphabetical       — validates column order +
#       RETURN column order                    aggregate values agree
#                                               identically across transports
#   TC6  UNWIND + MERGE batch write          — validates list-parameter
#                                               threading through a batched
#                                               MERGE across transports
#   TC7  DELETE + RETURN count(n)            — validates delete semantics
#                                               agree across transports
#
# Each case creates its own isolated node(s) — tagged with a per-transport
# label suffix (`Http` / `Rpc` / `Resp3`) baked directly into the Cypher text
# — so the three transports never see each other's writes mid-case. All
# probe/setup nodes carry the shared `:TPParity` label; the script wipes
# every `:TPParity` node before and after the run, so re-running it is
# idempotent and it leaves no residue in whatever database the server
# happens to be serving (safe to run against a server that is also serving
# other traffic, e.g. a concurrent KNN benchmark on unrelated labels).
#
# Cases that need a REAL Cypher parameter (not just per-transport label
# isolation) — TC1's $v and TC6's $items — are the ones deliberately
# exercising $param threading; every other case uses a literal, per-transport
# label suffix instead of a $param filter. This is not just style: this
# script's own validation run against a live 2.5.0-branch build surfaced a
# genuine, transport-agnostic bug where `MATCH (n {prop: $x}) ... DELETE n`
# silently fails to bind `$x` (HTTP no-ops with an empty 200, RPC raises
# ERR_MISSING_PARAMETER) — see the write-up in the task report / knowledge
# base. That bug is orthogonal to transport parity (it reproduces the same
# way on every transport) and is out of scope for this harness to fix, so
# TC7 deliberately avoids the pattern to keep this battery's baseline
# meaningful as a transport-divergence signal rather than permanently red on
# an unrelated, already-reproduced core bug.
#
# DELIBERATE EXCLUSIONS (documented, not silently dropped)
#
#   - Explicit multi-statement transactions (BEGIN/COMMIT/ROLLBACK) and
#     `CALL { ... } IN TRANSACTIONS ... REPORT STATUS` — HTTP-only surface
#     today; REPORT STATUS also returns multiple rows per call, which
#     complicates the simple whole-response diff this script does. Left for
#     a dedicated harness once the transaction surface is transport-uniform.
#   - GraphQL mutations — a different wire protocol entirely (not a
#     Cypher-over-TCP transport), and already tracked by its own bug (B5)
#     and task (`phase3_graphql-and-streaming-write-unification`). Wiring a
#     4th transport into this script is a natural follow-up once that task
#     lands, not a fit for "reuse the existing battery" today.
#   - Admin / database-management commands (CREATE USER, CREATE DATABASE,
#     SHOW ...) — these legitimately branch per transport onto
#     server-level services (DatabaseManager, RBAC) per the documented
#     anti-pattern ("per-transport reimplementation of query execution");
#     they are not instances of the write-path-fork bug class this harness
#     targets.
#   - The full 300-query Neo4j diff battery — out of scope for a first cut;
#     wiring the FULL battery across transports (task item 3.2, "wire the
#     per-transport run into CI as the permanent write-path regression net")
#     is a larger follow-up that can reuse the `query_http`/`query_rpc`/
#     `query_resp3`/`normalize` helpers below.
#
# ============================================================================

set -uo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "$SCRIPT_DIR/../.." && pwd)"

NEXUS_HTTP_ADDR="${NEXUS_HTTP_ADDR:-127.0.0.1:15474}"
NEXUS_RPC_ADDR="${NEXUS_RPC_ADDR:-127.0.0.1:15475}"
NEXUS_RESP3_HOST="${NEXUS_RESP3_HOST:-host.docker.internal}"
NEXUS_RESP3_PORT="${NEXUS_RESP3_PORT:-15476}"
DOCKER_REDIS_IMAGE="${DOCKER_REDIS_IMAGE:-redis:7-alpine}"
DOCKER_JQ_IMAGE="${DOCKER_JQ_IMAGE:-ghcr.io/jqlang/jq}"
NEXUS_CLI="${NEXUS_CLI:-}"

# --- resolve the nexus CLI binary -------------------------------------------
if [ -z "$NEXUS_CLI" ]; then
    if [ -x "$REPO_ROOT/target/release/nexus.exe" ]; then
        NEXUS_CLI="$REPO_ROOT/target/release/nexus.exe"
    elif [ -x "$REPO_ROOT/target/release/nexus" ]; then
        NEXUS_CLI="$REPO_ROOT/target/release/nexus"
    else
        echo "ERROR: nexus CLI binary not found at target/release/nexus(.exe)." >&2
        echo "       Build it first: cargo +nightly build --release --package nexus-cli" >&2
        echo "       Or point NEXUS_CLI at an existing build." >&2
        exit 1
    fi
fi

# --- required tools ----------------------------------------------------------
for tool in curl docker; do
    if ! command -v "$tool" >/dev/null 2>&1; then
        echo "ERROR: required tool '$tool' not found on PATH." >&2
        exit 1
    fi
done

# --- health check (never starts/stops the server ourselves) -----------------
health_code="$(curl -s -m 5 -o /dev/null -w '%{http_code}' "http://${NEXUS_HTTP_ADDR}/health" 2>/dev/null || true)"
if [ "$health_code" != "200" ]; then
    echo "ERROR: nexus-server HTTP health check failed (http://${NEXUS_HTTP_ADDR}/health -> '${health_code}')." >&2
    echo "       Start the server first — this script never manages its lifecycle." >&2
    exit 1
fi

echo "Nexus transport-parity harness"
echo "  HTTP:  http://${NEXUS_HTTP_ADDR}"
echo "  RPC:   nexus://${NEXUS_RPC_ADDR}"
echo "  RESP3: ${NEXUS_RESP3_HOST}:${NEXUS_RESP3_PORT} (via dockerized redis-cli)"
echo "  CLI:   ${NEXUS_CLI}"
echo ""

PASS=0
FAIL=0
FAILED_NAMES=()

# ============================================================================
# Transport clients — one function per wire protocol, same {query, params}
# contract, same stdout contract (whatever the tool prints; may or may not
# be valid JSON on error — `normalize` below handles both).
# ============================================================================

query_http() {
    local query="$1" params="$2"
    "$NEXUS_CLI" --json --url "http://${NEXUS_HTTP_ADDR}" query "$query" --params "$params" 2>&1
}

query_rpc() {
    local query="$1" params="$2"
    "$NEXUS_CLI" --json --url "nexus://${NEXUS_RPC_ADDR}" query "$query" --params "$params" 2>&1
}

query_resp3() {
    local query="$1" params="$2"
    docker run --rm "$DOCKER_REDIS_IMAGE" redis-cli \
        -h "$NEXUS_RESP3_HOST" -p "$NEXUS_RESP3_PORT" --json \
        CYPHER.WITH "$query" "$params" 2>&1
}

# Reduce a transport's raw stdout to a stable, comparable text: only the
# `columns` and `rows` fields survive (transport-specific `stats` /
# `execution_time_ms` are expected to differ and are not part of the
# correctness contract), compacted and key-sorted so pretty-printed
# (nexus CLI) and single-line (redis-cli) JSON compare equal. Non-JSON
# output (a CLI error message, a Cypher error string) normalizes to an
# `ERROR:<first line>` sentinel so an error state is never silently
# confused with a real, matching payload.
normalize() {
    local raw="$1" out
    out="$(printf '%s' "$raw" | docker run --rm -i "$DOCKER_JQ_IMAGE" -cS '{columns, rows}' 2>/dev/null)"
    if [ -z "$out" ]; then
        out="ERROR:$(printf '%s' "$raw" | head -n1)"
    fi
    printf '%s' "$out"
}

# Substitute the `__T__` placeholder in a Cypher template with a literal,
# per-transport label suffix (Http / Rpc / Resp3) — see the file header for
# why this is a literal substitution rather than a $param.
render() {
    local template="$1" suffix="$2"
    printf '%s' "${template//__T__/$suffix}"
}

run_case() {
    local name="$1" setup_template="$2" probe_template="$3" params="$4"

    local http_setup rpc_setup resp3_setup
    http_setup="$(render "$setup_template" "Http")"
    rpc_setup="$(render "$setup_template" "Rpc")"
    resp3_setup="$(render "$setup_template" "Resp3")"

    if [ -n "$setup_template" ]; then
        query_http "$http_setup" "$params" >/dev/null
        query_rpc "$rpc_setup" "$params" >/dev/null
        query_resp3 "$resp3_setup" "$params" >/dev/null
    fi

    local http_probe rpc_probe resp3_probe
    http_probe="$(render "$probe_template" "Http")"
    rpc_probe="$(render "$probe_template" "Rpc")"
    resp3_probe="$(render "$probe_template" "Resp3")"

    local http_raw rpc_raw resp3_raw
    http_raw="$(query_http "$http_probe" "$params")"
    rpc_raw="$(query_rpc "$rpc_probe" "$params")"
    resp3_raw="$(query_resp3 "$resp3_probe" "$params")"

    local http_norm rpc_norm resp3_norm
    http_norm="$(normalize "$http_raw")"
    rpc_norm="$(normalize "$rpc_raw")"
    resp3_norm="$(normalize "$resp3_raw")"

    if [ "$http_norm" = "$rpc_norm" ] && [ "$rpc_norm" = "$resp3_norm" ]; then
        echo "OK        $name"
        PASS=$((PASS + 1))
    else
        echo "DIVERGENT $name"
        echo "  HTTP:  $http_norm"
        echo "  RPC:   $rpc_norm"
        echo "  RESP3: $resp3_norm"
        FAIL=$((FAIL + 1))
        FAILED_NAMES+=("$name")
    fi
}

# ============================================================================
# Battery.
# ============================================================================

# Wipe any leftover probe data from a previous (possibly interrupted) run.
query_http "MATCH (n:TPParity) DETACH DELETE n" '{}' >/dev/null

run_case \
    "TC1 parameterized CREATE round-trip (B6)" \
    "" \
    "CREATE (n:TPParity:TPCreate {val: \$v}) RETURN n.val AS val" \
    '{"v": 99}'

run_case \
    "TC2 MERGE-rel with inline props (B1)" \
    "MERGE (a:TPParity:TPMergeRel__T__ {side: 'a'}) MERGE (b:TPParity:TPMergeRel__T__ {side: 'b'})" \
    "MATCH (a:TPParity:TPMergeRel__T__ {side: 'a'}), (b:TPParity:TPMergeRel__T__ {side: 'b'}) MERGE (a)-[r:TP_REL {weight: 7}]->(b) RETURN count(r) AS rel_count, r.weight AS weight" \
    '{}'

run_case \
    "TC3 CREATE ... RETURN r.prop same statement (B3)" \
    "" \
    "CREATE (a:TPParity:TPCreateRel)-[r:TP_REL3 {w: 11}]->(b:TPParity:TPCreateRel) RETURN r.w AS w" \
    '{}'

run_case \
    "TC4 SET on a relationship variable (B2)" \
    "MERGE (a:TPParity:TPSetRel__T__ {side: 'a'}) MERGE (b:TPParity:TPSetRel__T__ {side: 'b'}) MERGE (a)-[r:TP_REL2]->(b)" \
    "MATCH (a:TPParity:TPSetRel__T__ {side: 'a'})-[r:TP_REL2]->(b:TPParity:TPSetRel__T__ {side: 'b'}) SET r.audited = true RETURN r.audited AS audited" \
    '{}'

run_case \
    "TC5 aggregation, mixed RETURN column order" \
    "MERGE (n1:TPParity:TPAgg__T__ {tag: 'a', val: 5}) MERGE (n2:TPParity:TPAgg__T__ {tag: 'b', val: 15})" \
    "MATCH (n:TPParity:TPAgg__T__) RETURN max(n.val) AS max_val, min(n.val) AS min_val, count(n) AS cnt" \
    '{}'

run_case \
    "TC6 UNWIND + MERGE batch write" \
    "" \
    "UNWIND \$items AS item MERGE (n:TPParity:TPBatch__T__ {idx: item}) RETURN count(n) AS cnt" \
    '{"items": [1, 2, 3, 4, 5]}'

run_case \
    "TC7 DELETE + RETURN count(n)" \
    "MERGE (n:TPParity:TPDelete__T__)" \
    "MATCH (n:TPParity:TPDelete__T__) DELETE n RETURN count(n) AS deleted_count" \
    '{}'

# Final teardown — leave the shared database exactly as clean as we found it.
query_http "MATCH (n:TPParity) DETACH DELETE n" '{}' >/dev/null

# ============================================================================
# Summary.
# ============================================================================

echo ""
echo "-----------------------------------------------------------------"
echo "Total: $((PASS + FAIL))   OK: $PASS   DIVERGENT: $FAIL"
if [ "$FAIL" -gt 0 ]; then
    echo "Divergent cases:"
    for n in "${FAILED_NAMES[@]}"; do
        echo "  - $n"
    done
    exit 1
fi
echo "All cases agree across HTTP, RPC, and RESP3."
exit 0
