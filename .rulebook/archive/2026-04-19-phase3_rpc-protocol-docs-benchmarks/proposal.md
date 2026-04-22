# Proposal: phase3_rpc-protocol-docs-benchmarks

## Why

Shipping RPC and RESP3 without numbers, docs, and a migration story would be a
half-finished feature. Users need three artefacts to actually adopt the new
default transport:

1. **Hard performance numbers.** The claim "3–10x faster" must be backed by
   reproducible criterion benchmarks that any contributor can re-run. Without
   this, future regressions go undetected and marketing claims rot.
2. **A complete wire-format spec.** Third-party SDK authors, observability
   tools, and future HiveLLM projects need the same `.md` spec that Synap
   publishes for its RESP3 and RPC formats. Every decision (endian, framing,
   reserved IDs, error codes) must be written down once so it cannot drift
   across implementations.
3. **An operator runbook.** Ports to open in firewalls, metrics to scrape,
   slow-query thresholds to tune, failure modes to recognize. Ops teams
   cannot promote Nexus to production on vibes.

We also need a **stable baseline** before V2 work — distributed clustering,
streaming Cypher, and live-query subscriptions all depend on the RPC
push-channel semantics being nailed down.

## What Changes

Three deliverables in one phase:

### 1. Wire-format specifications

New docs under `docs/specs/`:

- `rpc-wire-format.md` — binary RPC
  - Frame layout (`[u32 LE len][msgpack body]`), 64 MiB cap
  - `NexusValue` variants, their rmp-serde externally-tagged encoding
  - `Request`/`Response` structure, `id` semantics, reserved `u32::MAX`
    for server push
  - Every command's command name, argument list, response shape
  - Error format: `Response::err(id, message)`; canonical error prefixes
    (`ERR`, `WRONGTYPE`, `NOAUTH`, `TIMEOUT`, `RATE_LIMIT`, `NOTFOUND`)
  - Authentication handshake via `HELLO` / `AUTH`
  - Pipelining rules, backpressure, max in-flight per connection
- `resp3-nexus-commands.md` — RESP3 command reference
  - All 12 type prefixes with example encodings
  - Every Nexus command: syntax, argument types, response encoding
  - Differences from Redis semantics
  - Inline command support (redis-cli, telnet)
- `sdk-transport.md` — how SDKs pick and use a transport
  - `TransportMode` enum semantics
  - Auto-downgrade behavior on connect failure
  - Command-map coverage table (per SDK)
  - Adding a new language SDK: checklist

### 2. Benchmarks

New Criterion benchmarks under `nexus-core/benches/protocol/`:

```
benches/protocol/
|- rpc_cypher_point_read.rs     # 100k simple MATCH queries
|- rpc_cypher_pattern.rs        # 10k 3-hop patterns
|- rpc_knn_search.rs            # 10k KNN queries (k=10, dim=768)
|- rpc_knn_bytes_vs_array.rs    # Bytes vs Array<Float> embedding encoding
|- rpc_ingest_bulk.rs           # 1M nodes batched at 10k per frame
|- http_parity.rs               # same workloads via HTTP for ratio
|- resp3_parity.rs              # same workloads via RESP3 for ratio
|- pipelining.rs                # 1000 in-flight requests per connection
```

Reports generated under `target/criterion/` and committed as a markdown
summary at `docs/PERFORMANCE.md` (§ "Protocol benchmarks — v0.13").
Required acceptance numbers (localhost, loopback, warm page cache):

| Scenario                     | HTTP (baseline) | RPC target  | Speedup |
|------------------------------|-----------------|-------------|---------|
| Point read (MATCH by id)     | 320 us p50      | <120 us p50 | >=2.6x  |
| 3-hop pattern                | 1.2 ms p50      | <700 us p50 | >=1.7x  |
| KNN k=10 dim=768             | 4.8 ms p50      | <2.5 ms p50 | >=1.9x  |
| Bulk ingest 10k nodes/frame  | 780 ms          | <220 ms     | >=3.5x  |
| Pipelined 1k queries         | N/A (serial)    | <40 ms      | N/A     |

If a target is missed, the phase is **blocked** and we root-cause before
shipping — no "we'll fix it later".

### 3. Operator runbook

New file `docs/OPERATING_RPC.md`:

- Ports and firewall: 15474 (HTTP), 15475 (RPC), 15476 (RESP3)
- Bind-address recommendations: RPC public, RESP3 loopback by default
- TLS posture (V1: no native TLS; terminate at LB or stunnel)
- Metrics to scrape: full prometheus catalog for RPC and RESP3
- Alert thresholds: slow-command warnings, frame-size p99, connection
  churn
- Rate limits and DOS posture: `max_in_flight_per_conn`,
  `max_frame_bytes`, per-IP connection caps
- Failure modes and playbook:
  - "Clients report SLOW_COMMAND warnings" -> check planner cache, GC
  - "Frame too large errors" -> tune `max_frame_bytes` or batch smaller
  - "RPC port unreachable but HTTP works" -> firewall, bind host check
- Upgrade path from v0.12 (HTTP-only SDK) to v0.13 (RPC default)

### 4. Migration guide

`docs/MIGRATION_v0.12_to_v0.13.md`:

- What changed (one-paragraph summary)
- What operators must do (open port 15475, maybe 15476)
- What SDK users must do (nothing — RPC is default with auto-downgrade)
- Opt-out path (`NEXUS_SDK_TRANSPORT=http`)
- Rollback plan (set `rpc.enabled = false` in server config; restart)

### 5. Decision records

Capture three ADRs under `.rulebook/decisions/`:

- `2026-nn-native-rpc-over-http.md` — why we picked msgpack+tcp over
  gRPC, QUIC, or WebSocket
- `2026-nn-resp3-compat-layer.md` — why RESP3 (not RESP2, not custom)
- `2026-nn-rpc-default-transport.md` — why RPC is the new SDK default

## Impact

- **Affected specs**: NEW `docs/specs/rpc-wire-format.md`,
  `docs/specs/resp3-nexus-commands.md`, `docs/specs/sdk-transport.md`,
  `docs/OPERATING_RPC.md`, `docs/MIGRATION_v0.12_to_v0.13.md`.
- **Affected code**: NEW `nexus-core/benches/protocol/*.rs` (8 bench files,
  no production code touched).
- **Breaking change**: NO (docs and benchmarks only).
- **User benefit**: reproducible proof, ops confidence, clean upgrade.

## Non-goals

- Implementing TLS for RPC — deferred to V2 alongside clustering TLS.
- A benchmarks dashboard / CI integration — we commit the markdown, but
  wiring a public grafana board is a separate task.
- Cross-language SDK benchmarks — the in-repo benchmarks are Rust-only.
  SDK-level perf numbers live in each SDK's test suite README.

## Reference

Synap published the same artefacts; our docs should mirror their tone
and structure:

- `Synap/docs/specs/resp3-protocol.md`
- `Synap/docs/specs/synap-rpc-wire.md`
- `Synap/docs/PERFORMANCE.md` (§ "Binary protocols")
