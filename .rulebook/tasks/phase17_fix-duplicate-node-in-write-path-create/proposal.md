# Proposal: phase17_fix-duplicate-node-in-write-path-create

**Priority: HIGH — silent data corruption. Found and empirically confirmed while
implementing phase14; not previously reported and not tracked by any GitHub issue.**
Should be fixed before 2.6.0 ships.

## Why

`CREATE` with a relationship pattern, when combined in the same query with `SET`,
`REMOVE`, `MERGE`, or `FOREACH`, **creates a phantom duplicate of the target node**
and binds the query variable to the orphan rather than to the connected node.

Confirmed empirically against an isolated engine (temporary probe, since removed):

```
CREATE (a:Alpha {name:'a'})-[:LINKS]->(b:Beta {name:'b'})            → beta=1  connected=1  ✅
CREATE (a:Alpha {name:'a'})-[:LINKS]->(b:Beta {name:'b'}) SET a.k=1  → beta=2  connected=1  ❌
```

**Mechanism.** In `crates/nexus-core/src/engine/write_exec.rs`, the linear CREATE arm
iterates `create_clause.pattern.elements`, which for `(a)-[:R]->(b)` is
`[Node(a), Relationship, Node(b)]` (`executor/parser/clauses/pattern.rs:20,50,54`).
The `Relationship` arm creates the target node itself by peeking at
`elements.get(i + 1)` so it can wire the edge — but the loop has **no skip
mechanism**, so it then advances to `i + 1` and the `Node` arm creates `b` a *second*
time. The second, unconnected node overwrites `context["b"]` and `last_node_id`.

Consequences beyond the stray node: any later clause in the same query that
references `b` (e.g. `SET b.x = 1`, or a `RETURN b`) operates on the **orphan**, not
on the node the relationship actually points at. So this silently writes to the wrong
node, not merely leaks an extra one.

**Why it is scoped to this path.** Plain `CREATE` with no `SET`/`REMOVE`/`MERGE`/
`FOREACH` routes to the executor create operator instead
(`engine/query_pipeline.rs:664`), which handles the pattern correctly — verified
above. Only `execute_write_query` is affected, i.e. exactly the routing branch at
`query_pipeline.rs:659-662`.

**Relationship to phase14.** Discovered while fixing issue #29 in the same function.
phase14's `ext_id_consumed` guard already ensures the duplicate node does not *also*
claim the external id, so the two bugs do not compound — but phase14 deliberately did
not fix the duplication itself, to keep that diff reviewable and because this defect
needs its own tests.

## What Changes

Give the linear CREATE loop a way to skip elements the `Relationship` arm already
consumed — e.g. track consumed indices in a `HashSet<usize>` (or restructure the walk
to advance past the target node), so each pattern element produces exactly one node.

Two details to get right:

- The variable binding and `last_node_id` must end up pointing at the **connected**
  node, which is the one the `Relationship` arm created — not a later re-creation.
- Chained patterns (`(a)-[:R]->(b)-[:S]->(c)`) must still work: `b` is both a
  relationship target and the source of the next relationship, so skipping must not
  break the `last_node_id` handoff.

Verify the equivalent walk in the UNWIND+CREATE arm
(`write_exec.rs:440-475`) — it currently rejects relationship elements outright, so it
is likely unaffected, but confirm rather than assume.

## Impact

- Affected specs: `docs/specs/cypher-subset.md` (CREATE semantics, if the documented
  behaviour needs clarifying)
- Affected code: `crates/nexus-core/src/engine/write_exec.rs` (linear CREATE arm,
  ~`:126-210`), new tests in `crates/nexus-core/tests/`
- Breaking change: NO — it fixes silently wrong behaviour. Note that deployments may
  hold orphan nodes created by this bug; assess whether a cleanup/detection query is
  warranted (orphan detection is straightforward: nodes of the target label with no
  incoming relationship that have an identical-property twin that does).
- User benefit: `CREATE … SET` stops silently corrupting the graph with duplicate,
  unconnected nodes and stops binding variables to the wrong node.

## References

- Empirical confirmation: probe run 2026-07-19 on branch `release/2.6.0`, results
  quoted above.
- Faulty loop: `crates/nexus-core/src/engine/write_exec.rs` linear CREATE arm; the
  `Relationship` arm creating `elements.get(i + 1)` is the peek that is never skipped.
- Pattern element ordering: `crates/nexus-core/src/executor/parser/clauses/pattern.rs:20,50,54`
- Routing that selects this path: `crates/nexus-core/src/engine/query_pipeline.rs:659-666`
- Correct reference behaviour: the executor create operator
  (`crates/nexus-core/src/executor/operators/create.rs`)
