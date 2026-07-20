# Proposal: phase0_fix-plan-reorder-drops-predicates

**Priority: CRITICAL — the cost-based operator-reordering pass silently drops
WHERE predicates (or expands them into O(all edges) full scans) whenever a
compiled plan contains a `NodeIndexSeek`, `VariableLengthPath`, or
`QuantifiedExpand` operator, on every query that reaches this pass.** Found
during a query-planner correctness audit; not previously reported.

## Why

`optimize_operator_order`
(`crates/nexus-core/src/executor/planner/queries/cost.rs:590-614`) buckets
each operator in a compiled plan before recombining them in a fixed order.
The bucketing match recognizes exactly three operator kinds as "scans"
(cost.rs:592-596):

```rust
Operator::NodeByLabel { .. }
| Operator::AllNodesScan { .. }
| Operator::IndexScan { .. } => {
    scans.push(operator);
}
```

`NodeIndexSeek`, `VariableLengthPath`, and `QuantifiedExpand` — every one of
which **binds** a variable to a freshly generated set of rows rather than
filtering/consuming an existing row stream — fall through to the catch-all
(cost.rs:610-612):

```rust
_ => {
    others.push(operator);
}
```

Recombination then places `others` **last**, after `filters` and even after
`optimized_joins` (cost.rs:654-659):

```rust
result.extend(optimized_scans);
result.extend(expansions);
result.extend(unwinds);
result.extend(filters);
result.extend(optimized_joins);
result.extend(others);
```

So any `Filter` or `Expand` that references the variable bound by a
`NodeIndexSeek`/`VariableLengthPath`/`QuantifiedExpand` ends up **before** the
operator that produces that variable's rows. These binding operators
regenerate their entire result set independently — `seed_scan_variable`
(`executor/operators/dispatch.rs:497-539`) does not consume upstream rows —
while `evaluate_predicate_on_row` treats a missing/unbound variable as
`Null`, which evaluates to `false` (`executor/eval/helpers.rs:868-876`). A
`Filter` executed before its binding operator therefore runs against empty
input and is a silent no-op; the binding operator then reappears downstream
and regenerates the **full, unfiltered** result set as if the filter had
never existed.

`node_index_seek_for` is what emits `NodeIndexSeek` (strategy.rs:1307-1342);
`VariableLengthPath` is emitted at `relationships.rs:161`. This pass runs on
every query, invoked from `executor/dispatch.rs:1308` inside the main
`execute_inner` path (`dispatch.rs:172-175`).

### Consequences (confirmed by code inspection)

Given `CREATE INDEX FOR (n:Person) ON (n.id)`:

- `MATCH (n:Person {id:'b'}) WHERE n.age > 30 RETURN n` reorders to
  `[Filter(id='b'), Filter(age>30), NodeIndexSeek(n), Project]`. Both filters
  run against empty input (no-op); the seek regenerates **all**
  `:Person{id:'b'}` rows — the `age>30` predicate is silently dropped and the
  query returns rows that violate its own WHERE clause.
- `MATCH (a:Person {id:'x'})-[:R]->(b) RETURN b` places `Expand` before
  `NodeIndexSeek(a)`; `Expand` with an unbound source scans **every** `:R`
  edge in the graph (`operators/expand.rs:108-186`) — wrong results plus an
  O(all edges) cost cliff.
- `MATCH (a:A)-[:R*1..2]->(b) WHERE b.name='x' RETURN b` places
  `Filter(b.name)` before `b` is bound — always returns an empty result.

`engine/tests/indexes.rs:202-217` asserts operator **presence**, not order,
so it passes unchanged on the misordered plan — this regression has no test
coverage today. The path is gated on a real property index being installed
(`dispatch.rs:1301-1303`); planner unit tests constructed via
`QueryPlanner::new` install none, which is why it went uncaught.

## What Changes

- Add `NodeIndexSeek`, `CompositeBtreeSeek`, `SpatialSeek`,
  `VariableLengthPath`, and `QuantifiedExpand` to the scan/expand buckets in
  `optimize_operator_order` (cost.rs:590-614) so every variable-**binding**
  operator recombines before any operator that depends on the variables it
  binds (cost.rs:638-660).
- Prefer, if feasible within scope, making `optimize_operator_order`
  order-preserving with respect to variable dependencies (a topological
  ordering keyed on bound/consumed variables) rather than enumerating binding
  operator kinds by name — a category-bucket list must be kept in sync by
  hand for every future binding operator and will silently regress again
  otherwise. Document the trade-off if out of scope for this fix.
- Strengthen `engine/tests/indexes.rs:202-217` (or add a companion test) to
  assert compiled operator **order**, not just presence, so this class of
  regression is caught by CI going forward.

## Impact

- Affected specs: `docs/specs/cypher-subset.md` (query planning / WHERE
  semantics)
- Affected code: `crates/nexus-core/src/executor/planner/queries/cost.rs`
  (bucketing `:590-614`, recombine `:638-660`), `strategy.rs`
  (`node_index_seek_for` `:1307-1342`), `relationships.rs`
  (`VariableLengthPath` emission `:161`), `executor/operators/dispatch.rs`
  (`seed_scan_variable` `:497-539`, invocation `:1308`)
- Breaking change: NO — this is a correctness fix; query results become
  correct where they were previously silently wrong
- User benefit: WHERE predicates that follow an index seek or variable-length
  path are no longer dropped; queries return correct rows instead of
  unfiltered/duplicated ones, and avoid unbounded full-graph scans
- Related: `phase0_fix-correlated-predicate-index-seek`,
  `phase0_fix-where-clause-index-seek` — other planner/index-seek defects in
  the same area
