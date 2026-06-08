## 1. Investigation (diagnostic-led — no fix without a confirmed root cause)
- [ ] 1.1 Add/curate telemetry for no-query-running activity: background-task / index-maintenance / WAL / flush / GC, lock-wait/contention counters, and a thread/stack snapshot hook or `GET /admin/...` introspection endpoint
- [ ] 1.2 Reproduce under sustained write load (tens of thousands of MERGE+SET + edge MERGEs, indexes engaging) until 100% CPU / unresponsive; capture where the CPU spins
- [ ] 1.3 Identify the busy-loop root cause (index maintenance per write / WAL / flush / async-writer loop / transaction-epoch retry / relationship-index rebuild / spin without backoff)

## 2. Implementation
- [ ] 2.1 Fix the identified hot loop so sustained writes drain instead of pinning the core and wedging the server
- [ ] 2.2 Ensure the diagnostic telemetry from 1.1 ships (operators can see what a no-query server is doing)

## 3. Tail (mandatory — enforced by rulebook v5.3.0)
- [ ] 3.1 Update or create documentation covering the fix + the new telemetry knobs (CHANGELOG / GH #12)
- [ ] 3.2 Write tests: a soak/regression guard for the identified loop (bounded CPU/time over a sustained write batch) and telemetry-surface tests
- [ ] 3.3 Run tests and confirm they pass
