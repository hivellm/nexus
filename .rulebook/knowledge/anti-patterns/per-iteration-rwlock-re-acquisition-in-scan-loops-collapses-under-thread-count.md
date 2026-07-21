# Per-iteration RwLock re-acquisition in scan loops collapses under thread count

**Category**: performance
**Tags**: performance, concurrency, locking, phase8, benchmark

## Description

phase8_neo4j-concurrency-gaps: aggregation.count_all flatlined at 2.9k qps with p99 124ms at 64 workers (Neo4j: 13k) while being at parity at 1-4 workers. Root cause: count_live_nodes_* re-acquired the nodes_mmap parking_lot::RwLock once PER NODE (O(n) acquisitions per query) — negligible single-threaded, catastrophic under 64 concurrent readers hammering the same lock word. Fix pattern ("acquire-once bulk-snapshot"): RecordStore::read_all_node_headers() takes the lock ONCE, bytemuck-casts the mmap to &[NodeRecord] bounded by node_count(), and callers filter in memory. Result: 62,653 qps @64w (21.8x), p99 2.25ms. Note parking_lot::RwLock is non-reentrant (see the existing recursive-acquire anti-pattern), so the fix must thread the held guard or snapshot — a caller cannot hold an outer guard across a callee that re-acquires. Same signature to look for elsewhere: fine at low concurrency + flatline-with-exploding-p99 from ~16 threads = per-item lock acquisition inside a loop. The traversal scan loops (read_node_as_value via scan.rs/expand.rs/path.rs, ~6 call sites) still carry this pattern — item 2 of the same task.

## When to Use

Recognize it when a read-heavy operation scales linearly to ~4-16 threads then flatlines with p99 exploding, and the loop body calls any method that internally acquires a shared RwLock/Mutex per element.

## When NOT to Use

Per-iteration locking is fine when the loop is short (constant, small), the lock is uncontended, or the data must be re-validated per element for correctness (e.g. MVCC visibility checks that cannot be snapshotted).
