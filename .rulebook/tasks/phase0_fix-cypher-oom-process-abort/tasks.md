# Tasks: phase0_fix-cypher-oom-process-abort

`UNWIND $rows AS r MATCH (a:P {id: r.s}), (b:P {id: r.d}) CREATE (a)-[:KNOWS]->(b)`
kills the server: `memory allocation of 4000000000000 bytes failed`, process aborts,
connection reset, no error response. Reproduced on a release build with 5 000 param
rows over a 10 000-node graph.

Order matters: isolate before diagnosing, diagnose before fixing. The cause is a
HYPOTHESIS (capacity from an unvalidated cardinality estimate), not a finding — §1
must confirm or refute it before any code is touched. Do not start §3 before §2.

## 1. Isolate the minimal repro
- [ ] 1.1 Bisect the query shape against a fresh release server, one variable at a time, recording the allocation size reported for each: (a) `MATCH (a:P {id: 1}), (b:P {id: 2}) RETURN a.id, b.id`, (b) same + `CREATE (a)-[:R]->(b)`, (c) `UNWIND` + comma MATCH + `RETURN`, (d) `UNWIND` + comma MATCH + `CREATE`. Identify the smallest shape that aborts
- [ ] 1.2 Determine what the allocation size scales with — vary node count and `$rows` length independently and see whether the reported byte count tracks one, the other, or their product. `4_000_000_000_000` is suspiciously round; work out what expression produces exactly that from the inputs
- [ ] 1.3 Capture a backtrace with `RUST_BACKTRACE=1` (the abort message itself recommends it) and name the exact allocation site, file and line
- [ ] 1.4 Check whether the comma-separated `MATCH (a), (b)` cartesian form is required at all, or whether a single-pattern equivalent also aborts — this decides whether the bug is in cartesian planning or somewhere more general

## 2. Confirm the mechanism
- [ ] 2.1 From the §1.3 site, establish whether the size comes from a planner cardinality estimate or from real data, and write the finding down explicitly — if the hypothesis is wrong, correct the proposal before proceeding
- [ ] 2.2 Determine whether the estimate itself is also wrong (an over-estimate that would be harmless if merely bounded, versus an estimate that is correct and genuinely needs that much memory). These need different fixes and must not be conflated

## 3. Fix
- [ ] 3.1 Fix the identified cause. If it is an unbounded `with_capacity` from an estimate, size the allocation from actual data or grow incrementally; if the estimate is wrong, fix the estimate too — do not paper over a broken estimate with a cap alone
- [ ] 3.2 Add a hard ceiling so no estimate can ever become an unbounded allocation: exceeding a configured budget must produce a typed, catchable error. A wrong estimate must degrade to a failed query, never to a dead process
- [ ] 3.3 Audit sibling call sites for the same pattern — `with_capacity` / `reserve` fed by estimated rather than actual counts — and apply the same bound. Record which sites were checked so the audit is verifiable

## 4. Tail (docs + tests — check or waive with tailWaiver)
- [ ] 4.1 Update or create documentation covering the implementation (the memory-budget knob and its default in the server config docs; CHANGELOG entry noting the query shape that used to abort the process)
- [ ] 4.2 Write tests covering the new behavior: a regression test running the §1.1 minimal repro shape and asserting it either succeeds or returns an error — the test process must survive, which is the entire point. Add a unit test for the ceiling itself with a deliberately absurd estimate
- [ ] 4.3 Run tests and confirm they pass (`cargo +nightly fmt --all`, `cargo clippy --workspace --all-targets --all-features -- -D warnings`, `cargo +nightly test --workspace` green)

## Related
- Discovered by `phase7_ldbc-snb-benchmark` item 1.3. The loader wanted exactly this
  query shape to create edges by LDBC id and must avoid it until this ships.
- `phase0_fix-ingest-bulk-path` covers the other half of that finding: `/ingest` is
  too slow to be the alternative.
