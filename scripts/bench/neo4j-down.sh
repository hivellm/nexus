#!/usr/bin/env bash
# Idempotent: stops the Neo4j bench container and drops its
# dedicated volume. Re-running when the container is already gone
# is a fast no-op that still returns 0.
#
# `compose down -v` is the full reset that the bench harness
# relies on between runs — the next `neo4j-up.sh` starts on an
# empty database, which is the contract the divergence guard
# expects (both engines see the same seed dataset, deterministic
# scenario output).

set -euo pipefail

COMPOSE_FILE="$(cd "$(dirname "$0")" && pwd)/docker-compose.yml"

docker_compose() {
    if docker compose version >/dev/null 2>&1; then
        docker compose "$@"
    elif command -v docker-compose >/dev/null 2>&1; then
        docker-compose "$@"
    else
        echo "error: neither 'docker compose' nor 'docker-compose' found on PATH" >&2
        return 127
    fi
}

echo "stopping neo4j bench container and dropping nexus-bench-neo4j-data volume..."
docker_compose -f "$COMPOSE_FILE" down -v --remove-orphans

echo "neo4j bench container and volume removed"
