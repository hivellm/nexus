# Proposal: phase0_fix-varlength-multi-reltype-dropped

**Priority: CRITICAL — a variable-length relationship pattern naming more
than one relationship type (`[:R1|R2*1..3]`) silently traverses only the
first type; every row reachable solely through the other type(s) is dropped
with no error.** Found during a query-planner correctness audit; not
previously reported.

## Why

The Cypher parser produces the full list of relationship types for a
variable-length pattern as `type_ids: Vec<u32>`, exactly as it does for a
single-hop pattern. But the default (non-QPP) lowering path for
variable-length patterns keeps only the first one
(`crates/nexus-core/src/executor/planner/queries/relationships.rs:158-169`):

```rust
let path_var = pattern.path_variable.clone().unwrap_or_default();
// For variable-length paths, only use first type for now
let type_id = type_ids.first().copied();
operators.push(Operator::VariableLengthPath {
    type_id,
    direction,
    source_var,
    target_var,
    rel_var: rel.variable.clone().unwrap_or_default(),
    path_var,
    quantifier: quantifier.clone(),
});
```

`Operator::VariableLengthPath.type_id` is declared as a single
`Option<u32>` (`executor/types.rs:456-471`), not a `Vec<u32>`, so the operator
is **structurally** incapable of traversing more than one relationship type —
this is not a filtering bug downstream, the second and later types are
discarded before the operator is even constructed. Every consumer of the
operator (`executor/operators/path.rs`) converts the single `Option<u32>`
back into an at-most-one-element slice before calling `find_relationships`,
e.g. `let type_ids_slice: Vec<u32> = type_id.into_iter().collect();`
(path.rs:1062, 1164, 1218, 1283, 1311), so the BFS can only ever match one
type.

Contrast this with the single-hop lowering right below it, which already
carries the full type list (relationships.rs:172-179):

```rust
// Use regular Expand operator for single-hop relationships
operators.push(Operator::Expand {
    type_ids,
    source_var: final_source_var,
    target_var: final_target_var,
    rel_var: rel.variable.clone().unwrap_or_default(),
    direction: final_direction,
    optional: is_optional,
    ...
```

This is the **default** code path: the QPP-rewrite alternative
(`qpp_legacy_rewrite_enabled()`, relationships.rs:122) — which does thread
the full `type_ids` into `QppHopSpec` — is off by default pending a
performance-parity gate, so essentially all `*m..n` variable-length queries
in production go through the lossy `VariableLengthPath` branch.

### Consequence (confirmed by code inspection)

`MATCH (a:Person {name:'Alice'})-[:KNOWS|FOLLOWS*1..3]->(b:Person) RETURN
b.name` only traverses `KNOWS` edges (the first type in the parsed list); any
`b` reachable exclusively via one or more `FOLLOWS` hops is silently absent
from the result, with no error or warning. This reproduces without any index
and affects the single most common way to express a multi-type
variable-length traversal in Cypher.

## What Changes

- Change `Operator::VariableLengthPath.type_id: Option<u32>` to
  `type_ids: Vec<u32>` (mirroring `Operator::Expand.type_ids`), and update the
  lowering at `relationships.rs:158-169` to pass the full parsed `type_ids`
  list instead of `type_ids.first().copied()`.
- Update every BFS/traversal call site in `executor/operators/path.rs` that
  currently rebuilds a one-element slice from `type_id` to pass the operator's
  `type_ids: Vec<u32>` straight through to `find_relationships`, matching on
  membership in the full type set (already what `find_relationships` does for
  `Expand`'s `type_ids: &[u32]`, path.rs:56).
- Preserve "empty = match all types" semantics: `find_relationships` already
  treats `type_ids.is_empty()` as unfiltered (path.rs:56, 345, 490, 601); an
  unqualified `[*1..3]` pattern must continue to traverse every relationship
  type.

## Impact

- Affected specs: `docs/specs/cypher-subset.md` (variable-length relationship
  pattern semantics)
- Affected code: `crates/nexus-core/src/executor/planner/queries/relationships.rs`
  (lowering `:158-169`), `crates/nexus-core/src/executor/types.rs`
  (`Operator::VariableLengthPath` struct `:456-471`),
  `crates/nexus-core/src/executor/operators/path.rs` (BFS traversal and every
  `type_id.into_iter().collect()` call site)
- Breaking change: NO for query semantics — this is a correctness fix; the
  `Operator::VariableLengthPath.type_id` field rename/retype is an internal
  API change with no external surface
- User benefit: `[:R1|R2*m..n]` and longer type-union variable-length
  patterns return every reachable row instead of silently dropping rows
  reachable only through non-first types
- Related: `phase0_fix-plan-reorder-drops-predicates` (another
  `VariableLengthPath`-adjacent planner correctness defect, in operator
  ordering rather than type handling)
