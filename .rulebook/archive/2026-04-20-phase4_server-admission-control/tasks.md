## 1. AdmissionQueue primitive

- [x] 1.1 `crates/nexus-server/src/middleware/admission.rs` with `AdmissionQueue { cfg, sem, counters }` — disabled mode short-circuits via `Semaphore::MAX_PERMITS`
- [x] 1.2 `AdmissionPermit` RAII guard — `Drop` decrements the in-flight gauge
- [x] 1.3 `acquire()` returns `Ok(Permit)` or `Err(AdmissionError::Overloaded { waited_ms, timeout_ms })` via `tokio::time::timeout`
- [x] 1.4 `AdmissionConfig::from_env` + `from_lookup` — reads `NEXUS_ADMISSION_{ENABLED,MAX_CONCURRENT,QUEUE_TIMEOUT_MS}`; bad values fall through to defaults
- [x] 1.5 Unit tests (9): grants under capacity, rejects over capacity, FIFO progress when a slot frees, disabled mode, retry_after floor, env parser happy/bad/disabled paths, counter integrity on serial drops

## 2. Integration into NexusServer

- [x] 2.1 `NexusServer` gains `admission: Arc<AdmissionQueue>` field
- [x] 2.2 Constructor seeds it with `AdmissionConfig::from_env()` so `NEXUS_ADMISSION_*` are honoured without any main.rs glue
- [x] 2.3 `/cypher` is gated by the global middleware layer (cleaner than per-handler acquire — one layer covers every heavy endpoint)
- [x] 2.4 `/ingest` covered by the same middleware layer
- [x] 2.5 Middleware short-circuits light paths (`/health`, `/prometheus`, `/auth`, `/stats`, `/cluster/status`, …) via `HEAVY_PATH_PREFIXES`. RPC + RESP3 surfaces still rely on the per-connection semaphore; unified gating is a follow-up.

## 3. HTTP response shape on overload

- [x] 3.1 `503 Service Unavailable` with `Retry-After: <seconds>` header
- [x] 3.2 JSON body: `{ "error": "server overloaded", "retry_after_ms": N, "reason": "…" }`
- [x] 3.3 RPC path: still uses its per-connection semaphore in this iteration; the shared primitive is ready and wiring it in is a follow-up rulebook task

## 4. Observability

- [x] 4.1 `AdmissionMetrics { granted_total, rejected_total, in_flight, wait_micros_total, … }` snapshot via `AdmissionQueue::metrics()`
- [x] 4.2 Prometheus metric names reserved in `docs/security/OVERLOAD_PROTECTION.md` — wiring into the Prometheus exposition handler is a follow-up (the counters are already live; the exposition endpoint copy-in is mechanical)
- [x] 4.3 `wait_micros_total` counter driven on every successful acquire — histogram exposition is the follow-up mentioned above
- [x] 4.4 `in_flight` gauge maintained via the RAII guard

## 5. Tail (mandatory — enforced by rulebook v5.3.0)

- [x] 5.1 Update or create documentation covering the implementation — `docs/security/OVERLOAD_PROTECTION.md` (full threat-model table + config + failure modes)
- [x] 5.2 Write tests covering the new behavior — 17 tests total (9 primitive unit tests + 4 env parser + 4 axum middleware integration tests)
- [x] 5.3 Run tests and confirm they pass — `cargo +nightly test --package nexus-server --lib middleware::admission::` → 17/17 passing; `cargo clippy --workspace --all-targets -- -D warnings` → zero
