# Quantified Path Patterns (QPP)

> Status: **slices 1, 2, 3a shipped in 1.15.0**. Anonymous-body
> shape lowers to the legacy `*m..n` operator at parse time.
> Single-relationship and multi-hop bodies with named/labelled
> inner nodes execute via the dedicated `QuantifiedExpand`
> operator. Inner `WHERE` push-down and `shortestPath(qpp)` over
> named-body shapes are slice 3b — see
> `phase6_opencypher-quantified-path-patterns`.

Quantified Path Patterns are the marquee feature of Cypher 25 (the
GQL-aligned release). Where the legacy `*m..n` shorthand quantifies
a single relationship, QPP quantifies a whole parenthesised path
fragment, so traversals that previously required `UNWIND` +
recursion fold into one pattern expression.

## What works today

### Anonymous-body shape (slice 1, lowered)

```cypher
MATCH (a)( ()-[:T]->() ){m,n}(b) RETURN a, b
```

Both boundary nodes inside the parentheses are **anonymous** — no
variable, no label, no property map — and the body holds exactly
one relationship. Direction (`->`, `<-`, `-`), relationship type,
relationship variable, and the relationship's property map are all
preserved on lowering.

This shape is lowered at parse time to the legacy form

```cypher
MATCH (a)-[:T*m..n]->(b) RETURN a, b
```

so the existing `VariableLengthPath` operator handles it. From a
user's perspective, anything you can express as `*m..n` you can
also express as `( ()-[:T]->() ){m,n}` and get byte-identical row
sets.

### Examples

```cypher
-- Bounded range (1 to 5 hops)
MATCH (a:Employee)( ()-[:REPORTS_TO]->() ){1,5}(ceo:CEO)
RETURN a, ceo

-- Exact count
MATCH (a)( ()-[:KNOWS]->() ){3}(b) RETURN a, b

-- Optional (`?` is `{0,1}`)
MATCH (a)( ()-[:LIKES]->() )?(b) RETURN a, b

-- Unbounded (`*` is `{0,}`, `+` is `{1,}`)
MATCH (a)( ()-[:CHILD_OF]->() )+(b) RETURN a, b

-- Incoming and bidirectional
MATCH (a)( ()<-[:OWNS]-() ){1,3}(b) RETURN a, b
MATCH (a)( ()-[:FRIEND_OF]-() ){1,2}(b) RETURN a, b

-- Relationship variable for length checks
MATCH (a)( ()-[r:KNOWS]->() ){1,3}(b)
RETURN a.name, length(r) AS hops

-- Property filter on the relationship
MATCH (a)( ()-[:RATED {weight: 5}]->() ){1,2}(b) RETURN a, b

-- Inside shortestPath
MATCH p = shortestPath((a)( ()-[:KNOWS]->() ){1,5}(b))
RETURN length(p)
```

## Quantifiers

| Form    | Meaning             | Desugars to |
|---------|---------------------|-------------|
| `{n}`   | Exactly `n` times   | `*n..n`     |
| `{m,n}` | Between `m` and `n` | `*m..n`     |
| `{m,}`  | At least `m` times  | `*m..`      |
| `{,n}`  | At most `n` times   | `*0..n`     |
| `+`     | One or more         | `*1..`      |
| `*`     | Zero or more        | `*0..`      |
| `?`     | Optional (zero or one) | `*0..1`  |

The parser rejects `{n,m}` with `n > m` at parse time
(`ERR_QPP_INVALID_QUANTIFIER`) and rejects nested QPP groups
(`ERR_QPP_NESTING_TOO_DEEP`) — Cypher 25 itself only allows one
level of nesting.

### Named / labelled inner-boundary shape (slice 2, dedicated operator)

