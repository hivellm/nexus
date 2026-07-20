# Proposal: phase0_fix-create-path-index-and-constraints

**Priority: CRITICAL — nodes created via `MATCH…CREATE` are invisible to the
engine's typed property index (later `MERGE` duplicates them), and nodes created
via a bare `CREATE` silently bypass `NODE KEY`/composite-index enforcement.** Found
during a write-path/index-corruption audit; two related defects on the CREATE path
grouped into one task because both are "a CREATE variant that skips index/constraint
maintenance the sibling CREATE variant performs."

## Why

### C-5 — MATCH…CREATE omits typed-property-index maintenance

`query_pipeline.rs` has two CREATE branches. The standalone-CREATE branch calls
`index_typed_properties_for_new_nodes` after syncing storage
(`query_pipeline.rs:776-799`, call at `:799`):

```rust
let pre_create_node_count = self.storage.node_count();
let result = match source { ... };
// CRITICAL: Sync executor's store back to engine's storage
self.storage = self.executor.get_store();
self.index_typed_properties_for_new_nodes(pre_create_node_count);
```

The MATCH…CREATE branch (`query_pipeline.rs:704-725`) syncs `self.storage` back
(`:708`) but never calls `index_typed_properties_for_new_nodes`, and defers
`refresh_executor` to the caller:

```rust
let result = self.execute_match_create_query(ast, query_str_opt)?;
// CRITICAL: Sync executor's store back to engine's storage
// The executor has a cloned store, so changes need to be synced back
self.storage = self.executor.get_store();
```

The executor's own CREATE operator only touches its cloned label index — it never
writes to `Engine::indexes.property_index` (the same watermark comment at
`query_pipeline.rs:764-767` documents this gap for the sibling branch: "the
executor CREATE path writes storage + the label index but NOT the typed property
B-tree"). So a node created through `MATCH…CREATE` is durably stored and
label-indexed, but absent from the typed B-tree.

`MERGE`'s existence check, `find_nodes_by_node_pattern`
(`crud/lookup.rs:191-224`), resolves indexed filters through
`self.indexes.property_index.find_exact` (`:193-207`) before ever touching
storage — a node missing from that index is invisible to the check, so `MERGE`
concludes "not found" and creates a duplicate:

```
CREATE INDEX ON :Person(id);
MATCH (s:Seed) CREATE (n:Person {id:42});
MERGE (m:Person {id:42});
MATCH (p:Person {id:42}) RETURN count(p);   -- returns 2
```

This is distinct from the already-known standalone-CREATE duplicate bug: here the
node is genuinely created once, but its own index entry is missing, so a
*subsequent* `MERGE` cannot find it and creates a second, real duplicate.

### M-2 — bare CREATE skips constraint enforcement and composite-index population

The executor's CREATE operator (`executor/operators/create.rs`) runs only its own,
local constraint check (`check_constraints`, defined at `create.rs:641`, called at
`:253` inside `execute_create_pattern_internal`). This is a **different function**
from the engine's `check_constraints` (`engine/constraints.rs:567`) — and the
engine also has `enforce_extended_node_constraints` (`engine/constraints.rs:304`,
`pub(crate)`) and `index_composite_tuples` (`engine/crud/index_maintenance.rs:90`,
`pub(in crate::engine)`), neither of which is reachable from
`executor/operators/create.rs`. A bare `CREATE` therefore:

- Enforces only whatever the executor-local `check_constraints` implements
  (existence/uniqueness on a single property), never the engine's extended
  constraint set that includes `NODE KEY`.
- Never populates the composite B-tree (`index_composite_tuples`), so composite
  and `NODE KEY` indexes are left un-backed by any node created through a bare
  `CREATE`.

A `NODE KEY` constraint is therefore silently unenforced, and composite indexes
silently diverge from the graph, for the single most common way to create a node.

## What Changes

- **C-5**: route the MATCH…CREATE branch (`query_pipeline.rs:704-725`) through the
  same `index_typed_properties_for_new_nodes(pre_create_node_count)` call the
  standalone branch already makes (`:799`), using the node-count watermark taken
  before `execute_match_create_query` runs, and ensure the engine's label index
  (`Engine::indexes`, not just the executor's cloned copy) is synced the same way
  the standalone branch's comment at `:764-770` describes.
- **M-2**: make `executor/operators/create.rs`'s node-creation path invoke the
  engine's constraint/composite-index maintenance — either by routing bare CREATE
  through the engine's `enforce_extended_node_constraints` /
  `index_composite_tuples` (mirroring how MATCH…CREATE and MERGE already reach
  engine-level helpers), or by exposing equivalent maintenance callable from the
  executor and invoking it alongside the existing local `check_constraints` at
  `create.rs:253`.
- Do not remove the executor-local `check_constraints` — it may still serve as a
  fast local rejection — but it must not be the *only* enforcement for
  extended/composite constraints.

## Impact

- Affected specs: `docs/specs/cypher-subset.md` (CREATE / MERGE / constraint
  enforcement semantics — index-visibility contract of newly created nodes)
- Affected code: `crates/nexus-core/src/engine/query_pipeline.rs`
  (MATCH…CREATE branch `:704-725`, standalone branch `:776-799`),
  `crates/nexus-core/src/executor/operators/create.rs`
  (`execute_create_pattern_internal` `:97-256`, local `check_constraints` `:641`),
  `crates/nexus-core/src/engine/crud/lookup.rs`
  (`find_nodes_by_node_pattern` `:191-224`, the consumer that surfaces C-5),
  `crates/nexus-core/src/engine/constraints.rs`
  (`enforce_extended_node_constraints` `:304`, engine `check_constraints` `:567`),
  `crates/nexus-core/src/engine/crud/index_maintenance.rs`
  (`index_composite_tuples` `:90`)
- Breaking change: NO — this closes silent duplication and silent constraint
  bypass; no currently-passing query should depend on either defect
- User benefit: a node created via `MATCH…CREATE` is immediately visible to typed
  property indexes (no more MERGE-created duplicates); a `NODE KEY`/composite
  constraint is enforced and its index populated regardless of which CREATE form
  (bare or MATCH-prefixed) created the node
- Related: `phase0_fix-merge-relationship-dropped` (adjacent MERGE/CREATE
  write-path defect in the same subsystem), `phase0_fix-delete-path-index-cleanup`
  (composite B-tree also never evicted on delete — the write and delete sides of
  the same composite-index maintenance gap)
