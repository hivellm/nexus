# Proposal: phase0_fix-fts-async-writer-ordering

**Priority: HIGH — SET/REMOVE on an indexed node makes it permanently invisible to fulltext
search whenever the async fulltext writer is enabled.** Found during a write-path/index
corruption audit; not previously reported.

## Why

`fts_refresh_node` (`engine/crud/index_maintenance.rs:43-70`) implements a property update as
del-then-add: it enqueues `Del{node}` followed by `Add{node, newContent}` against the fulltext
writer's command queue, via `registry.remove_entity`/`add_node_document`
(`fulltext_registry.rs:551,676`).

`apply_batch` (`index/fulltext_writer.rs:280-293`) does not preserve arrival order within a batch —
it applies **all adds first, then all dels**:

```
apply_batch: adds.for_each(apply_add); dels.for_each(apply_del);
```

So within one batch, a node's `Add{newContent}` is applied, then its own `Del{node}` runs
afterward and removes it. The `members` bookkeeping set is never told about this reordering and
still ends up marking the node "present," so `indexes_containing` and the actual Tantivy index
disagree: the registry believes the node is indexed, but Tantivy no longer holds it. The node
becomes invisible to fulltext search despite having valid, current indexed content and despite
every bookkeeping structure claiming it is indexed.

### Reachability

This path only executes when `enable_async_writers` is active (opt-in, `index/mod.rs:72-77`;
currently only a test enables it) — but it is a shipped, documented "high-throughput" path, and
any deployment that turns it on corrupts fulltext-search visibility on every SET/REMOVE of an
indexed node's fulltext-indexed properties. The synchronous writer path does not have this defect
— it applies commands in arrival order, so del-then-add is respected there.

## What Changes

- Change `apply_batch` to preserve per-node del→add ordering: either apply commands strictly in
  arrival order within the batch, or coalesce a `Del{id}` immediately followed by an
  `Add{id, content}` for the same id into a single replace operation before applying the batch.
- Preserve the current throughput characteristics for unrelated ids in the same batch — only
  same-id ordering needs to change; batching across different ids must remain unaffected.

## Impact

- Affected specs: none — no dedicated fulltext-index spec exists in `docs/specs/`; the
  async-writer batching contract is presently undocumented. Consider adding a short note to
  `docs/specs/cypher-subset.md` (SET/REMOVE index-maintenance side effects) once fixed.
- Affected code: `index/fulltext_writer.rs` (`apply_batch:280-293`),
  `index/fulltext_registry.rs` (`remove_entity:551`, `add_node_document:676`),
  `engine/crud/index_maintenance.rs` (`fts_refresh_node:43-70`)
- Breaking change: NO — corrects the async-writer path to match the synchronous path's already-
  correct del-then-add semantics; no currently-correct behavior changes
- User benefit: SET/REMOVE on an indexed node's fulltext-indexed properties keeps the node visible
  to fulltext search when `enable_async_writers` is on, matching synchronous-writer behavior and
  the registry's own bookkeeping
- Related: `phase0_fix-update-node-index-divergence` — a different index-maintenance gap
  (`update_node` skips the refresh suite entirely) reached through the same
  `engine/crud/index_maintenance.rs` refresh helpers
