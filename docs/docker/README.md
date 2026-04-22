# Nexus

**High-performance property graph database with native vector search.**

[![License: Apache 2.0](https://img.shields.io/badge/License-Apache_2.0-blue.svg)](https://github.com/hivellm/nexus/blob/main/LICENSE)
[![GitHub](https://img.shields.io/badge/GitHub-hivellm%2Fnexus-blue?logo=github)](https://github.com/hivellm/nexus)

Nexus is a Neo4j-inspired graph engine written in Rust, designed for
read-heavy workloads, retrieval-augmented generation (RAG) systems,
and hybrid graph + vector search. Ships openCypher + native HNSW
vector search in a single binary.

## Quick start

```bash
# Pull + run (HTTP API only, auth off for local dev)
docker run -d \
  --name nexus \
  -p 15474:15474 \
  -p 15475:15475 \
  -v nexus-data:/app/data \
  -e NEXUS_AUTH_ENABLED=false \
  hivellm/nexus:latest

# Smoke-test
curl http://localhost:15474/health
```

```bash
# Cypher over HTTP
curl -X POST http://localhost:15474/cypher \
  -H "Content-Type: application/json" \
  -d '{"query":"CREATE (n:Person {name:\"Alice\"}) RETURN n.name","parameters":{}}'
```

## Supported tags

| Tag | Contents |
|---|---|
| `latest` | Latest stable release (currently `v1.14.0`). |
| `v1.14.0` | Geospatial predicates + `spatial.*` procedures slice A. |
| `v1.13.0` | FTS async writer + crash-recovery harness. |
| `v1.12.0` | FTS auto-maintenance on CREATE / SET / REMOVE / DELETE. |

Every tag ships the HTTP API (`:15474`) and the binary RPC
transport (`:15475`) the first-party SDKs use by default.

## Image layout

- **Base**: `debian:trixie-slim` (glibc 2.41).
- **Binary**: `/usr/local/bin/nexus-server` (single statically-linked
  release build of the workspace).
- **User**: `nexus` (uid 1000), non-root.
- **Data directory**: `/app/data` — mount a volume here for
  persistence.
- **Config directory**: `/app/config` — optional, for `config.yml`.
- **Image size**: ~152 MB.

## Ports

| Port | Purpose | Notes |
|---|---|---|
| `15474` | HTTP API (`/cypher`, `/knn_traverse`, `/health`, `/stats`, …) | Primary entry point for REST clients. |
| `15475` | Native binary RPC (`nexus://host:15475`) | SDK default since v1.10. Leave unpublished for HTTP-only deployments and set `[rpc].enabled = false` in `config.yml`. |

## Environment variables

| Variable | Default | Effect |
|---|---|---|
| `NEXUS_ADDR` | `0.0.0.0:15474` | HTTP bind address. |
| `NEXUS_DATA_DIR` | `/app/data` | Persistent data directory (mount a volume). |
| `NEXUS_AUTH_ENABLED` | `true` | Enable bearer-token authentication on HTTP + RPC. |
| `NEXUS_AUTH_REQUIRED_FOR_PUBLIC` | `true` | Reject unauthenticated requests when binding to a non-localhost address. |
| `NEXUS_ROOT_USERNAME` | `admin` | Initial root user name when auth is enabled. |
| `NEXUS_ROOT_PASSWORD` | _(unset)_ | Initial root password. Prefer `NEXUS_ROOT_PASSWORD_FILE` for production. |
| `NEXUS_ROOT_PASSWORD_FILE` | _(unset)_ | Path to a file containing the root password. Mount via `docker run --secret` or `compose` secrets. |
| `NEXUS_ROOT_ENABLED` | `true` | Provision the root user on first boot. |
| `NEXUS_DISABLE_ROOT_AFTER_SETUP` | `false` | Disable the root user once you have provisioned a replacement admin — recommended for production. |
| `RUST_LOG` | `info` | Log level (`trace`, `debug`, `info`, `warn`, `error`). |

## Production deployment with Docker secrets

```bash
mkdir -p secrets
echo "$(openssl rand -hex 32)" > secrets/root_password.txt
chmod 600 secrets/root_password.txt

docker run -d \
  --name nexus \
  -p 15474:15474 \
  -p 15475:15475 \
  -v nexus-data:/app/data \
  -v $(pwd)/secrets/root_password.txt:/run/secrets/nexus_root_password:ro \
  -e NEXUS_ROOT_USERNAME=admin \
  -e NEXUS_ROOT_PASSWORD_FILE=/run/secrets/nexus_root_password \
  -e NEXUS_AUTH_ENABLED=true \
  -e NEXUS_DISABLE_ROOT_AFTER_SETUP=true \
  hivellm/nexus:latest
```

## docker-compose

```yaml
services:
  nexus:
    image: hivellm/nexus:latest
    container_name: nexus
    ports:
      - "15474:15474"
      - "15475:15475"
    volumes:
      - nexus-data:/app/data
      - ./config:/app/config:ro
    environment:
      NEXUS_ROOT_USERNAME: admin
      NEXUS_ROOT_PASSWORD_FILE: /run/secrets/nexus_root_password
      NEXUS_AUTH_ENABLED: "true"
      RUST_LOG: info
    secrets:
      - nexus_root_password
    restart: unless-stopped

volumes:
  nexus-data:
    driver: local

secrets:
  nexus_root_password:
    file: ./secrets/root_password.txt
```

## Health check

The image ships a `HEALTHCHECK` that polls `/health` every 30 s:

```bash
docker inspect --format='{{.State.Health.Status}}' nexus
# healthy
```

Programmatic probe:

```bash
curl -sf http://localhost:15474/health | jq .status
# "Healthy"
```

## Features

- **openCypher**: ~55 % of the openCypher surface (300/300 Neo4j
  compatibility tests passing at v1.14).
- **Full-text search**: Tantivy-backed BM25 with per-index async
  writers, `db.index.fulltext.*` Neo4j-compatible procedure
  surface, and crash-recovery via WAL replay.
- **Vector search**: HNSW kNN over `f32` embeddings with
  SIMD-accelerated distance kernels (AVX-512 → AVX2 → SSE4.2 →
  NEON → scalar).
- **Geospatial**: `Point` type (Cartesian / WGS-84, 2D / 3D),
  `point.withinBBox` / `withinDistance` / `azimuth` /
  `distance` predicates, `spatial.bbox` / `distance` /
  `nearest` / `interpolate` procedures with `ERR_CRS_MISMATCH`
  / `ERR_BBOX_MALFORMED` error taxonomy.
- **ACID transactions**: epoch-based MVCC with a single-writer
  model; snapshot isolation for readers.
- **Multi-database**: isolated catalogues within a single server
  via the `DatabaseManager`.
- **Binary RPC**: first-party SDKs (Python, TypeScript, Rust, Go,
  C#, PHP) speak `nexus://` natively for p95 sub-millisecond
  round-trips.

## Links

- **Source**: https://github.com/hivellm/nexus
- **Documentation**: https://github.com/hivellm/nexus/tree/main/docs
- **SDKs**: https://github.com/hivellm/nexus/tree/main/sdks
- **Issues**: https://github.com/hivellm/nexus/issues
- **Changelog**: https://github.com/hivellm/nexus/blob/main/CHANGELOG.md

## License

Apache 2.0. © HiveLLM Contributors.
