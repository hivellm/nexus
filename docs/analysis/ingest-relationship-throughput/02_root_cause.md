# 02 — Root cause: a durable LMDB fsync per relationship

The fixed ~0.33 ms/edge is a **disk sync per relationship**, done just to bump
an in-catalog counter.

## The chain

`/ingest` → `create_relationship_direct` (`api/ingest.rs`) →
`Engine::create_relationship` → `create_relationship_with_transaction`
(`engine/crud/relationships.rs`). With no session transaction (the `/ingest`
path), the LAST thing it does for every row is:

```rust
self.catalog.increment_rel_count(type_id)?;   // relationships.rs:137
```

and `increment_rel_count` (`catalog/stats.rs:82`) is:

```rust
pub fn increment_rel_count(&self, type_id: TypeId) -> Result<()> {
    let mut stats = self.get_statistics()?;              // LMDB READ txn + deserialize the WHOLE stats blob
    *stats.rel_counts.entry(type_id).or_insert(0) += 1;
    self.update_statistics(&stats)                       // LMDB WRITE txn + serialize the WHOLE blob + commit
}

pub fn update_statistics(&self, stats: &CatalogStats) -> Result<()> {
    let mut wtxn = self.env.write_txn()?;
    self.stats_db.put(&mut wtxn, "main", stats)?;
    wtxn.commit()?;                                       // <-- durable commit
    Ok(())
}
```

The catalog LMDB env is opened with **default flags** (`catalog/store.rs:372`
— no `NoSync`/`NoMetaSync`), so LMDB fsyncs data + metadata on **every commit**.
That is a `FlushFileBuffers`-class disk sync **per relationship**, whose
latency (~0.1–1 ms on this box) is the entire 0.33 ms budget. On top of the
sync, each call reads, deserialises, mutates, and re-serialises the ENTIRE
`CatalogStats` structure (all node- and rel-count maps).

## Why nodes are ~3× faster

`create_node_inner` (`engine/crud/nodes.rs`) does **not** call
`increment_node_count` per node — the node-count catalog update is batched
elsewhere (`batch_increment_node_counts`, "Phase 1 Optimization: reduces
I/O"). So the node path takes NO per-row LMDB fsync. Relationships have no such
batching, so they pay one durable commit each.

This is a KNOWN, documented gap. `engine/mod.rs:481-491`:

> **Caveat on relationship counts.** The current CREATE operator batches
> node-count catalog updates but does NOT increment `catalog.rel_counts`…
> `batch_increment_node_counts` is called, the rel-type equivalent is not…
> Fixing create.rs to also batch `increment_rel_count` is a separate follow-up.

Note the asymmetry across the two write paths:
- **Executor `CREATE`** (`executor/operators/create.rs`): batches node counts,
  and does NOT increment rel counts at all → fast, but the catalog rel total is
  wrong (a lower bound, often 0).
- **`Engine::create_relationship`** (`/ingest`, engine CRUD): increments rel
  counts per row → correct total, but a durable fsync per edge.

A correct fix must make relationship counting BOTH cheap (batched/deferred,
one commit per batch) AND correct (both paths agree).

## Secondary cost — a redundant parallel write (Phase 8)

`create_relationship_with_transaction` also does, per row
(`relationships.rs:83-114`):

```rust
if let Some(rel_storage) = self.executor.relationship_storage() {
    rel_storage.write().create_relationship(from, to, type_id, props_map)…   // a SECOND full insert
    // + relationship_property_index if props
}
```

`relationship_storage` is `Some` by default (`executor/shared.rs:129,253`),
so every relationship is written a second time into a parallel
`RelationshipStorageManager`. Its read path in `find_relationships` is
commented out and it was just superseded by the store adjacency index
(the traversal repoint in commit 88f78245) — so this is now dead-on-read work
paid on every write. Cost is CPU + a lock, not a sync, so it is secondary to
the fsync but is pure waste.

## What is NOT the cost (measured or read)

- `transaction_manager.begin_write()/commit()` — in-memory epoch/tx-id
  bookkeeping only (`transaction/mod.rs:188,201`), no I/O.
- `get_or_create_type` — cache hit after the first edge of a type
  (`catalog/mappings.rs:269`), lock-free.
- `write_wal_async` — the WAL append is async and explicitly NOT flushed per
  op (`relationships.rs:129`).
- The outgoing-chain insert — O(1) prepend (see [01](01_measurement.md)).
