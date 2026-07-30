# Proposal: phase21_tck-expand-target-label-enforcement

> Part of the openCypher TCK 100%-conformance epic. Plan: docs/analysis/tck/.
> Surfaced during the WITH...WHERE/expand fixes (independent review probes).

## Why
Inline label constraints on pattern nodes beyond the first are not enforced by
the expand path. Two confirmed manifestations:
- `MATCH (a:A)-[:KNOWS]->(b:X)-->(c:X)` returns `:Y`-labeled nodes for `c`
  (probed on the binary-tree-2 TCK fixture) — this caps the last 4
  useCases/triadicSelection scenarios ([7],[8],[9],[17]).
- `MATCH (a:A) OPTIONAL MATCH (a)-[r:T]->(c:C)` binds `c` to a `:D` node with
  `r IS NULL = false` — the optional target's label predicate is ignored, so
  rows that should be padded with NULL are filled with wrong matches.

## What Changes
Enforce the declared label(s) of expand *target* nodes when filtering candidate
relationships, in both required and optional expand. For optional expand a
label-rejected candidate must count as "no match" (row padded with NULL per
LEFT-OUTER semantics), not as a match. Audit whether the same gap exists in
multi-hop chains lowered through consecutive Expand operators and in the
planner's pattern-lowering (where inline labels may be dropped before reaching
the operator).

## Impact
- Affected specs: docs/specs/cypher-subset.md (MATCH pattern semantics)
- Affected code: crates/nexus-core/src/executor/operators/expand.rs, possibly
  executor/planner/queries/ (pattern lowering carrying node label info)
- Breaking change: NO (current behavior returns spec-wrong rows)
- User benefit: label predicates in patterns honored everywhere; closes the
  remaining 4 useCases/triadicSelection scenarios plus any match/optional-match
  scenarios where inline labels on non-anchor nodes select wrong rows.
