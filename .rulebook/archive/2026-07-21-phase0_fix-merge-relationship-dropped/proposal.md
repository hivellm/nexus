# Proposal: phase0_fix-merge-relationship-dropped

**Priority: CRITICAL — `MERGE (a)-[:KNOWS]->(b)`, the most common relationship-MERGE
form in Cypher, silently drops the edge AND the second node, with no error.** Found
during a write-path/index-corruption audit; not previously reported.

## Why

`process_merge_relationship` (`crates/nexus-core/src/engine/write_exec.rs:851-962`) is
the only code path that creates a relationship as part of a 3-element `Node,
Relationship, Node` MERGE pattern. It bails out to `Ok(None)` whenever the
relationship or either endpoint has no bound *variable* — which is exactly the
common, idiomatic case of an anonymous relationship type:

```rust
let rel_var = match &rel_pattern.variable {
    Some(v) => v.clone(),
    None => return Ok(None),
};
```
(`write_exec.rs:887-890`; the endpoint-variable checks are the same shape at
`877-884`.)

The caller does not treat `Ok(None)` as an error — it treats it as "not a
relationship MERGE, fall back to node-only MERGE" (`write_exec.rs:219-233`):

```rust
if let Some((rel_var, rel_id, rel_type)) =
    self.process_merge_relationship(&merge_clause, &mut context)?
{
    ...
} else {
    // Fall back to node MERGE
    let (variable, node_ids) = self.process_merge_clause(merge_clause)?;
    context.insert(variable, node_ids);
}
```

`process_merge_clause` (`write_exec.rs:558-573`) then walks the pattern's
`elements` and merges only the **first** `Node` it finds via `find_map` — it has
no knowledge of a relationship or second node in the pattern at all. So for
`MERGE (a:Person{name:'Alice'})-[:KNOWS]->(b:Person{name:'Bob'})`:

- `rel_pattern.variable` is `None` (no `r` in `[:KNOWS]`) → `process_merge_relationship`
  returns `Ok(None)` at line 890, having never looked at `b` or created the edge.
- The fallback `process_merge_clause` merges only `a` (`Alice`).
- `b` (`Bob`) is never created or matched; `[:KNOWS]` is never created or matched.
- No error, no notification — the query returns success.

### Consequence (confirmed by code inspection and the trigger below)

```
MERGE (a:Person{name:'Alice'})-[:KNOWS]->(b:Person{name:'Bob'})
MATCH (b:Person{name:'Bob'}) RETURN b            -- 0 rows (Bob never created)
MATCH (a:Person{name:'Alice'})-[r:KNOWS]->() RETURN r  -- 0 rows (edge never created)
```

Anonymous relationship types and anonymous node endpoints are ordinary,
widely-used Cypher (`-[:KNOWS]->`, `-[:KNOWS]->(  )`) — this is not an edge case,
it fires on the single most common way to write a relationship MERGE.

## What Changes

- Synthesize internal (non-user-visible) variable names for any pattern element in
  a MERGE relationship pattern that lacks one — anonymous source node, anonymous
  relationship, and/or anonymous destination node — before dispatching into
  `process_merge_relationship`, so the full 3-element pattern is always resolved
  and the early `Ok(None)` bailouts at `write_exec.rs:877-890` are only taken for
  patterns that are genuinely not a `Node, Relationship, Node` shape (line 858's
  `elements.len() != 3` check stays as the real "not a relationship MERGE" gate).
- Ensure the relationship type is still required (`rel_pattern.types.first()` at
  `write_exec.rs:891-894`) — that check is legitimate and unrelated to this bug.
- Keep the synthesized variables out of `RETURN`/`context` visibility so they do
  not leak into user-facing bindings or column names.

## Impact

- Affected specs: `docs/specs/cypher-subset.md` (MERGE semantics — relationship
  pattern with anonymous endpoints/relationship)
- Affected code: `crates/nexus-core/src/engine/write_exec.rs`
  (`process_merge_relationship` `:851-962`, its caller `:219-233`,
  `process_merge_clause` `:558-573`)
- Breaking change: NO — this fixes silent data loss; previously-passing queries
  that relied on the drop (none should exist) are not a supported contract
- User benefit: `MERGE (a)-[:TYPE]->(b)` and any anonymous-endpoint/anonymous-rel
  MERGE pattern now creates or matches the full pattern atomically, as Cypher
  requires, instead of silently discarding the edge and the second node
- Related: `phase0_fix-create-path-index-and-constraints` (MERGE existence-check
  correctness on the CREATE side of the same write paths),
  `phase0_fix-relationship-write-clauses-dropped` (other MERGE/relationship
  clause-dropping defects in the same file)
