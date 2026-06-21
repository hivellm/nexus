# Proposal: phase7_merge-persists-rel-props

Source: GitHub issue #25 (https://github.com/hivellm/nexus/issues/25)

## Why
`MERGE (a)-[r:T {k:'v'}]->(b)` silently drops the inline relationship
properties — the edge is created but `r.k` reads back as null, while the
same inline props via `CREATE` persist. This breaks the standard
idempotent edge-upsert pattern: the only way to persist rel props today is
a non-idempotent standalone `CREATE`, forcing a two-statement
delete-then-create workaround that doubles write volume. Separately, `SET`
on a relationship variable (`MATCH (a)-[r:T]->(b) SET r.k = 'v'`) is
rejected with "Unknown variable 'r' in SET clause" because the write-path
MATCH never binds relationship variables.

Root causes (traced):
- `engine/write_exec.rs::process_merge_relationship` creates the rel with a
  hardcoded empty props map (`Value::Object(Map::new())`), ignoring
  `rel_pattern.properties` — the CREATE path (`engine/match_exec.rs`)
  extracts and persists them.
- `engine/write_exec.rs::process_match_clause_multi` binds only
  `PatternElement::Node`, never `PatternElement::Relationship`, so a
  matched rel variable is unbound; `apply_set_clause` then can't resolve it.

## What Changes
- Fix A (MERGE persists inline rel props): `process_merge_relationship`
  evaluates `rel_pattern.properties` and passes them to
  `create_relationship` on the create branch (ON CREATE SET still applies
  on top). openCypher parity for the create case; idempotent upsert with
  props.
- Fix B (SET on a relationship variable): `process_match_clause_multi`
  binds a matched `(node)-[r:T]->(node)` relationship variable into a rel
  context; `apply_set_clause` (and the write-path SET dispatch) resolves
  rel-variable targets and updates relationship properties (`SET r.k = v`
  and `SET r += {…}`), reusing the existing
  `update_relationship_properties` storage seam.

## Impact
- Affected specs: cypher / MERGE / SET (relationship properties)
- Affected code: `crates/nexus-core/src/engine/write_exec.rs`
  (process_merge_relationship, process_match_clause_multi, apply_set_clause)
- Breaking change: NO (only persists props that were previously dropped, and
  accepts SET on rel vars that previously errored)
- User benefit: idempotent edge upsert with properties via MERGE; `SET r.*`
  works — removes the delete-then-create workaround entirely.
