#!/usr/bin/env bash
# Load scenario: GraphQL N+1.
# Issues queries that trigger allRelationships-style traversal — currently
# materializes outgoing + incoming fully for each node in the page.

set -euo pipefail

HOST="${HOST:-http://localhost:15474}"
N_QUERIES="${N_QUERIES:-500}"

echo "[graphql] ${N_QUERIES} queries against ${HOST}/graphql"

for i in $(seq 1 30); do
    if curl -sf "${HOST}/health" >/dev/null 2>&1; then break; fi
    sleep 1
done

# Probe GraphQL endpoint existence — if absent, fall back to /cypher
PROBE=$(curl -sf -o /dev/null -w "%{http_code}" -X POST "${HOST}/graphql" \
    -H "Content-Type: application/json" \
    -d '{"query":"{ __typename }"}' || echo "000")

if [ "$PROBE" != "200" ]; then
    echo "[graphql] endpoint not reachable (HTTP $PROBE) — exercising equivalent N+1 via /cypher"
    for i in $(seq 1 "$N_QUERIES"); do
        Q="MATCH (n:Item)-[r]-(m) RETURN n.id, collect(r), collect(m) LIMIT 50"
        curl -sf -X POST "${HOST}/cypher" \
            -H "Content-Type: application/json" \
            -d "{\"query\": \"${Q}\"}" >/dev/null || true
        [ $((i % 50)) -eq 0 ] && echo "[graphql]   q=${i}"
    done
else
    for i in $(seq 1 "$N_QUERIES"); do
        GQL='{"query":"{ nodes(label: \"Item\", first: 20) { id allRelationships { type target { id } } } }"}'
        curl -sf -X POST "${HOST}/graphql" \
            -H "Content-Type: application/json" \
            -d "$GQL" >/dev/null || true
        [ $((i % 50)) -eq 0 ] && echo "[graphql]   q=${i}"
    done
fi

echo "[graphql] done"
