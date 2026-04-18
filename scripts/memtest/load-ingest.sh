#!/usr/bin/env bash
# Load scenario: ingestion.
# Creates N_NODES nodes then N_RELS relationships via POST /cypher in chunks.
# Exercises graph_engine mmap growth + page cache.

set -euo pipefail

HOST="${HOST:-http://localhost:15474}"
N_NODES="${N_NODES:-100000}"
N_RELS="${N_RELS:-500000}"
CHUNK="${CHUNK:-500}"

echo "[ingest] target: ${N_NODES} nodes + ${N_RELS} rels via ${HOST}/cypher"

# Wait for server readiness
for i in $(seq 1 30); do
    if curl -sf "${HOST}/health" >/dev/null 2>&1; then break; fi
    sleep 1
done

# Nodes
echo "[ingest] creating ${N_NODES} nodes in chunks of ${CHUNK}"
i=0
while [ "$i" -lt "$N_NODES" ]; do
    END=$((i + CHUNK))
    [ "$END" -gt "$N_NODES" ] && END=$N_NODES
    # Build UNWIND range query
    Q="UNWIND range(${i}, $((END - 1))) AS id CREATE (n:Item {id: id, name: 'item_' + id, payload: 'x'})"
    curl -sf -X POST "${HOST}/cypher" \
        -H "Content-Type: application/json" \
        -d "{\"query\": \"${Q}\"}" >/dev/null
    i=$END
    [ $((i % 10000)) -eq 0 ] && echo "[ingest]   nodes=${i}"
done

# Relationships (random pairs)
echo "[ingest] creating ${N_RELS} rels in chunks of ${CHUNK}"
i=0
while [ "$i" -lt "$N_RELS" ]; do
    END=$((i + CHUNK))
    [ "$END" -gt "$N_RELS" ] && END=$N_RELS
    COUNT=$((END - i))
    Q="UNWIND range(1, ${COUNT}) AS _ MATCH (a:Item), (b:Item) WHERE a.id = toInteger(rand() * ${N_NODES}) AND b.id = toInteger(rand() * ${N_NODES}) CREATE (a)-[:LINKS_TO]->(b)"
    curl -sf -X POST "${HOST}/cypher" \
        -H "Content-Type: application/json" \
        -d "{\"query\": \"${Q}\"}" >/dev/null || true
    i=$END
    [ $((i % 50000)) -eq 0 ] && echo "[ingest]   rels=${i}"
done

echo "[ingest] done"
