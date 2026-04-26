# Quantified Path Patterns (QPP)

> Status: **slice 1 shipped in 1.15.0** — anonymous-body shape only.
> Slice 2 (`QuantifiedExpand` operator with list-promoted bindings,
> named/labelled inner nodes, multi-hop bodies, intermediate
> predicates, `shortestPath(qpp)` over named bodies) is tracked in
> `phase6_opencypher-quantified-path-patterns`.

Quantified Path Patterns are the marquee feature of Cypher 25 (the
GQL-aligned release). Where the legacy `*m..n` shorthand quantifies
a single relationship, QPP quantifies a whole parenthesised path
fragment, so traversals that previously required `UNWIND` +
recursion fold into one pattern expression.

## What works today (slice 1)

The shape this guide calls **anonymous-body QPP**:

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
level of nesting and slice 2 will lift this.

## What does **not** work yet (slice 2)

Anything that needs the dedicated `QuantifiedExpand` operator
returns a clean `ERR_QPP_NOT_IMPLEMENTED` error rather than
silently producing wrong rows. The list:

- **Named inner boundary nodes**:
  `MATCH (a)( (m)-[:T]->() ){1,5}(b)`. The `m` variable would be
  list-promoted to `LIST<NODE>` in the outer scope.
- **Labelled or property-filtered inner boundary nodes**:
  `MATCH (a)( (:Manager)-[:T]->() ){1,5}(b)` and
  `MATCH (a)( ({active: true})-[:T]->() ){1,5}(b)`.
- **Multi-hop bodies**:
  `MATCH (a)( ()-[:A]->()-[:B]->() ){1,5}(b)`.
- **Intermediate predicates inside the body**:
  `MATCH (a)( ()-[:T]->() WHERE inner_predicate ){1,5}(b)`.
- **`CREATE` with QPP**: pattern construction with QPP is a
  read-only feature in Cypher 25; `CREATE` returns
  `ERR_QPP_NOT_IN_CREATE`.

For all of these cases, write the equivalent legacy `*m..n` form
or break the QPP into multiple `MATCH` clauses. The slice-2
operator will land before 1.16.0 and lift every restriction in
this list.

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

1. Anonymous-body shapes work today. Bring them across as-is.
2. For named/labelled inner nodes, keep the legacy `*m..n` form
   and a follow-up `WHERE` clause until slice 2 ships, e.g.

   ```cypher
   -- Cypher 25 (queue for slice 2)
   MATCH (a)( (m:Manager)-[:REPORTS_TO]->() ){1,5}(b) RETURN a, m, b

   -- Workaround on Nexus 1.15.0
   MATCH (a)-[:REPORTS_TO*1..5]->(b)
   WITH a, b, [n IN nodes(p) WHERE 'Manager' IN labels(n) | n] AS m
   RETURN a, m, b
   ```

3. `shortestPath((a)( ... ){m,n}(b))` works for the
   anonymous-body shape.

## Implementation notes

Lowering lives in
`crates/nexus-core/src/executor/parser/ast.rs` as
`QuantifiedGroup::try_lower_to_var_length_rel`. The parser invokes
it in `crates/nexus-core/src/executor/parser/clauses.rs::parse_pattern`
right after a successful `try_parse_qpp_group`. Anything the
helper rejects survives as `PatternElement::QuantifiedGroup` and
the planner's `add_relationship_operators` returns
`ERR_QPP_NOT_IMPLEMENTED`. Tests live in
`crates/nexus-core/src/executor/parser/tests.rs::qpp_*` (parser /
lowering unit tests, 14 of them) and
`crates/nexus-core/tests/executor_comprehensive_test.rs::test_qpp_*`
(end-to-end parity tests against the legacy operator).
