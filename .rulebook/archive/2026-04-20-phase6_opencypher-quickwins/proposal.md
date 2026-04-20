# Proposal: openCypher Quick Wins (Type Checks, List Converters, Dynamic Access, `SET +=`)

## Why

Eight small, independent gaps in the Cypher surface area account for a
disproportionate share of openCypher incompatibility reports. Individually
each is a few days of work; together they raise Cypher parity from ~55% to
~65% with low architectural risk. They are grouped into a single task
because they all touch the same three modules (`executor/eval`,
`executor/parser`, `executor/operators/write`) and can be validated by one
shared TCK test harness.

Specifically these are:

1. **Type-checking predicates** — `isInteger`, `isFloat`, `isString`,
   `isBoolean`, `isList`, `isMap`, `isNode`, `isRelationship`, `isPath`.
   Neo4j/openCypher expose these as scalar functions returning `BOOLEAN`;
   Nexus currently has none of them.
2. **List type converters** — `toIntegerList`, `toFloatList`,
   `toStringList`, `toBooleanList`. Bulk conversion of `List<T>` is common
   when ingesting CSV rows or reshaping aggregation output. Nexus exposes
   the scalar converters but not the list variants.
3. **`isEmpty` predicate** — Must work on `List<T>`, `Map<K,V>`, and
   `String`. Today callers must write `size(x) = 0` which is readable but
   non-standard.
4. **Dynamic property access `n[expr]`** — Reading and writing a property
   whose key is a runtime expression (most commonly `$param`). Parser
   already tokenises `[` and `]`, but the AST rejects the form in
   property-reference position.
5. **`SET += {...}` map merge** — Merges a map into a node/relationship's
   existing properties without removing keys absent from the right-hand
   side. Distinct from `SET = {...}` which replaces the entire property
   bag.
6. **`properties(exists)` behaviour on missing properties** — Today
   `properties(n)` returns the map, but callers want an `EXISTS` variant
   that distinguishes "absent" from "present but NULL". We add
   `exists(n.prop)` returning `BOOLEAN` (different semantics from the
   pattern predicate `EXISTS { ... }`).
7. **`left(str, n)` / `right(str, n)`** — Prefix/suffix extraction.
   `substring` covers both but the dedicated builtins are required for
   openCypher parity.
8. **Dynamic label expression in WHERE (read-only)** — `WHERE
   any(l IN labels(n) WHERE l = $x)` already works; the shortcut `n:$x`
   does not. Scope of this task: **read-only** dynamic labels in `WHERE`
   and `RETURN`. Write-side `CREATE (:$x)` is out of scope (see
   `phase6_opencypher-advanced-types`).

## What Changes

- Add 9 type-checking predicate functions to `executor/eval/functions.rs`.
- Add 4 list type-converter functions to `executor/eval/functions.rs`.
- Add `isEmpty`, `left`, `right`, `exists(prop)` to the scalar registry.
- Parser: allow `expr '[' expr ']'` as a property reference; allow
  `':' '$' ident` in label-predicate positions in `WHERE`/`RETURN`.
- AST: new `AccessKind::Dynamic(Expr)` variant for `PropertyAccess`.
- Evaluator: new `set_property_dynamic` path that coerces the key to
  `String` at runtime; raises `ERR_INVALID_KEY` on non-string.
- Writer: new operator `SetPropertyMapMerge` that implements `SET n += m`
  semantics (merge, not replace).
- Parser: new `SetItem::MapMerge(lhs, map_expr)` in `executor/parser/ast.rs`.

**BREAKING**: none. Every addition is net-new syntax/function; existing
queries continue to parse and execute identically.

## Impact

### Affected Specs

- NEW capability spec: `cypher-type-predicates`
- NEW capability spec: `cypher-list-converters`
- NEW capability spec: `cypher-dynamic-property-access`
- MODIFIED capability spec: `cypher-write-clauses` (adds `SET +=`)

### Affected Code

- `nexus-core/src/executor/eval/functions.rs` (~350 lines added)
- `nexus-core/src/executor/parser/expression.rs` (~120 lines added)
- `nexus-core/src/executor/parser/clauses.rs` (~40 lines added, SET +=)
- `nexus-core/src/executor/parser/ast.rs` (~20 lines added, new variants)
- `nexus-core/src/executor/operators/write.rs` (~150 lines added)
- `nexus-core/src/executor/eval/access.rs` (~80 lines added, dynamic path)
- `nexus-core/tests/quickwins_tck.rs` (NEW, ~600 lines, TCK-style coverage)

### Dependencies

- Requires: none (all additions are self-contained in executor)
- Unblocks: `phase6_opencypher-apoc-ecosystem` (APOC relies heavily on
  `toIntegerList`, `toStringList`, and dynamic property access).

### Timeline

- **Duration**: 2–3 weeks
- **Complexity**: Low
- **Risk**: Low — additive changes only, no executor re-architecture
