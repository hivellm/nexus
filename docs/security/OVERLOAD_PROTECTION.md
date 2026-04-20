# Overload Protection

> **Since**: 1.0.x (post-V2 sharding), task `phase4_server-admission-control`
>
> **Default**: enabled. CPU-derived concurrency cap (`[4, 32]`), 5 s
> queue timeout, `503 Service Unavailable + Retry-After` on overload.

Nexus server layers three independent back-pressure mechanisms. Each
protects a different threat surface; understanding the stack helps
tune the right knob when traffic shape changes.

## 1. Rate limiter — per API key

**What**: [`middleware::RateLimiter`](../../crates/nexus-server/src/middleware/rate_limit.rs)
gates requests by API-key quota (default 1 000 / minute, 10 000 / hour).

**Catches**: sustained floods from a single authenticated caller.

**Misses**: short bursts that stay under the per-minute budget; every
other caller's burst.

## 2. RPC per-connection semaphore

**What**: [`protocol/rpc/server.rs`](../../crates/nexus-server/src/protocol/rpc/server.rs)
caps concurrent dispatches per TCP connection.

**Catches**: a single client pipelining N simultaneous RPCs down one
socket.

**Misses**: N clients each opening one connection — the aggregate
concurrency is unbounded.

## 3. Global admission queue ← this doc

**What**: [`middleware::admission`](../../crates/nexus-server/src/middleware/admission.rs).
Shared semaphore in front of every query-bearing route. Callers that
would push concurrency over `NEXUS_ADMISSION_MAX_CONCURRENT` wait
in a FIFO queue; those that exceed `NEXUS_ADMISSION_QUEUE_TIMEOUT_MS`
get rejected.

**Catches**: the concrete failure mode that motivated this layer —
one client firing tens of thousands of `execute_cypher` calls
through a single HTTP keep-alive, saturating the engine and wedging
the process.

**Misses**: persistent high traffic from legitimate use. The queue
does not *reduce* load, it **bounds concurrency**. Capacity planning
+ horizontal scaling are still the answer for sustained load.

## Configuration

Env vars, read once at server boot:

| Variable | Default | Meaning |
|---|---|---|
| `NEXUS_ADMISSION_ENABLED` | `true` | Master kill-switch. `false` makes every `acquire` a no-op. |
| `NEXUS_ADMISSION_MAX_CONCURRENT` | CPU-count, clamped to `[4, 32]` | Simultaneous permits the queue hands out. |
| `NEXUS_ADMISSION_QUEUE_TIMEOUT_MS` | `5000` | How long a caller may wait for a permit before the server returns 503. |

### When to tune

- **Tight wait timeout** (`500`–`2000`): front-end users — prefer
  fail-fast + client retry over a hung tab. Cost: a burst of legit
  traffic during a GC pause returns spurious 503s.
- **Loose wait timeout** (`10 000`–`30 000`): batch / ETL workloads —
  prefer queueing to retries. Cost: back-pressure arrives later,
  engine may already be saturated.
- **Disable** (`NEXUS_ADMISSION_ENABLED=false`): benchmark rigs that
  already front-load their rate. **Never** run a public binding
  with admission disabled.

## Which endpoints are gated?

The middleware consults `is_heavy_path(uri)` before requesting a
permit. Light-weight endpoints (`/health`, `/prometheus`, `/auth`,
`/schema/*`, `/stats`, `/cluster/status`, …) always pass through
regardless of queue state — diagnostics must stay reachable when the
engine is saturated.

Gated prefixes (see `HEAVY_PATH_PREFIXES` in [admission.rs](../../crates/nexus-server/src/middleware/admission.rs)):

- `/cypher`
- `/ingest`
- `/knn_traverse`
- `/graphql`
- `/umicp`

RPC + RESP3 surfaces will gain permit acquisition in a follow-up
iteration; until then they rely on the per-connection semaphore.

## Response shape on overload

### HTTP

```
HTTP/1.1 503 Service Unavailable
Retry-After: 10
Content-Type: application/json

{
  "error": "server overloaded",
  "retry_after_ms": 10000,
  "reason": "server overloaded: waited 5000 ms, queue_timeout 5000 ms"
}
```

The `Retry-After` header is an integer number of seconds (sized as
`2 × queue_timeout` so the client's backoff gives the backlog a real
chance to drain).

## Observability

The queue publishes live counters via
[`AdmissionQueue::metrics()`](../../crates/nexus-server/src/middleware/admission.rs).
Prometheus integration lands in a subsequent patch; the canonical
metric names reserved today are:

| Metric | Type | Meaning |
|---|---|---|
| `nexus_admission_permits_granted_total` | counter | Requests that got through the queue. |
| `nexus_admission_permits_rejected_total` | counter | Requests rejected because of `queue_timeout`. |
| `nexus_admission_in_flight` | gauge | Currently-held permits (≤ `max_concurrent`). |
| `nexus_admission_wait_seconds` | histogram | Time spent waiting for a permit. |

## Failure-mode table

| Symptom | Likely cause | Action |
|---|---|---|
| Burst of 503s in a scheduled ETL run | Queue timeout < ETL step duration | Raise `NEXUS_ADMISSION_QUEUE_TIMEOUT_MS` or stagger the ETL batch |
| Steady 503s under normal load | `max_concurrent` too low for the host | Raise the cap; consider vertical or horizontal scaling |
| /health returns 503 | Would indicate a bug — `/health` is not gated | File an issue; it's not supposed to happen |
| Single bad client wedges others | Admission queue is the right layer, but one client dominates it | Layer per-tenant fairness (tracked in the cluster-mode roadmap) |
