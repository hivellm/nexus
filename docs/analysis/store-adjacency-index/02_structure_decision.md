# 02 — Structure decision: in-memory, both directions, owned by the store

Task `phase0_perf-store-reverse-incoming-adjacency-index`, item 1.2.

## Decision

An **in-memory `HashMap<node_id, HashSet<rel_id>>` pair (outgoing + incoming)
owned by `RecordStore`**, shared across clones via `Arc<RwLock<…>>`, maintained
in `write_rel` (see [01](01_write_chokepoint.md)) and rebuilt from the record
file when the store is opened.

## In-memory vs. on-disk incoming chain

Option (b) from the proposal — a second head pointer on `NodeRecord` plus a
`next_dst_ptr` chain — is a record-format change with a migration, and it buys
only durability-without-rebuild. Rejected, because the rebuild is free (below).

Option (a), chosen: rebuilt on open. Cost per relationship is one `HashSet`
entry in each direction; the index holds LIVE edges only (deleted records are
removed on the `write_rel` that marks them), so it tracks the live edge count,
not the slot count.

## Why the rebuild is free

`RecordStore::new` (`record_store.rs:196-210`) **already** walks every
relationship slot to derive `next_rel_id` and to collect legacy records for the
allocated-bit migration. Populating the index happens inside that existing loop:
zero additional I/O, zero additional passes.

This also answers item 1.4 more strongly than the task's own wording ("rebuild on
engine open, alongside `rebuild_relationship_index_from_storage` in
`engine/mod.rs`"): building it where the store learns its own contents means
every store user is covered — the engine, the executor's test harness, tooling —
not just `Engine::new`. An engine-level rebuild would leave a store opened by any
other route silently unindexed.

## Why BOTH directions, when the task only asks for incoming

Item 1.5 asks to drop the full-scan fallback in `node_has_live_relationship` once
the incoming source is authoritative. That is only safe if the OUTGOING half is
authoritative too, and today it is not: tier 1 is a deliberate best-effort walk
of `first_rel_ptr` that "can only short-circuit to `true`" and bails to the scan
on any unexpected chain state — a chain that `create_relationship` itself patches
up with a bounded backwards scan when the mmap looks stale. Keeping the full scan
as the outgoing authority would leave the guard at O(total edges), i.e. no fix.

Indexing both endpoints costs one extra `HashSet` insert per edge and makes the
guard, `DETACH DELETE`, and the MERGE exact-edge lookup all O(degree) off one
structure with one maintenance point. A self-loop lands in both maps and is
de-duplicated by `connected()`.

## Invariants

1. Every live relationship record is present under `outgoing[src_id]` and
   `incoming[dst_id]`; no deleted record is present under either.
2. Maintained ONLY in `write_rel` and `clear_all`; no caller opts in.
3. Idempotent: re-writing a live record (pointer fix-ups) re-inserts into a set,
   a no-op. Re-writing a deleted record removes an already-absent id, a no-op.
4. The index is an accelerator for LIVENESS and MEMBERSHIP questions. Callers
   that need the record itself still read it — a read that also re-verifies
   liveness, so an index entry can never resurrect a deleted edge.