```cypher
-- One named inner node, list-promoted to LIST<NODE>
MATCH (a)( (x:Person)-[:KNOWS]->() ){1,5}(b)
RETURN a.name, x

-- Both inner nodes named (two LIST<NODE> bindings)
MATCH (a)( (m:Manager)-[r:REPORTS_TO]->(emp:Employee) ){1,3}(b)
RETURN m, r, emp

-- Inline relationship-property filter
MATCH (a)( (x)-[:RATED {weight: 5}]->() ){1,2}(b) RETURN x

-- Zero-length (`{0,n}`) — empty path satisfies, lists empty
MATCH (a)( (x:Person)-[:KNOWS]->() ){0,2}(b) RETURN a, x, b
-- Row with a == b, x == [] is a valid result
```

Each variable declared inside the body is list-promoted in the
outer scope, indexed by iteration count: `x[0]` is the start
node of iteration 0, `r[2]` is the relationship traversed on
iteration 2.

### Multi-hop body (slice 3a, dedicated operator)

```cypher
-- Two relationships per iteration; every named inner var
-- list-promotes one entry per iteration
MATCH (a:Person)( (x:Person)-[:KNOWS]->(y:Person)-[:KNOWS]->(z:Person) ){1,3}(b:Person)
RETURN x, y, z
-- x, y, z each are LIST<NODE> with iteration-count entries
```

Body arity is `n` (number of relationships) — `inner_nodes.len()
== n + 1`. The operator walks every hop in order per iteration,
applying per-position node filters and per-hop relationship
filters at each candidate.

## What does **not** work yet (slice 3b)

Three remaining surface gaps still hit
`ERR_QPP_NOT_IMPLEMENTED` or fail with a parse error:

- **Inner `WHERE` clauses**:
  `MATCH (a)( (x)-[:T]->(y) WHERE x.age > y.age ){1,5}(b)` —
  the parser does not yet accept `WHERE` inside the body
  parentheses. Workaround: rewrite as a
  filter on the outer scope's list-promoted variables, e.g.
  `MATCH (a)( (x)-[:T]->(y) ){1,5}(b)
   WHERE all(idx IN range(0, length(x) - 1) WHERE x[idx].age > y[idx].age)
   RETURN a, b`.
- **`shortestPath(qpp)` over named-body shapes**:
  `shortestPath((a)( (m:Manager)-[:REPORTS_TO]->() ){1,5}(b))` —
  the parser routes `shortestPath` arguments through `parse_pattern`
  which lowers QPP at parse time, but only the slice-1 anonymous
  body lowers cleanly. Named-body QPP under `shortestPath` runs
  the slice-2/3a operator without the cost-bound BFS pruning
  `shortestPath` needs.
- **`CREATE` with QPP**: pattern construction with QPP is a
  read-only feature in Cypher 25; `CREATE` returns
  `ERR_QPP_NOT_IN_CREATE`.

For all three, the workaround is the legacy `*m..n` form or
breaking the QPP into multiple `MATCH` clauses.

## Error codes

| Code                          | Severity | When                                          |
|-------------------------------|----------|-----------------------------------------------|
| `ERR_QPP_INVALID_QUANTIFIER`  | Error    | `{n,m}` with `n > m`                          |
| `ERR_QPP_NESTING_TOO_DEEP`    | Error    | More than one level of nested QPP groups      |
| `ERR_QPP_NOT_IN_CREATE`       | Error    | QPP appears inside a `CREATE` clause          |
| `ERR_QPP_NOT_IMPLEMENTED`     | Error    | Body shape the operator can't drive yet       |
| `ERR_QPP_UNBOUND_UPPER`       | Warn     | Unbounded quantifier hit `MAX_QPP_DEPTH = 64` |

The four errors surface as HTTP 400 with positional spans so
clients can render them inline. The
`ERR_QPP_UNBOUND_UPPER` warning emits to the server's `tracing`
log (target `nexus_core::executor::quantified_expand`,
field `code = "ERR_QPP_UNBOUND_UPPER"`) when an unbounded
quantifier (`*` / `+` / `{m,}`) hits the per-query safety cap of
`MAX_QPP_DEPTH = 64` iterations with candidates still pending.
The query keeps returning rows up to the cap, but the result set
is truncated — bound the quantifier (`{m,n}` with an explicit
`n`) to silence the warning.

