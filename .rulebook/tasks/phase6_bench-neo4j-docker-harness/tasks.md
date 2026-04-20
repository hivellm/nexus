## 1. Docker Harness

- [x] 1.1 `scripts/bench/docker-compose.yml` for Neo4j Community 5.15 pinned image digest
- [x] 1.2 Isolated ports (7687 → 17687, 7474 → 17474) + dedicated data volume
- [x] 1.3 Config: cache 512 MiB, no TLS, no external metrics
- [x] 1.4 `scripts/bench/neo4j-up.sh` / `neo4j-down.sh` idempotent lifecycle
- [x] 1.5 Integration smoke: compose up → Bolt PING → compose down, all under 30 s

## 2. `neo4rs`-based BenchClient

- [x] 2.1 Promote `crates/nexus-bench/src/client.rs` → `client/mod.rs` + `client/http.rs` for symmetry
- [x] 2.2 New `client/neo4j.rs` behind a `neo4j` feature flag (composable with `live-bench`)
- [x] 2.3 `Neo4jBoltClient` implements the existing `BenchClient` trait verbatim
- [x] 2.4 Typed per-column row extraction using `neo4rs::Row::get::<T>(...)` — no `Debug` stand-in
- [x] 2.5 Hard `tokio::time::timeout` on every Bolt call
- [x] 2.6 Health probe on `connect`: Bolt `RUN "RETURN 1"` inside 2 s, matching HTTP client contract
- [ ] 2.7 Unit tests for wire conversions (no live server needed)

## 3. Comparative CLI

- [x] 3.1 `--neo4j-url` + `--compare` flags on `nexus-bench` (without them, today's Nexus-only flow stays intact)
- [x] 3.2 `--load-dataset` loads the tiny dataset on BOTH engines
- [x] 3.3 Scenario loop runs Nexus first, Neo4j second, builds `ComparativeRow` with both sides populated
- [x] 3.4 Divergence guard compares row contents, not just counts
- [x] 3.5 Markdown + JSON reports now surface ratios + classifications for every row
- [x] 3.6 Integration tests under `tests/live_compare.rs` — all `#[ignore]`, require both `NEXUS_BENCH_URL` + `NEO4J_BENCH_URL`

## 4. Parity-report automation

- [x] 4.1 `scripts/bench/update-parity.sh` consumes `report.json`
- [x] 4.2 Rewrites the Nexus-vs-Neo4j section of `docs/compatibility/NEO4J_COMPATIBILITY_REPORT.md` in place
- [ ] 4.3 CI step ensures the report is up to date on each PR touching the bench crate
- [ ] 4.4 Nightly job runs the harness + posts a summary comment

## 5. Tail (mandatory — enforced by rulebook v5.3.0)

- [x] 5.1 Update or create documentation covering the implementation — `docs/benchmarks/README.md` with the Docker workflow + `crates/nexus-bench/README.md` comparative-mode section
- [x] 5.2 Write tests covering the new behavior — Bolt unit tests + `tests/live_compare.rs` + a Docker-based CI smoke
- [ ] 5.3 Run tests and confirm they pass — `cargo +nightly test -p nexus-bench --features live-bench,neo4j` under a running docker compose
