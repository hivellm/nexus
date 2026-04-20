# nexus-bench

Comparative benchmark harness for Nexus. Pointed at a **running**
Nexus server over HTTP. **Never** starts an engine by itself.

## Why this exists (and why the design is restrictive)

The first draft of this crate spawned an in-process engine on every
`cargo test`, replayed ~280 Cypher statements to load a 10 000-node
dataset, and saturated a developer's workstation for minutes. This
rewrite enforces four hard rules so that failure mode cannot return:

1. **No engine in-process.** No `nexus-core` dependency. The harness
   speaks HTTP/JSON to a server the operator has already started.
2. **Tiny dataset only.** The `tiny` dataset is 100 nodes hardcoded
   as a **single** `CREATE` statement. One round-trip through the
   parser; no fan-out.
3. **All network tests are `#[ignore]` + feature-gated.** `cargo test
   -p nexus-bench` runs only pure-logic unit tests in milliseconds.
4. **Debug-build refusal.** The `nexus-bench` binary refuses to run
   when `cfg!(debug_assertions)` is on — benchmark numbers from a
   debug build are meaningless and 10–100× slower than release.
   Override with `NEXUS_BENCH_ALLOW_DEBUG=1` only for smoke checks.

## Build

```bash
# Pure-logic library + unit tests — no network, no binary.
cargo build -p nexus-bench
cargo test -p nexus-bench

# CLI binary + HTTP client.
cargo build -p nexus-bench --features live-bench --release --bin nexus-bench

# Integration tests against a live server (all #[ignore]).
NEXUS_BENCH_URL=http://127.0.0.1:15474 \
    cargo test -p nexus-bench --features live-bench -- --ignored
```

## Use

```bash
# Start a Nexus server in one terminal (admission control keeps it
# safe even under bench load):
./target/release/nexus-server &

# Dry run — health probe only, no Cypher sent:
./target/release/nexus-bench --url http://127.0.0.1:15474

# Actual run:
./target/release/nexus-bench \
    --url http://127.0.0.1:15474 \
    --i-have-a-server-running \
    --load-dataset
```

## Ceilings

Every dimension the harness exposes has a hard upper bound baked
into the library; callers cannot configure around it.

| Dimension | Ceiling | Where |
|---|---|---|
| Scenario timeout | 30 s | `scenario::MAX_TIMEOUT` |
| Measured iterations | 500 | `scenario::MAX_MEASURED_ITERS` |
| Measured multiplier | 5.0 | `harness::MAX_MULTIPLIER` |
| HTTP request (global) | 10 s | `reqwest::Client::builder().timeout()` |
| `/health` probe | 2 s | `HttpClient::connect` |

A pathological configuration still completes in bounded wall-clock
time, because the ceiling on each dimension is multiplied rather
than added.

## What ships

- `TinyDataset` — 100-node static fixture, single-CREATE load.
- `Scenario` + `ScenarioBuilder` with clamped defaults.
- `run_scenario` — generic over a narrow `BenchExecute` trait, with
  a divergence guard against expected row counts.
- `HttpClient` — `reqwest`-backed; feature-gated behind `live-bench`.
- `MarkdownReport` + `JsonReport` — pure string / serde output.
- 9 seed scenarios across scalar / point-read / label-scan /
  aggregation / filter / order categories.
- `nexus-bench` CLI — dry-run by default, explicit flag to actually
  send traffic.

## What does NOT ship (intentionally)

- Engine instantiation. If you want in-process numbers, write a
  custom `BenchExecute` impl in your own crate.
- Large datasets (`social`, `vector`, LDBC-SNB). A future follow-up
  may add them, but **only** if the load can stay a single HTTP
  round-trip or the server grows a bulk-import endpoint that does
  the fan-out on its side of the wire.
- A Neo4j / Bolt client. Tracked as a follow-up; until it ships,
  the Neo4j column in the report is `—`.

## Sanity

If you ever see this crate doing something that looks like it could
wedge a machine — spawning an engine, running a loop over hundreds of
queries without a ceiling, dropping the `#[ignore]` on an I/O test —
file a bug. The guard rails above are load-bearing.
