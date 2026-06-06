# Proposal: phase6_fix-match-scopes-parallel-load-flake

Surfaced while implementing `phase6_fix-prop-ptr-corruption-startup` (GH #4).

## Why
`engine::tests::match_scopes_by_label_and_property_together`
(crates/nexus-core/src/engine/tests.rs:902) fails ONLY under the full
multi-binary parallel run `cargo +nightly test -p nexus-core`. It passes:
- in isolation,
- in the `--lib`-only full run (2354/2354),
- in the full serial run `cargo +nightly test -p nexus-core -- --test-threads=1`
  (2354 lib + all integration tests).

The test seeds 7 nodes + 8 `:R` edges, asserts the sanity count
`MATCH ()-[r]->() RETURN count(r)` == 8 (passes even when the test fails),
then asserts `MATCH (:X {id:0})-[:R]->(b) RETURN count(b)` == 3. Under
cross-process load the composite source filter collapses and the count
returns 8 (== the sanity query's all-`:R` count) — i.e. both the `:X` label
and `{id:0}` property scopes are dropped.

The issue-#4 `rebuild_index` fix adds a correct full on-disk property-store
scan on every existing-store open (required for properties to survive reboot
at 137k-node scale); this raises cross-binary I/O/memory load and makes the
pre-existing flake reproduce reliably (3/3), but it does NOT logically cause
it — the test runs on a fresh tempdir where the fix is a no-op.

## Investigation findings (already established)
- NOT test isolation: `TestContext` gives each test a unique `tempfile::TempDir`.
- NOT the researcher's first hypothesis (stale label index): the executor's
  CREATE DOES update the shared label index synchronously
  (crates/nexus-core/src/executor/operators/create.rs:564
  `self.label_index_mut().add_node(...)`), and the test's MATCH runs only after
  `execute_cypher(CREATE)` fully returns (refresh_executor at
  engine/mod.rs:2615 is synchronous, before the return at 2618) — same thread,
  no in-process window.
- NOT a result/plan cache: the query-result cache block at
  executor/mod.rs:178-205 is commented out (`*/` at 206).
- NOT background threads: no `thread::spawn`/`tokio::spawn` in the
  engine/executor query path mutates shared index/executor state
  (`async_wal_writer` touches WAL only).
- NOT process-global mutable state: the executor `OnceLock`s are per-instance;
  the `serde_metrics` statics are atomic counters only.

The only remaining cross-process channel is OS-level: the record / property /
index stores are `memmap2`-backed and `read_node`'s `Acquire` fence orders CPU
only, not mmap page residency. Under heavy concurrent memory pressure a
reclaimed/re-faulted page can read stale/zero, dropping label_bits / property
reads and collapsing the filter. This is the leading hypothesis to confirm.

## What Changes
- Confirm the root cause with instrumented reproduction under load (capture the
  exact wrong plan/operator output and which read returns stale data).
- If mmap page-residency under memory pressure is confirmed: add the appropriate
  durability/visibility guarantee on the hot read path (e.g. explicit page
  fault-in / `madvise(WILLNEED)` / verified-read retry, or a non-mmap read path
  for integrity-critical fields) without regressing throughput.
- If instead a planner/operator edge is found, fix that.
- Make the test deterministic without masking the underlying defect.

## Impact
- Affected specs: storage-format / executor
- Affected code: `crates/nexus-core/src/storage/` (mmap read paths),
  `crates/nexus-core/src/executor/` (planner/operators), test in
  `crates/nexus-core/src/engine/tests.rs`
- Breaking change: NO
- User benefit: correct query results under high concurrent load; a green
  default (parallel) `cargo test -p nexus-core`
