# Proposal: phase0_fix-create-ignores-arrow-direction

## Why

`CREATE` treats relationship pattern elements as source->target in
left-to-right ARRAY order, ignoring the arrow direction entirely:
`RelationshipDirection` (`Incoming`/`Outgoing`/`Both`) is not read anywhere in
`crates/nexus-core/src/executor/operators/create.rs` (confirmed by search,
2026-07-21, during the MATCH...CREATE inline-target fix — commit 87ecca67).
So `CREATE (x:Y)<-[:T]-(a)` writes the edge as x->T->a instead of a->T->x —
silently reversed. Any traversal, direction-sensitive MATCH, or export built
on such data is wrong. Neo4j honours the arrow. This affects BOTH
`execute_create_with_context` (MATCH...CREATE) and the standalone
`execute_create_pattern_internal`, which share the same array-order
convention. MERGE's relationship path must be audited for the same gap.

## What Changes

- Reproduce and pin with failing tests: `CREATE (a:A)-[:T]->(b:B)` vs
  `CREATE (b:B)<-[:T]-(a:A)` must produce identical stored edges
  (src=a, dst=b), asserted via direction-sensitive MATCH from both ends.
- Fix both CREATE code paths to consult the parsed relationship direction
  when choosing src/dst ids; decide-and-document the semantics of the
  undirected form (`-[:T]-`) in CREATE (Neo4j rejects it — prefer an explicit
  error over a silent default).
- Audit `process_merge_relationship` and any other relationship-writing path
  for the same array-order assumption; fix or file follow-ups with evidence.
- Regression tests for both directions on both CREATE paths (standalone and
  MATCH-context) and for the undirected-form decision.

## Impact

- Affected specs: `docs/specs/cypher-subset.md` (CREATE semantics)
- Affected code: `crates/nexus-core/src/executor/operators/create.rs`,
  possibly `crates/nexus-core/src/engine/write_exec.rs` (MERGE relationship)
- Breaking change: BEHAVIORAL fix — data written by reversed-arrow CREATE
  changes orientation to the correct one; existing databases written under
  the bug keep their stored (wrong) orientation. Call this out in the
  CHANGELOG.
- User benefit: relationship direction in the database matches the query as
  written; Neo4j-compatible semantics
