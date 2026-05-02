#!/bin/bash
# phase9_external-node-ids — end-to-end test against a live Nexus container.
#
# Usage:
#   ./scripts/compatibility/test-external-ids-docker.sh [host:port]
#
# Default endpoint: http://localhost:15474

set -e

HOST="${1:-http://localhost:15474}"
PASS=0
FAIL=0
TOTAL=0

# ── Helpers ──────────────────────────────────────────────────────────

color_green() { printf "\033[32m%s\033[0m\n" "$1"; }
color_red() { printf "\033[31m%s\033[0m\n" "$1"; }
color_yellow() { printf "\033[33m%s\033[0m\n" "$1"; }

assert_eq() {
    local name="$1"; local actual="$2"; local expected="$3"
    TOTAL=$((TOTAL + 1))
    if [ "$actual" = "$expected" ]; then
        PASS=$((PASS + 1))
        color_green "  PASS  $name"
    else
        FAIL=$((FAIL + 1))
        color_red "  FAIL  $name"
        echo "        expected: $expected"
        echo "        actual:   $actual"
    fi
}

assert_contains() {
    local name="$1"; local actual="$2"; local needle="$3"
    TOTAL=$((TOTAL + 1))
    if echo "$actual" | grep -q -- "$needle"; then
        PASS=$((PASS + 1))
        color_green "  PASS  $name"
    else
        FAIL=$((FAIL + 1))
        color_red "  FAIL  $name"
        echo "        looking for: $needle"
        echo "        in:          $actual"
    fi
}

curl_post_cypher() {
    local query="$1"; local params="${2:-{}}"
    curl -s -X POST "$HOST/cypher" \
        -H "Content-Type: application/json" \
        -d "{\"query\":$(jq -Rs . <<< "$query"),\"parameters\":$params}"
}

curl_post_node() {
    local body="$1"
    curl -s -X POST "$HOST/data/nodes" \
        -H "Content-Type: application/json" \
        -d "$body"
}

curl_get_by_ext() {
    local ext="$1"
    curl -s "$HOST/data/nodes/by-external-id?external_id=$(jq -nr --arg s "$ext" '$s|@uri')"
}

# ── Test cases ───────────────────────────────────────────────────────

echo
echo "=== phase9 external-node-ids — Docker e2e suite ==="
echo "    target: $HOST"
echo

# ─── 1. Health ──────────────────────────────────────────────────────
echo "[1] Health check"
HEALTH=$(curl -s "$HOST/health")
assert_contains "health endpoint reachable" "$HEALTH" "ok\|healthy\|status"

# ─── 2. REST: POST /data/nodes with external_id (sha256) ────────────
echo
echo "[2] REST POST /data/nodes — sha256 external id"
SHA="sha256:1111111111111111111111111111111111111111111111111111111111111111"
RES=$(curl_post_node "{\"labels\":[\"FileSha\"],\"properties\":{\"name\":\"a.txt\"},\"external_id\":\"$SHA\"}")
NODE_ID=$(echo "$RES" | jq -r '.node_id // empty')
ERR=$(echo "$RES" | jq -r '.error // empty')
[ -z "$ERR" ] && PASS=$((PASS+1)) || { FAIL=$((PASS+1)); color_red "  FAIL  create returned error: $ERR"; }
TOTAL=$((TOTAL+1))
[ -z "$ERR" ] && color_green "  PASS  create with sha256 external id (node_id=$NODE_ID)"

# ─── 3. REST: GET by external id round-trip ─────────────────────────
echo
echo "[3] REST GET /data/nodes/by-external-id"
RES=$(curl_get_by_ext "$SHA")
GOT_ID=$(echo "$RES" | jq -r '.node.id // empty')
assert_eq "round-trip resolves same internal id" "$GOT_ID" "$NODE_ID"

# ─── 4. UUID variant ────────────────────────────────────────────────
echo
echo "[4] UUID variant"
UUID="uuid:11111111-1111-1111-1111-111111111111"
RES=$(curl_post_node "{\"labels\":[\"FileUuid\"],\"properties\":{},\"external_id\":\"$UUID\"}")
ERR=$(echo "$RES" | jq -r '.error // empty')
TOTAL=$((TOTAL+1))
[ -z "$ERR" ] && { PASS=$((PASS+1)); color_green "  PASS  uuid variant accepted"; } || { FAIL=$((FAIL+1)); color_red "  FAIL  $ERR"; }

# ─── 5. Str variant ─────────────────────────────────────────────────
echo
echo "[5] Str variant"
STR="str:my-natural-key-doc-42"
RES=$(curl_post_node "{\"labels\":[\"FileStr\"],\"properties\":{},\"external_id\":\"$STR\"}")
ERR=$(echo "$RES" | jq -r '.error // empty')
TOTAL=$((TOTAL+1))
[ -z "$ERR" ] && { PASS=$((PASS+1)); color_green "  PASS  str variant accepted"; } || { FAIL=$((FAIL+1)); color_red "  FAIL  $ERR"; }

