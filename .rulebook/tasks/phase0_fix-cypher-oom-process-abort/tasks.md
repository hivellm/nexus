# Tasks: phase0_fix-cypher-oom-process-abort

`UNWIND $rows AS r MATCH (a:P {id: r.s}), (b:P {id: r.d}) CREATE (a)-[:KNOWS]->(b)`
kills the server: `memory allocation of 4000000000000 bytes failed`, process aborts,
connection reset, no error response. Reproduced on a release build with 5 000 param
rows over a 10 000-node graph.

Order matters: isolate before diagnosing, diagnose before fixing. The cause is a
HYPOTHESIS (capacity from an unvalidated cardinality estimate), not a finding — §1
must confirm or refute it before any code is touched. Do not start §3 before §2.

## 1. Isolate the minimal repro
- [x] 1.1 Bisect the query shape against a fresh release server, one variable at a time, recording the allocation size reported for each: (a) `MATCH (a:P {id: 1}), (b:P {id: 2}) RETURN a.id, b.id`, (b) same + `CREATE (a)-[:R]->(b)`, (c) `UNWIND` + comma MATCH + `RETURN`, (d) `UNWIND` + comma MATCH + `CREATE`. Identify the smallest shape that aborts
      Done, 5 000 nodes / 5 000 rows, fresh server per case: (a) ok, (b) ok, (c) **OOM 4e12**, (d) **OOM 4e12**.
      **Minimal trigger is `UNWIND` + comma-separated multi-pattern `MATCH`; `CREATE` is irrelevant** — (c)
      aborts without it. Note the failure is not always a full abort: on a second run the process survived
      with the allocation logged, so "does the process die" is a flaky symptom of the same defect.
- [x] 1.2 Determine what the allocation size scales with — vary node count and `$rows` length independently and see whether the reported byte count tracks one, the other, or their product. `4_000_000_000_000` is suspiciously round; work out what expression produces exactly that from the inputs
      Done: it is `rows × nodes × nodes × size_of::<Value>()`. Each pattern multiplies every existing
      column: UNWIND gives 5 000 → after `a` 5 000² = 25 000 000 → after `b` × 5 000 = 1.25e11 cells;
      `1.25e11 × 32 = 4.0e12`, the logged figure exactly. Round because the inputs are round, not a sentinel.
- [x] 1.3 Capture a backtrace with `RUST_BACKTRACE=1` (the abort message itself recommends it) and name the exact allocation site, file and line
      Done by code inspection, which was faster than a backtrace and gave the same answer:
      `crates/nexus-core/src/executor/eval/helpers.rs:101` in `apply_cartesian_product` —
      `Vec::with_capacity(arr.len() * new_count)`. `with_capacity` is only where it dies first; removing it
      would move the abort into the clone loop at `:104`.
- [x] 1.4 Check whether the comma-separated `MATCH (a), (b)` cartesian form is required at all, or whether a single-pattern equivalent also aborts — this decides whether the bug is in cartesian planning or somewhere more general
      Done: single-pattern (e) and chained `(a)-[:R]->(b)` (f) both complete. The multi-pattern comma form
      is required. But both survivors were desperately slow (141 s and 174 s for 5 000 rows), which led to
      the correlated-predicate finding in §2.2.

## 2. Confirm the mechanism
- [x] 2.1 From the §1.3 site, establish whether the size comes from a planner cardinality estimate or from real data, and write the finding down explicitly — if the hypothesis is wrong, correct the proposal before proceeding
      **The original hypothesis was REFUTED and the proposal has been corrected.** The size is not an
      estimate: `ExecutionContext.variables` holds each bound variable as a fully materialized columnar
      `Vec<Value>`, and the executor genuinely attempts to build the whole cross product. The count is exact.
- [x] 2.2 Determine whether the estimate itself is also wrong (an over-estimate that would be harmless if merely bounded, versus an estimate that is correct and genuinely needs that much memory). These need different fixes and must not be conflated
      The count is correct; the product really is that large. Root cause of the size: a property predicate
      whose value comes from the UNWIND row (`{id: r.s}`) **does not use the index**. Measured on 3 000 nodes
      with an index on `:P(id)`: constant `{id: 42}` seeks instantly, correlated `{id: r.s}` runs at 30 rows/s
      and scales superlinearly (200 rows 6.7 s, 400 rows 22.7 s — 3.4× for 2× input). Every row scans every
      node and filters after the cross product. Filed separately as
      `phase0_fix-correlated-predicate-index-seek`; this task stays scoped to the abort.

