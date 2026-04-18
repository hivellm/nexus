# Memory Tuning

Operational reference for Nexus's memory footprint after the memory-leak
remediation pass on `fix/memory-leak-v1`.

## Summary of changes

| Area                    | Before              | After                 | Why it mattered                                                                 |
| ----------------------- | ------------------- | --------------------- | ------------------------------------------------------------------------------- |
| `INITIAL_NODE_CAPACITY` | 1_000_000           | 100_000               | Forced ~56 MB of eager mmap on every cold start                                 |
| `INITIAL_REL_CAPACITY`  | 5_000_000           | 500_000               | Added another ~160 MB eager mmap on cold start                                  |
| `FILE_GROWTH_FACTOR`    | 2.0                 | 1.5                   | Halves the transient spike during grow-and-remap                                |
| Query cache             | 512 MB / 10 000 / 1 h TTL | 64 MB / 1 000 / 10 min TTL | Cache itself was one of the largest in-process consumers                 |
| Vectorizer cache        | 100 MB / 10 000 / 1 h TTL | 32 MB / 1 000 / 10 min TTL | Charged full budget even on deployments that did not use it              |
| HNSW `max_elements`     | Hardcoded 10 000    | `KnnConfig` (default 1 000) | Every label-scoped KNN index allocated ~15 MB up front                     |
| HTTP body limit         | Axum default (2 MB, implicit) | 16 MB via `DefaultBodyLimit` (`NEXUS_MAX_BODY_SIZE_MB`) | No config-driven cap; explicit now                       |
| Executor row ceiling    | Unbounded `Vec`     | `MAX_INTERMEDIATE_ROWS = 1 000 000` (scans + expand + var-length) | One bad query could allocate multi-GB heap       |
| GraphQL rel resolvers   | Unbounded Cypher MATCH | `limit` arg (default 100, cap 500) | N+1 expansion on high-degree nodes                                 |
| Page cache              | Silent eviction stall | `tracing::warn!` when all pages pinned | Now observable                                               |
| Metric collector        | Unbounded key map   | `DEFAULT_MAX_METRICS_SIZE = 10 000` unique keys (updates still allowed) | High-cardinality labels could leak memory        |

Concrete expected impact on a cold, empty server:

- RSS at boot drops from **~220 MB** to **~22 MB**.
- Worst-case cache footprint drops from **~612 MB** (query + vectorizer) to **~96 MB**.
- Queries that would previously exhaust heap now return
  `Error::OutOfMemory` with a clear message telling the caller to add a
  `LIMIT`.

## Environment variables

Runtime overrides currently read by the server (`nexus-server/src/config.rs`):

| Variable                      | Default           | Effect                                         |
| ----------------------------- | ----------------- | ---------------------------------------------- |
| `NEXUS_ADDR`                  | `127.0.0.1:15474` | Bind address                                   |
| `NEXUS_DATA_DIR`              | `./data`          | Storage root                                   |
| `NEXUS_CONFIG_PATH`           | `config.yml`      | YAML file consulted before compiled defaults   |
| `NEXUS_MAX_BODY_SIZE_MB`      | `16`              | HTTP body ceiling in MB (applied as a layer)   |
| `NEXUS_AUTH_ENABLED`          | `false`           | Toggle authentication                          |
| `NEXUS_ROOT_USERNAME` / `…_PASSWORD` / `…_PASSWORD_FILE` | — | Root user bootstrap |

Priority order is env var → YAML file → compiled default, with env vars
always winning. See `nexus-server/src/config.rs::Config::from_env`.

## Memtest harness

`scripts/memtest/` (Dockerfile.memtest + docker-compose.memtest.yml + helpers)
boot the server with `mem_limit=512m` and swap disabled so leaks fail
loudly (OOMKilled) instead of thrashing the host.

```bash
# Baseline — run against whichever revision you want to measure.
bash scripts/memtest/run-all.sh baseline

# After a change, run with a new tag and diff the CSVs.
bash scripts/memtest/run-all.sh phase1
diff memtest-output/baseline-*.csv memtest-output/phase1-*.csv
```

`run-all.sh` exercises three scenarios: bulk ingestion (100k nodes / 500k
rels), KNN (10k vectors × 128-dim + 1k queries), and GraphQL N+1.

## Known follow-ups

These were scoped out of this pass and deserve their own work:

1. **YAML config loading — partial.** `Config::from_yaml_file` now reads
   `server.{addr, max_body_size_mb}` and `storage.{data_dir,
   page_cache.capacity}` from `config.yml` (path overridable via
   `NEXUS_CONFIG_PATH`). Remaining subtrees — `storage.wal`,
   `storage.mvcc`, `storage.knn`, `authentication`, `vectorizer`,
   `metrics` — still fall through to compiled defaults and need
   equivalent wiring.
2. **WAL auto-checkpoint on size threshold.** `max_wal_size_mb` is
   documented in the YAML but no code path triggers `checkpoint()` when
   the file exceeds the threshold. Callers must still drive checkpoints
   explicitly.
3. **Executor streaming.** Cartesian-product paths and the final
   projection still materialise to `Vec<Row>`. The hardcap (scans,
   expand, variable-length) stops the bleeding; true streaming
   evaluation would lift the cap.
4. **GraphQL cursor pagination.** Current `limit` resolves the acute
   issue but does not support stable cursors for paging through large
   neighbourhoods.
5. **MVCC GC verification.** Need a regression test that keeps a long
   snapshot reader alive and asserts that versions of concurrent
   transactions are eventually reclaimed.
6. **Bounded maps.** `MetricCollector.metrics` now has a cardinality
   cap. `memory_management.rs::allocated_blocks` and the `DashMap`
   caches in `catalog/mod.rs` remain unbounded; audit + bound them if
   workloads exercise dynamic labels/keys.

## Raising the caps

Deployments that genuinely need more capacity should bump the compiled
defaults (not the YAML — it isn't wired yet):

- `nexus-core/src/storage/graph_engine/format.rs` — `INITIAL_*_CAPACITY`
- `nexus-core/src/query_cache.rs` — `QueryCacheConfig::default`
- `nexus-core/src/vectorizer_cache.rs` — `CacheConfig::default`
- `nexus-core/src/index/mod.rs` — `KnnConfig::default`, or construct
  `KnnIndex::with_config(dim, KnnConfig { max_elements: …, .. })`
- `nexus-core/src/executor/mod.rs` — `MAX_INTERMEDIATE_ROWS`
- `nexus-server/src/config.rs` — `max_body_size_bytes` default