# ─── 6. Bytes variant ───────────────────────────────────────────────
echo
echo "[6] Bytes variant"
BYTES="bytes:deadbeef"
RES=$(curl_post_node "{\"labels\":[\"FileBytes\"],\"properties\":{},\"external_id\":\"$BYTES\"}")
ERR=$(echo "$RES" | jq -r '.error // empty')
TOTAL=$((TOTAL+1))
[ -z "$ERR" ] && { PASS=$((PASS+1)); color_green "  PASS  bytes variant accepted"; } || { FAIL=$((FAIL+1)); color_red "  FAIL  $ERR"; }

# ─── 7. Blake3 variant ──────────────────────────────────────────────
echo
echo "[7] Blake3 variant"
B3="blake3:2222222222222222222222222222222222222222222222222222222222222222"
RES=$(curl_post_node "{\"labels\":[\"FileB3\"],\"properties\":{},\"external_id\":\"$B3\"}")
ERR=$(echo "$RES" | jq -r '.error // empty')
TOTAL=$((TOTAL+1))
[ -z "$ERR" ] && { PASS=$((PASS+1)); color_green "  PASS  blake3 variant accepted"; } || { FAIL=$((FAIL+1)); color_red "  FAIL  $ERR"; }

# ─── 8. Sha512 variant ──────────────────────────────────────────────
echo
echo "[8] Sha512 variant"
S512="sha512:33333333333333333333333333333333333333333333333333333333333333333333333333333333333333333333333333333333333333333333333333333333"
RES=$(curl_post_node "{\"labels\":[\"FileS512\"],\"properties\":{},\"external_id\":\"$S512\"}")
ERR=$(echo "$RES" | jq -r '.error // empty')
TOTAL=$((TOTAL+1))
[ -z "$ERR" ] && { PASS=$((PASS+1)); color_green "  PASS  sha512 variant accepted"; } || { FAIL=$((FAIL+1)); color_red "  FAIL  $ERR"; }

# ─── 9. Conflict policy: error (default) ───────────────────────────
echo
echo "[9] Conflict policy ERROR (default) on duplicate"
RES=$(curl_post_node "{\"labels\":[\"FileSha\"],\"properties\":{\"name\":\"dup\"},\"external_id\":\"$SHA\"}")
ERR=$(echo "$RES" | jq -r '.error // empty')
TOTAL=$((TOTAL+1))
if [ -n "$ERR" ]; then
    PASS=$((PASS+1)); color_green "  PASS  duplicate rejected: $ERR"
else
    FAIL=$((FAIL+1)); color_red "  FAIL  duplicate was accepted (should have errored)"
fi

# ─── 10. Conflict policy: match ────────────────────────────────────
echo
echo "[10] Conflict policy MATCH returns existing id"
RES=$(curl_post_node "{\"labels\":[\"FileSha\"],\"properties\":{\"name\":\"ignored\"},\"external_id\":\"$SHA\",\"conflict_policy\":\"match\"}")
GOT_ID=$(echo "$RES" | jq -r '.node_id // empty')
assert_eq "match returns existing id" "$GOT_ID" "$NODE_ID"

# ─── 11. Conflict policy: replace ──────────────────────────────────
echo
echo "[11] Conflict policy REPLACE overwrites properties"
RES=$(curl_post_node "{\"labels\":[\"FileSha\"],\"properties\":{\"name\":\"updated\"},\"external_id\":\"$SHA\",\"conflict_policy\":\"replace\"}")
GOT_ID=$(echo "$RES" | jq -r '.node_id // empty')
assert_eq "replace returns same id" "$GOT_ID" "$NODE_ID"

# ─── 12. Cypher CREATE with _id literal ────────────────────────────
echo
echo "[12] Cypher CREATE with _id string literal"
CYPHER_ID="sha256:4444444444444444444444444444444444444444444444444444444444444444"
RES=$(curl_post_cypher "CREATE (n:Doc {_id: '$CYPHER_ID', name: 'cypher_lit'}) RETURN n._id")
ROWS=$(echo "$RES" | jq -r '.rows[0][0] // empty')
assert_eq "RETURN n._id projects prefixed string" "$ROWS" "$CYPHER_ID"

# ─── 13. Cypher CREATE with $_id parameter ─────────────────────────
echo
echo "[13] Cypher CREATE with parameter _id"
PARAM_ID="uuid:55555555-5555-5555-5555-555555555555"
RES=$(curl_post_cypher "CREATE (n:Doc {_id: \$ext_id, name: 'cypher_param'}) RETURN n._id" "{\"ext_id\":\"$PARAM_ID\"}")
ROWS=$(echo "$RES" | jq -r '.rows[0][0] // empty')
assert_eq "param-form projects same prefixed string" "$ROWS" "$PARAM_ID"

