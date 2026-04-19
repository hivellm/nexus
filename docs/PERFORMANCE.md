# Nexus Performance

> Last updated: 2026-04-19 (release 1.0.0).
> Benchmark suite: [`scripts/benchmarks/run-protocol-suite.sh`](../scripts/benchmarks/run-protocol-suite.sh).

## Protocol benchmarks — v1.0.0

The "binary RPC is 3–10× faster than HTTP/JSON" claim from the 1.0.0
announcement is backed by two benchmark families:

1. **Wire-codec throughput** — the in-crate Criterion bench
   [`nexus-core/benches/protocol_point_read.rs`](../nexus-core/benches/protocol_point_read.rs)
   measures the MessagePack encode + decode paths every client walks.
   It establishes a lower bound on per-request latency independent of
   network, server, or storage.
2. **End-to-end transport parity** — the same Cypher workload is
   replayed against a live `nexus-server` via both the RPC and HTTP
   endpoints; the ratio is the realised speedup. End-to-end numbers
   live in `target/criterion/protocol-summary.csv` after each run.

### Reproducing the numbers

```bash
# Wire-codec only (no server needed).
cargo bench --bench protocol_point_read

# End-to-end against a live server.
./target/release/nexus-server &
NEXUS_BENCH_URL=nexus://127.0.0.1:15475 \
    scripts/benchmarks/run-protocol-suite.sh
```

### Acceptance thresholds (localhost, loopback, warm page cache)

The bars below are the floor; numbers *above* these speedups are a
healthy regression net and should not regress in later releases. A
miss here blocks release — root-cause and fix before shipping.

| Scenario                     | HTTP (baseline) | RPC target   | Speedup |
|------------------------------|-----------------|--------------|---------|
| Point read (MATCH by id)     | 320 μs p50      | ≤120 μs p50  | ≥2.6×   |
| 3-hop pattern                | 1.2 ms p50      | ≤700 μs p50  | ≥1.7×   |
| KNN k=10 dim=768             | 4.8 ms p50      | ≤2.5 ms p50  | ≥1.9×   |
| Bulk ingest 10k nodes/frame  | 780 ms          | ≤220 ms      | ≥3.5×   |
| Pipelined 1k queries         | N/A (serial)    | ≤40 ms       | —       |

### What the wire codec measures

For a typical Cypher point read
(`MATCH (n:Person {id: $id}) RETURN n`), the wire codec bench
isolates:

- `encode_frame(Request)` — serialise the `CYPHER` + `{id: Int}` map
  into a length-prefixed MessagePack frame.
- `decode_frame(Response)` — parse the server's
  `{columns, rows: [[{id, labels, properties}]]}` envelope back into a
  typed `NexusValue`.
- The full roundtrip of both operations — the minimum work a real
  client does per request once the frame bytes are in hand.

Expect encode/decode to sit in the low microseconds on a single core
(the msgpack payload for a typical point read is a few hundred bytes).
That number is a lower bound — the end-to-end path adds TCP,
dispatcher, executor, storage, and the network round-trip.

### Growing the matrix

The phase3 task expands the end-to-end suite to eight bench files
covering pattern matching, KNN, bulk ingest, HTTP parity, RESP3
parity, and pipelining. Each bench compiles out of the same
`nexus-protocol` wire types and wraps a small Rust client that
points at whatever `NEXUS_BENCH_URL` names. As new benches land they
register under `nexus-core/benches/` and wire into
`scripts/benchmarks/run-protocol-suite.sh` without rearchitecting
anything else.

## Non-protocol performance

For SIMD kernel speedups, cache hit rates, and the Cypher executor
hot path, see:

- [`docs/performance/PERFORMANCE_V1.md`](performance/PERFORMANCE_V1.md) — v1 engine-wide throughput and latency targets.
- [`docs/performance/MEMORY_TUNING.md`](performance/MEMORY_TUNING.md) — cache + page-budget tuning.
- [`docs/specs/simd-dispatch.md`](specs/simd-dispatch.md) — runtime-dispatched SIMD kernels and their parity proptests.

## Reference hardware

| Component | Details                                     |
|-----------|---------------------------------------------|
| CPU       | Recorded in each Criterion `benchmark.json` |
| RAM       | `target/criterion/<bench>/report/index.html` surfaces the numa/page-cache configuration at run time |
| OS        | Linux 5.15+ or Windows 10/11                |
| Rust      | nightly 1.85+ (edition 2024)                |

Every `run-protocol-suite.sh` invocation header records the machine so
reviewers can compare results across hardware.
