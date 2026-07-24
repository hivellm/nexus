# 01 — Where relationship writes actually funnel

Task `phase0_perf-store-reverse-incoming-adjacency-index`, item 1.1.

The task (and its proposal) assumed `storage::record_store_ops::create_relationship`
is the single chokepoint every write path funnels through, and that there is "a
single relationship-deletion path". Neither holds. What IS universal is one level
lower: `RecordStore::write_rel`.

## Creation — funnels through `create_relationship` (as assumed)

| Path | Call site |
|---|---|
| Engine CRUD | `engine/crud/relationships.rs:51` |
| Executor `CREATE` | `executor/operators/create.rs:513`, `:548`, `:1278` |
| Bulk loader | `loader/mod.rs:585` |

## Deletion — NO single path

`RecordStore::delete_rel` (`record_store_ops.rs:388`) exists, but most deletions
never call it — they read the record, `mark_deleted()`, and `write_rel` it back:

| Path | Call site | Uses `delete_rel`? |
|---|---|---|
| Cypher `DELETE r` | `engine/crud/nodes.rs:371-374` | no — direct `write_rel` |
| `DETACH DELETE` | `engine/crud/nodes.rs:591-592` | no — direct `write_rel` |
| Graph API | `graph/core/graph.rs:224/281`, `:386/389` | no — direct `write_rel` |
| Transaction rollback | `engine/transactions.rs:166`, `:279` | yes |
| `CALL { … }` undo | `executor/operators/call_subquery.rs:619` | yes |

Routing all of those through `delete_rel` (as item 1.3 suggests) would be a wide,
risky refactor of five call sites — and would still leave the invariant one
`write_rel` away from being bypassed by the next path someone adds.

## The real chokepoint: `write_rel`

`record_store_ops.rs:294`. Every mutation of a relationship record — create,
delete, and the `next_src_ptr` fix-ups `create_relationship` performs on the
previous edge — goes through it, and `rels_mmap` is written in exactly one place
(`record_store_ops.rs:319`, inside `write_rel`). Verified by grep: no other code
writes the relationship mmap, and nothing anywhere assigns `.src_id` / `.dst_id`
on an existing record.

Two consequences:

1. **Maintaining the adjacency index inside `write_rel` makes it authoritative by
   construction** — for every current path AND every future one, with no
   discipline required of callers. That is the property the non-authoritative
   `cache::RelationshipIndex` never had (executor `CREATE` and the loader write
   edges without calling `add_relationship`, which is exactly why it must not
   back correctness).
2. **Endpoints are immutable after the first write**, so the index never needs to
   move an entry between nodes: `write_rel` only has to decide add-or-remove from
   the record's own `is_deleted()` flag. No read-before-write on the hot path.

The one place that mutates relationship storage WITHOUT `write_rel` is
`clear_all` (`record_store_ops.rs:1081`), which re-maps the files wholesale. It
clears the index explicitly.