# ─── 14. Cypher ON CONFLICT MATCH idempotent ───────────────────────
echo
echo "[14] Cypher CREATE ... ON CONFLICT MATCH idempotent"
ON_MATCH_ID="uuid:66666666-6666-6666-6666-666666666666"
RES1=$(curl_post_cypher "CREATE (n:Doc {_id: '$ON_MATCH_ID'}) ON CONFLICT MATCH RETURN n._id")
RES2=$(curl_post_cypher "CREATE (n:Doc {_id: '$ON_MATCH_ID'}) ON CONFLICT MATCH RETURN n._id")
ROWS1=$(echo "$RES1" | jq -r '.rows[0][0] // empty')
ROWS2=$(echo "$RES2" | jq -r '.rows[0][0] // empty')
assert_eq "first run returns external id" "$ROWS1" "$ON_MATCH_ID"
assert_eq "second run also returns external id (no error)" "$ROWS2" "$ON_MATCH_ID"

# ─── 15. Cypher ON CONFLICT REPLACE ────────────────────────────────
echo
echo "[15] Cypher CREATE ... ON CONFLICT REPLACE"
RES=$(curl_post_cypher "CREATE (n:Doc {_id: '$ON_MATCH_ID', tag: 'replaced'}) ON CONFLICT REPLACE RETURN n._id, n.tag")
ROWS=$(echo "$RES" | jq -r '.rows[0][0] // empty')
assert_eq "replace returns the same external id" "$ROWS" "$ON_MATCH_ID"

# ─── 16. Cypher ON CONFLICT ERROR explicit ─────────────────────────
echo
echo "[16] Cypher CREATE ... ON CONFLICT ERROR rejects duplicate"
RES=$(curl_post_cypher "CREATE (n:Doc {_id: '$ON_MATCH_ID'}) ON CONFLICT ERROR")
ERR=$(echo "$RES" | jq -r '.error // empty')
TOTAL=$((TOTAL+1))
if [ -n "$ERR" ]; then
    PASS=$((PASS+1)); color_green "  PASS  ON CONFLICT ERROR rejected duplicate"
else
    FAIL=$((FAIL+1)); color_red "  FAIL  ON CONFLICT ERROR accepted duplicate"
fi

# ─── 17. RETURN n._id null when unset ──────────────────────────────
echo
echo "[17] RETURN n._id is null when no external id was set"
curl_post_cypher "CREATE (n:NoExt {name: 'plain'})" > /dev/null
RES=$(curl_post_cypher "MATCH (n:NoExt {name: 'plain'}) RETURN n._id")
ROWS=$(echo "$RES" | jq -r '.rows[0][0]')
assert_eq "n._id is null on plain node" "$ROWS" "null"

# ─── 18. Invalid external_id format rejected ───────────────────────
echo
echo "[18] Invalid external_id format rejected"
RES=$(curl_post_node "{\"labels\":[\"X\"],\"properties\":{},\"external_id\":\"not-a-real-prefix:zzz\"}")
ERR=$(echo "$RES" | jq -r '.error // empty')
TOTAL=$((TOTAL+1))
if echo "$ERR" | grep -q "Invalid external_id"; then
    PASS=$((PASS+1)); color_green "  PASS  invalid external_id rejected with clear error"
else
    FAIL=$((FAIL+1)); color_red "  FAIL  expected 'Invalid external_id' error, got: $ERR"
fi

# ─── 19. Invalid conflict_policy rejected ──────────────────────────
echo
echo "[19] Invalid conflict_policy rejected"
RES=$(curl_post_node "{\"labels\":[\"X\"],\"properties\":{},\"external_id\":\"uuid:77777777-7777-7777-7777-777777777777\",\"conflict_policy\":\"ignore\"}")
ERR=$(echo "$RES" | jq -r '.error // empty')
TOTAL=$((TOTAL+1))
if echo "$ERR" | grep -q "Invalid conflict_policy"; then
    PASS=$((PASS+1)); color_green "  PASS  invalid conflict_policy rejected"
else
    FAIL=$((FAIL+1)); color_red "  FAIL  expected 'Invalid conflict_policy' error, got: $ERR"
fi

# ─── 20. GET by absent external id returns 200 with no node ────────
echo
echo "[20] GET by absent external id"
ABSENT="uuid:88888888-8888-8888-8888-888888888888"
RES=$(curl_get_by_ext "$ABSENT")
NODE=$(echo "$RES" | jq -r '.node // "null"')
assert_eq "absent external id returns null node" "$NODE" "null"

# ─── 21. Cypher CREATE with invalid _id parses but errors at runtime ─
echo
echo "[21] Cypher invalid _id format errors at execute time"
RES=$(curl_post_cypher "CREATE (n:X {_id: 'not-a-real-prefix:zz'})")
ERR=$(echo "$RES" | jq -r '.error // empty')
TOTAL=$((TOTAL+1))
if [ -n "$ERR" ]; then
    PASS=$((PASS+1)); color_green "  PASS  invalid _id surface a runtime error"
else
    FAIL=$((FAIL+1)); color_red "  FAIL  invalid _id was accepted"
fi

# ─── Summary ───────────────────────────────────────────────────────
echo
echo "==========================================="
echo "  Total: $TOTAL    Pass: $PASS    Fail: $FAIL"
echo "==========================================="
[ "$FAIL" -eq 0 ] && exit 0 || exit 1