## Migration tips

If you are coming from Neo4j 5.9+:

1. Anonymous-body shapes lower to legacy `*m..n` at parse
   time — bring them across as-is, runtime is identical.
2. Named or labelled inner-boundary shapes execute via the
   dedicated operator — bring them across as-is too. Both
   single-relationship (`( (x:Person)-[:T]->() ){1,5}`) and
   multi-hop (`( (x)-[:A]->(y)-[:B]->(z) ){1,3}`) bodies
   work; every named inner var list-promotes to `LIST<NODE>`
   / `LIST<RELATIONSHIP>` indexed by iteration.
3. Inner `WHERE` clauses are not yet accepted by the parser
   (slice 3b). For now, hoist the predicate to an outer
   `WHERE all(...)` over the list-promoted bindings:

   ```cypher
   -- Cypher 25 (queue for slice 3b)
   MATCH (a)( (x)-[:T]->(y) WHERE x.age > y.age ){1,5}(b)
   RETURN a, b

   -- Workaround on Nexus 1.15.0
   MATCH (a)( (x)-[:T]->(y) ){1,5}(b)
   WHERE all(idx IN range(0, length(x) - 1)
             WHERE x[idx].age > y[idx].age)
   RETURN a, b
   ```

4. `shortestPath((a)( ... ){m,n}(b))` works for the
   anonymous-body shape via the slice-1 lowering. Named-body
   `shortestPath` is slice 3b — for now, drop the
   `shortestPath` wrapper and add `LIMIT 1` after the QPP if a
   single shortest match is enough.

## Implementation notes

Two execution paths cover the QPP surface:

1. **Slice-1 lowering** (anonymous body) — at parse time
   `QuantifiedGroup::try_lower_to_var_length_rel` in
   `crates/nexus-core/src/executor/parser/ast.rs` collapses
   the textbook `( ()-[:T]->() ){m,n}` shape to a
   `RelationshipPattern` with quantifier. Every downstream
   consumer (planner, projection, EXISTS subqueries, …) sees
   the legacy form and the existing `VariableLengthPath`
   operator handles execution.

2. **Slice-2/3a `QuantifiedExpand` operator** (named/labelled
   inner nodes, multi-hop bodies) — anything the lowering
   rejects survives as `PatternElement::QuantifiedGroup` and
   the planner's `add_relationship_operators` calls
   `build_quantified_expand_operator` (in
   `crates/nexus-core/src/executor/planner/queries.rs`). The
   operator (in
   `crates/nexus-core/src/executor/operators/quantified_expand.rs`)
   carries `hops: Vec<QppHopSpec>` and
   `inner_nodes: Vec<QppNodeSpec>`; BFS hops the body once per
   iteration via `qpp_walk_body` and emits list-promoted
   bindings via `qpp_path_rels_as_value_list` +
   per-position node accumulation.

Test surface (slices 1 + 2 + 3a, total 26 tests):

- 14 parser unit tests in
  `crates/nexus-core/src/executor/parser/tests.rs::qpp_*`
- 7 executor integration tests in
  `crates/nexus-core/tests/executor_comprehensive_test.rs::test_qpp_*`
- 5 planner-shape tests in
  `crates/nexus-core/src/executor/planner/tests.rs::test_plan_qpp_*`

Performance benchmarks live in
`crates/nexus-core/benches/qpp_benchmark.rs` — three Criterion
groups (`qpp/legacy_var_length`, `qpp/anonymous_body`,
`qpp/named_body`) on a 50-node `:KNOWS` chain plus
`qpp/dense_fanout` on a `FANOUT=3, DEPTH=6` `:TreeNode` tree.
Run via
`cargo +nightly bench -p nexus-core --bench qpp_benchmark`.
