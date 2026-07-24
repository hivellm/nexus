# 03 — Fix plan

Goal: relationship ingest at least on par with node ingest (~8 500 rel/s), and
ideally an order of magnitude up, WITHOUT losing correctness of
`catalog.rel_counts` and without weakening durability guarantees the engine
actually relies on.

## Primary fix — stop fsyncing per relationship

The counter bump does not need its own durable transaction per edge. Two viable
shapes, not mutually exclusive:

### A1. Batch the rel-count update at the write boundary (mirror nodes)

Add `batch_increment_rel_counts(&[(TypeId, u32)])` to `catalog/stats.rs`
(twin of the existing `batch_increment_node_counts`: one `get_statistics` +
one `update_statistics` for the whole batch). Route the `/ingest` relationship
loop and the executor CREATE path through it so a batch of N edges does ONE
LMDB commit instead of N. This directly collapses the per-row fsync and also
fixes the executor path's missing rel-count increment
(`engine/mod.rs:481` caveat) in the same stroke.

- `/ingest`: accumulate `(type_id, +1)` per created edge in the batch, flush
  once after the relationship loop (the handler already frames the batch).
- Executor `CREATE`: it already batches node counts; add the rel-count column.

### A2. Make the counter not a durable-per-op LMDB write at all

`rel_counts` is a performance HINT (used for planner cardinality and the
`get_node_count`/rel-total helpers), not a correctness-critical record — it is
already reconstructed on load. It does not warrant a metadata fsync per edge.
Options: keep the live count in memory (atomic per type) and persist it lazily
(on checkpoint / flush / shutdown), or move it behind the same async-WAL
durability the record stores use rather than a synchronous catalog commit.

**Recommendation:** do A1 first (smallest, mirrors an existing, trusted
pattern, fixes both the perf and the executor-path correctness caveat). Weigh
A2 as a follow-up if per-batch commit is still a hotspot at SF1.

## Secondary fix — stop the redundant Phase-8 write

Gate or remove the per-row `RelationshipStorageManager` /
`RelationshipPropertyIndex` write (`relationships.rs:83-114`). Its read path is
disabled and the store adjacency index now serves traversal, so it is dead work
on every write. At minimum put it behind an off-by-default flag; ideally retire
it once nothing reads it. Measure its isolated contribution first (instrument or
toggle) so the removal is evidence-backed, not assumed.

## Verification gates

1. Micro-benchmark (the same harness as [01](01_measurement.md)): relationship
   ingest rate before/after, distinct- and same-source, across batch sizes.
   Target: ≥ node rate (~8 500 rel/s); stretch: ≥ 30 000 rel/s.
2. End-to-end: reload LDBC SF0.1 and compare total wall-clock to the ~480 s
   baseline; the relationship passes should shrink from the dominant share to
   a minor one.
3. Correctness: `catalog` rel total equals the loader's submitted total through
   BOTH paths (`/ingest` AND `UNWIND … CREATE`), closing the
   `engine/mod.rs:481` caveat. Durability unchanged for the record stores.
4. Full gate: `cargo +nightly test --workspace` green; no regression in the
   relationship-count-dependent planner tests.

## Scope guard

This is an ingest-throughput fix. It must not change query results, the WAL
record-store durability contract, or the on-disk record formats. The catalog
stats blob is the only durability surface touched, and only to make its update
cadence batch-scoped instead of per-edge.
