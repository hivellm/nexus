# nexus-bench

Comparative benchmark harness for Nexus. Points at a **running**
Nexus RPC listener and (optionally) a **running** Neo4j Bolt
server. **Never** starts an engine by itself.

## Transport: both sides binary, on purpose

- Nexus: native length-prefixed MessagePack RPC
  (`nexus_protocol::rpc`).
- Neo4j: Bolt via `neo4rs`.

HTTP/JSON is deliberately not a transport. A `Nexus-HTTP ↔ Neo4j-Bolt`
comparison measures JSON serialisation overhead, not engine work;
with both sides on binary wire the `Ratio` column in the report
reflects actual query cost.

## Why this crate is restrictive

The first draft spawned an in-process engine on every `cargo
test`, replayed ~280 Cypher statements to load a 10 000-node
dataset, and saturated a developer's workstation for minutes. This
rewrite enforces four hard rules so that failure mode cannot
return:

1. **No engine in-process.** No `nexus-core` dependency. The
   harness speaks `nexus_protocol::rpc` to a server the operator
   has already started.
2. **Tiny dataset only.** The `tiny` dataset is 100 nodes + 50
   `KNOWS` edges hardcoded as a **single** `CREATE` statement.
   One round-trip through the parser; no fan-out.
3. **All network tests are `#[ignore]` + feature-gated.** `cargo
   test -p nexus-bench` runs only pure-logic unit tests in
   milliseconds.
4. **Debug-build refusal.** The `nexus-bench` binary refuses to
   run when `cfg!(debug_assertions)` is on — benchmark numbers
   from a debug build are meaningless and 10–100× slower than
   release. Override with `NEXUS_BENCH_ALLOW_DEBUG=1` only for
   smoke checks.

## Build

```bash
# Pure-logic library + unit tests — no network, no binary.
cargo build -p nexus-bench
cargo test  -p nexus-bench

# CLI binary + RPC client.
cargo build -p nexus-bench --features live-bench --release --bin nexus-bench

# CLI + RPC client + Bolt client (for --compare).
cargo build -p nexus-bench --features live-bench,neo4j --release --bin nexus-bench

# Integration tests against live servers (all #[ignore]).
NEXUS_BENCH_RPC_ADDR=127.0.0.1:15475 \
    cargo test -p nexus-bench --features live-bench -- --ignored

NEXUS_BENCH_RPC_ADDR=127.0.0.1:15475 \
NEO4J_BENCH_URL=bolt://127.0.0.1:17687 \
    cargo test -p nexus-bench --features live-bench,neo4j -- --ignored
```

## Use

### Nexus-only baseline

```bash
# Start a Nexus server with the RPC listener enabled (default):
./target/release/nexus-server &

# Dry run — HELLO + PING only, no Cypher sent:
./target/release/nexus-bench --rpc-addr 127.0.0.1:15475

# Actual run — loads the tiny dataset, runs every seed scenario:
./target/release/nexus-bench \
    --rpc-addr 127.0.0.1:15475 \
    --i-have-a-server-running \
    --load-dataset
```

### Comparative mode (requires the `neo4j` feature + a live Neo4j)

```bash
# One-time: bring up the pinned Neo4j container.
./scripts/bench/neo4j-up.sh

# Comparative run — loads the tiny dataset on BOTH engines,
# runs every scenario against both, emits a Markdown report
# with p50 / p95 / ratio / ⭐✅⚠️🚨 classification.
./target/release/nexus-bench \
    --rpc-addr 127.0.0.1:15475 \
    --neo4j-url bolt://127.0.0.1:17687 \
    --compare \
    --i-have-a-server-running \
    --load-dataset \
    --format both --output target/bench/report

# Patch the parity section of the compat report in place.
./scripts/bench/update-parity.sh target/bench/report.json

# Tear down when done.
./scripts/bench/neo4j-down.sh
```

### Authentication

The RPC handshake issues `AUTH` only when credentials are
supplied. Three env vars map to the three CLI flags:

| Env | Flag | Notes |
|---|---|---|
| `NEXUS_BENCH_API_KEY` | `--rpc-api-key` | `AUTH <key>` form |
| `NEXUS_BENCH_USER` | `--rpc-user` | paired with password below |
| `NEXUS_BENCH_PASSWORD` | `--rpc-password` | `AUTH <user> <pass>` form |

A Nexus server with auth disabled accepts the connection without
`AUTH`, so these flags are optional. Ditto the Neo4j side when the
container runs with `NEO4J_AUTH=none`.

## Ceilings

Every dimension the harness exposes has a hard upper bound baked
into the library; callers cannot configure around it.

| Dimension | Ceiling | Where |
|---|---|---|
| Scenario timeout | 30 s | `scenario::MAX_TIMEOUT` |
| Measured iterations | 500 | `scenario::MAX_MEASURED_ITERS` |
| Measured multiplier | 5.0 | `harness::MAX_MULTIPLIER` |
| RPC connect | 5 s | `NexusRpcClient::connect` |
| RPC HELLO / AUTH / PING | 2 s each | `NexusRpcClient::connect` |
| Bolt connect | 5 s | `Neo4jBoltClient::connect` |
| Bolt `RETURN 1` probe | 2 s | `Neo4jBoltClient::connect` |

A pathological configuration still completes in bounded
wall-clock time, because the ceiling on each dimension is
multiplied rather than added.

## What ships

- `TinyDataset` — 100-node + 50-edge static fixture, single
  `CREATE` load.
- `Scenario` + `ScenarioBuilder` with clamped defaults.
- `run_scenario` — generic over a narrow `BenchExecute` trait,
  with a divergence guard against expected row counts.
- `NexusRpcClient` (+ `NexusRpcCredentials`) — native RPC
  transport, feature-gated behind `live-bench`.
- `Neo4jBoltClient` — Bolt via `neo4rs`, feature-gated behind
  `neo4j` (composable with `live-bench`).
- `compare_rows` — cross-engine row-content divergence guard
  (`nexus_bench::divergence`).
- 25 seed scenarios across scalar / point-read / label-scan /
  aggregation / filter / order / traversal / subquery /
  procedure categories.
- `MarkdownReport` + `JsonReport` — pure string / serde output.
- `nexus-bench` CLI — dry-run by default, explicit flag to
  actually send traffic.
- Docker harness: `scripts/bench/docker-compose.yml` +
  `neo4j-up.sh` + `neo4j-down.sh` + `smoke.sh`.
- Parity automation: `scripts/bench/update-parity.sh` rewrites
  the Nexus↔Neo4j section of
  `docs/compatibility/NEO4J_COMPATIBILITY_REPORT.md` from a
  `report.json`.

## What does NOT ship (intentionally)

- HTTP transport. Removed on purpose — see "Transport" above.
- Engine instantiation. If you want in-process numbers, write a
  custom `BenchExecute` impl in your own crate.
- Large datasets (`social`, `vector`, LDBC-SNB). The follow-up
  task `phase6_bench-scenario-expansion` may add a
  `SmallDataset`, but **only** if the load can stay a single
  round-trip or the server grows a bulk-import endpoint that does
  the fan-out server-side.

## Sanity

If you ever see this crate doing something that looks like it
could wedge a machine — spawning an engine, running a loop over
hundreds of queries without a ceiling, dropping the `#[ignore]`
on an I/O test — file a bug. The guard rails above are
load-bearing.
