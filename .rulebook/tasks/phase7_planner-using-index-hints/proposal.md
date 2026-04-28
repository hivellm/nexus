# Proposal: phase7_planner-using-index-hints

## Why

The Cypher parser already accepts `USING INDEX <var>:<Label>(<prop>)`, `USING SCAN <var>:<Label>`, and `USING JOIN ON <var>` clauses, but the planner ignores them. This is a regression vs Neo4j's behaviour and removes the only escape hatch a DBA has when the heuristic planner picks a wrong index. Today, the planner greedily prefers the label-bitmap index even when a property-equality predicate is more selective (`docs/analysis/nexus/02_architecture_assessment.md` §4 lists this explicitly). Honouring `USING INDEX` adds tunability with no algorithmic complexity — the hint is a directive, not a suggestion.

## What Changes

- In `crates/nexus-core/src/executor/planner/`, propagate parsed `UsingHint` AST nodes into the operator-selection step.
- For `USING INDEX`: force the named index even if cost-model would pick another. Fail with a clear error if the named index does not exist.
- For `USING SCAN`: bypass index selection entirely and emit a label scan.
- For `USING JOIN ON v`: force a hash-join on the named variable.
- Add planner unit tests for each hint shape and a "hint conflicts with cost model" test (hint wins).
- Update `docs/specs/cypher-subset.md` to mark these as supported.

## Impact

- Affected specs: `docs/specs/cypher-subset.md`.
- Affected code: `crates/nexus-core/src/executor/planner/`, parser already done.
- Breaking change: NO (currently silent → now honoured).
- User benefit: DBA can override planner mistakes; Neo4j-compat level rises.
