# Proposal: Server admission control for the query surface

## Why

A single client can today monopolise the server engine with a burst
of legitimate-looking Cypher calls. Concrete example that triggered
this task: the `nexus-bench` micro-dataset generator fired ~40 000
sequential `CREATE` statements via `Engine::execute_cypher`. Each one
is a full parse + plan + execute cycle; the engine's single-writer
discipline plus debug-build overhead saturated the host for the
entire burst. On a production server behind `/cypher`, the same
shape of traffic (a stuck ingest loop, a misconfigured test, a
malicious client) would wedge the process.

Existing mitigations are insufficient:

- **Rate limiter** (`middleware::RateLimiter`) gates requests by
  API-key / minute / hour quotas. It stops sustained floods, not
  short bursts, and applies per key — a single authenticated client
  still bypasses it with legal quota.
- **Per-connection RPC semaphore** (`protocol/rpc/server.rs`) caps
  concurrent dispatch *per TCP connection* — so one connection
  cannot oversubscribe, but a handful of connections still can.
- **Request body limit** (`DefaultBodyLimit`) caps single-payload
  size; it does not cap arrival rate.

None of these puts a ceiling on concurrent engine-facing work
globally. The engine is the bottleneck the server must protect.

## What Changes

Introduce a global **admission queue** shared by every query-bearing
endpoint (`/cypher`, `/ingest`, the RPC `CYPHER` command, RESP3
`CYPHER`). Two knobs:

- `max_concurrent` — permits available. Default CPU-count clamped to
  `[4, 32]`.
- `queue_timeout` — how long a caller may wait for a permit. Default
  5 s.

Behaviour:

1. Caller enters the queue and asks for a permit.
2. Granted ≤ `queue_timeout`: proceed to engine, release on
   completion. Happy path.
3. Not granted within `queue_timeout`: return `503 Service
   Unavailable` + `Retry-After` header on HTTP, or
   `ERR_SERVER_OVERLOADED` on RPC.
4. tokio's semaphore is FIFO — no starvation.

Env-var configuration (same shape as the sharding config so ops
learns one pattern):

- `NEXUS_ADMISSION_MAX_CONCURRENT` (u32, default = CPU-clamped)
- `NEXUS_ADMISSION_QUEUE_TIMEOUT_MS` (u64, default 5000)
- `NEXUS_ADMISSION_ENABLED` (bool, default true)

Prometheus metrics added through the existing surface:

- `nexus_admission_permits_granted_total`
- `nexus_admission_permits_rejected_total`
- `nexus_admission_wait_seconds` (histogram)
- `nexus_admission_in_flight` (gauge)

## Impact

- Affected specs: NEW capability `admission-control`.
- Affected code:
  - `crates/nexus-server/src/middleware/admission.rs` (NEW)
  - `crates/nexus-server/src/lib.rs` — `NexusServer` gains an
    `Arc<AdmissionQueue>` field
  - `crates/nexus-server/src/main.rs` — instantiate + install
    middleware
  - `crates/nexus-server/src/api/cypher/execute.rs` — acquire permit
  - `crates/nexus-server/src/api/ingest.rs` — acquire permit
  - `crates/nexus-server/src/protocol/rpc/dispatch/mod.rs` — acquire
    permit
- Breaking change: NO — env-var default keeps admission enabled with
  a generous CPU-derived cap that fits every healthy workload.
  Loaded-test rigs can disable with `NEXUS_ADMISSION_ENABLED=false`.
- User benefit: a runaway client no longer wedges the server. The
  admission gate surfaces pressure as `503` / `Retry-After` so the
  caller backs off instead of consuming unbounded engine time.