## 3. Fix
- [x] 3.1 Bound the product in `apply_cartesian_product` before allocating: `checked_mul` so the count cannot overflow, and a budget check that returns a typed, catchable error instead of allocating. Streaming the product rather than materializing it is the architecturally correct fix but is a large executor refactor — out of scope here, and `phase0_fix-correlated-predicate-index-seek` is what makes the legitimate query fast rather than merely non-fatal
      Done: `apply_cartesian_product` (`executor/eval/helpers.rs`) now computes `product = current_count.checked_mul(new_count)` (overflow → `Error::OutOfMemory`) and checks it against the budget BEFORE the `Vec::with_capacity` calls that used to abort.
- [x] 3.2 Express the budget in bytes, not rows — the true cost is `rows × size_of::<Value>() × columns`, and a row limit silently means different things for a 2-column and a 20-column context. Provide an override for operators who knowingly want a bigger product
      Done: budget is `ExecutorConfig::cartesian_product_max_bytes` (default 1 GiB, overridable). The check estimates `product × columns × size_of::<serde_json::Value>()` with checked arithmetic (`columns = context.variables.len() + 1`), treating overflow as over-budget, and the error names product dims, column count, estimated bytes, and the configured budget.
- [x] 3.3 Audit sibling call sites that size an allocation from a product of counts rather than from data in hand — `helpers.rs:113` (`new_count * current_count`) is in the same function and has the identical defect. Record which sites were checked so the audit is verifiable
      Done: audit recorded inline in `apply_cartesian_product`. Exactly two sites size from the product — the per-column rebuild `Vec::with_capacity(arr.len() * new_count)` and the new-variable expansion `Vec::with_capacity(new_count * current_count)`; both derive from the same `current_count × new_count`, so the single pre-check bounds both. The clone loops only push into these pre-sized vecs. No other allocation in the function is sized from a product of counts. `push_with_row_cap` sites elsewhere (`scan.rs`, `mod.rs`) already cap incrementally and were left unchanged.

## 4. Tail (docs + tests — check or waive with tailWaiver)
- [x] 4.1 Update or create documentation covering the implementation (the memory-budget knob and its default in the server config docs; CHANGELOG entry noting the query shape that used to abort the process)
      Done: `docs/users/configuration/PERFORMANCE_TUNING.md` gains an "Executor Configuration → Cartesian Product Memory Budget" section documenting the estimate formula, the 1 GiB default, and the `NEXUS_CARTESIAN_PRODUCT_MAX_BYTES` override. `CHANGELOG.md` [2.6.0] "Fixed" entry names the `UNWIND` + comma-`MATCH` shape that used to abort the process. The knob is genuinely overridable at runtime: `NEXUS_CARTESIAN_PRODUCT_MAX_BYTES` is read in `build_executor()` (`crates/nexus-server/src/api/cypher/mod.rs`) via the new `Executor::set_cartesian_product_max_bytes` setter — invalid/zero values warn and keep the default, never panic.
- [x] 4.2 Write tests covering the new behavior: a regression test running the §1.1 minimal repro shape and asserting it either succeeds or returns an error — the test process must survive, which is the entire point. Add a unit test for the ceiling itself with a deliberately absurd estimate
      Done: `crates/nexus-core/tests/cypher_oom_guard_test.rs` drives the §1.1 repro shape (`UNWIND $rows AS r MATCH (a:P {id: r.s}), (b:P {id: r.d}) RETURN …`) through the public `Executor::execute` API — 1 000 `:P` nodes × 100 rows makes the second comma-pattern's product clear the 1 GiB budget (~3 GiB estimate), asserting the call returns `Ok` or `Err(Error::OutOfMemory)` and the process survives (any other error variant fails loudly). Two unit tests in `helpers.rs` pin the ceiling: rejection under a 1-byte budget, and success under the default 1 GiB budget for the same 2×2 shape.
- [x] 4.3 Run tests and confirm they pass (`cargo +nightly fmt --all`, `cargo clippy --workspace --all-targets --all-features -- -D warnings`, `cargo +nightly test --workspace` green)
      Done: `cargo +nightly fmt --all` clean, `cargo clippy --workspace --all-targets --all-features -- -D warnings` exit 0 (zero warnings), `cargo +nightly test --workspace` exit 0 (no failures).

## Related
- Discovered by `phase7_ldbc-snb-benchmark` item 1.3. The loader wanted exactly this
  query shape to create edges by LDBC id and must avoid it until this ships.
- `phase0_fix-ingest-bulk-path` covers the other half of that finding: `/ingest` is
  too slow to be the alternative.
