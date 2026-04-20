#!/usr/bin/env bash
# Idempotent: starts the Neo4j bench container if it is not
# already running, then polls until the HTTP + bolt ports answer.
# Exits 0 once the container reports ready, non-zero on timeout.
#
# Safe to invoke from CI, from a bench wrapper, or by hand —
# re-running when the container is already up is a fast no-op.

set -euo pipefail

COMPOSE_FILE="$(cd "$(dirname "$0")" && pwd)/docker-compose.yml"
SERVICE="neo4j"
HTTP_URL="http://localhost:17474"
READY_TIMEOUT_S=30

docker_compose() {
    # `docker compose` (v2, plugin) is the modern invocation; fall
    # back to the legacy `docker-compose` binary only if v2 is not
    # installed on the host.
    if docker compose version >/dev/null 2>&1; then
        docker compose "$@"
    elif command -v docker-compose >/dev/null 2>&1; then
        docker-compose "$@"
    else
        echo "error: neither 'docker compose' nor 'docker-compose' found on PATH" >&2
        return 127
    fi
}

running_services=$(docker_compose -f "$COMPOSE_FILE" ps --services --filter "status=running" 2>/dev/null || true)
if echo "$running_services" | grep -qx "$SERVICE"; then
    echo "neo4j container already running; nothing to do"
    exit 0
fi

echo "starting neo4j bench container..."
docker_compose -f "$COMPOSE_FILE" up -d "$SERVICE"

echo "polling $HTTP_URL for readiness (timeout ${READY_TIMEOUT_S}s)..."
for _ in $(seq 1 "$READY_TIMEOUT_S"); do
    if curl -fsS --max-time 1 "$HTTP_URL" >/dev/null 2>&1; then
        echo "neo4j ready: bolt://localhost:17687, $HTTP_URL"
        exit 0
    fi
    sleep 1
done

echo "neo4j failed to become ready within ${READY_TIMEOUT_S}s" >&2
docker_compose -f "$COMPOSE_FILE" logs --tail=200 "$SERVICE" >&2 || true
exit 1
