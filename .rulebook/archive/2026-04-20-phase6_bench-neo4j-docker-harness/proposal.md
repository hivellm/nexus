# Proposal: Neo4j-side of the nexus-bench harness

## Why

`phase6_nexus-vs-neo4j-benchmark-suite` shipped the Nexus-side
(commits `611752f3` + `abd3406a`) as an HTTP-only sandboxed harness.
It runs, it has guard rails, it reports Nexus numbers — but the
comparative half is missing: every `ratio` cell in the Markdown
report is `—` and every `Neo4j p50 (µs)` column is `—`.

The comparative question ("is Nexus faster or slower than Neo4j for
X?") can't be answered without a running Neo4j and a Bolt client the
harness speaks to. That plumbing needs Docker (Neo4j lives in an
isolated container with pinned cache / memory budgets / image
digest), plus a `neo4rs`-based `BenchClient` impl on the harness
side that mirrors the HTTP one.

Keeping this as a separate task means:

- The Nexus-side work stays usable + archivable today — operators
  already get Nexus latency numbers from the seed catalogue.
- The Neo4j-side ships on a slower cadence without blocking the
  Nexus numbers behind it.
- Docker-host concerns (where the runner lives, how to clean up
  containers, how to pin image digests) get a dedicated home
  instead of polluting a generic "bench" task.

## What Changes

### Docker harness

- `scripts/bench/docker-compose.yml` for Neo4j Community 5.15 on a
  pinned image digest, port remap 7687 → 17687 + 7474 → 17474,
  cache 512 MiB, no TLS, no external metrics.
- `scripts/bench/neo4j-up.sh` / `scripts/bench/neo4j-down.sh` —
  idempotent start / stop + data-volume reset.
- README walkthrough: `./scripts/bench/neo4j-up.sh && cargo run
  --release -p nexus-bench --features live-bench,neo4j -- --url ...
  --neo4j-url bolt://... --compare`.

### `neo4rs`-based `BenchClient`

- `crates/nexus-bench/src/client/neo4j.rs` behind a new `neo4j`
  feature flag (composable with `live-bench`).
- `Neo4jBoltClient` with the same narrow `BenchClient` contract as
  `HttpClient`: `engine_name`, `execute`, `reset`.
- Typed per-column row extraction so the divergence guard can
  compare row contents, not just row counts.
- Hard per-call timeout via `tokio::time::timeout` — same 5 s floor
  as `HttpClient`; parent-task guard rails carry over.

### Comparative CLI

- Extend `nexus-bench` to accept `--neo4j-url` + `--compare`.
  Without the flag the CLI behaves exactly like today
  (Nexus-only).
- With `--compare` + `--load-dataset`, the tiny dataset loads on
  BOTH engines before the scenario loop fires.
- Each scenario runs first on Nexus, then on Neo4j; the
  `ComparativeRow` gets both sides populated and the ratio +
  classification (⭐ Lead / ✅ Parity / ⚠️ Behind / 🚨 Gap) surface
  in the report.

### Parity-report automation

- `scripts/bench/update-parity.sh` consumes `report.json` and
  rewrites the Nexus-vs-Neo4j section of
  `docs/compatibility/NEO4J_COMPATIBILITY_REPORT.md` in place.
- Nightly CI job runs the harness + posts a summary.

## Impact

- Affected specs: `bench-docker-harness`, `bench-neo4j-client`,
  `bench-parity-report`.
- Affected code:
  - `scripts/bench/` (new)
  - `crates/nexus-bench/src/client/neo4j.rs` (new, feature-gated)
  - `crates/nexus-bench/src/client/mod.rs` (promote `client.rs` →
    `client/mod.rs` + `client/http.rs` for parallel shape)
  - `crates/nexus-bench/src/bin/nexus-bench.rs` (+ `--neo4j-url`,
    `--compare`)
  - `docs/compatibility/NEO4J_COMPATIBILITY_REPORT.md`
- Breaking change: NO — the comparative pieces are additive. The
  Nexus-only CLI flow continues to work without Docker, without
  `neo4rs`, and without the `neo4j` feature flag.
- User benefit: the report's `Ratio` and classification columns
  stop reading `—`; Phase 6 performance guarantees get a verifiable
  numeric backbone.
