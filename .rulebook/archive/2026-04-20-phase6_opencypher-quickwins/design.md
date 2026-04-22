# openCypher Quick Wins — Technical Design

## Scope

Eight low-risk additions to the Cypher surface area that individually
take days to implement but collectively raise openCypher parity from
~55% → ~65%. All changes are **additive** — no existing query semantics
change.

## Non-goals

- Write-side dynamic labels `CREATE (:$x)` — tracked in
  `phase6_opencypher-advanced-types`.
- Typed lists (`List<Integer>`) as a first-class type — tracked in
  `phase6_opencypher-advanced-types`.
- Pattern-style `EXISTS { ... }` — already implemented; this task only
  adds the scalar `exists(prop)` function.

## 1. Function registry model

Functions currently live in a single registry:

```rust
// executor/eval/functions.rs
static REGISTRY: Lazy<HashMap<&'static str, FnImpl>> = Lazy::new(|| { ... });
```

This task extends the registry with 17 new entries:

| Name              | Arity | Returns    | Category        |
|-------------------|-------|------------|-----------------|
| `isInteger`       | 1     | BOOLEAN    | type-check      |
| `isFloat`         | 1     | BOOLEAN    | type-check      |
| `isString`        | 1     | BOOLEAN    | type-check      |
| `isBoolean`       | 1     | BOOLEAN    | type-check      |
| `isList`          | 1     | BOOLEAN    | type-check      |
| `isMap`           | 1     | BOOLEAN    | type-check      |
| `isNode`          | 1     | BOOLEAN    | type-check      |
| `isRelationship`  | 1     | BOOLEAN    | type-check      |
| `isPath`          | 1     | BOOLEAN    | type-check      |
| `toIntegerList`   | 1     | LIST OF INTEGER | coercion    |
| `toFloatList`     | 1     | LIST OF FLOAT   | coercion    |
| `toStringList`    | 1     | LIST OF STRING  | coercion    |
| `toBooleanList`   | 1     | LIST OF BOOLEAN | coercion    |
| `isEmpty`         | 1     | BOOLEAN    | predicate       |
| `left`            | 2     | STRING     | string          |
| `right`           | 2     | STRING     | string          |
| `exists`          | 1     | BOOLEAN    | property-check  |

All predicates return NULL on NULL input (openCypher three-valued logic).

## 2. Dynamic property access

### Current AST

```rust
// executor/parser/ast.rs
pub struct PropertyAccess {
    pub target: Box<Expr>,
    pub key: String,              // static only
}
```

### New AST

```rust
pub enum AccessKind {
    Static(String),
    Dynamic(Box<Expr>),           // NEW
}

pub struct PropertyAccess {
    pub target: Box<Expr>,
    pub access: AccessKind,
}
```

### Parser change

The grammar already accepts `primary '.' IDENT`. Add a sibling rule
`primary '[' expr ']'`. Lookahead must distinguish:

- `arr[0]`  → list indexing (existing)
- `node[$k]` → dynamic property access (NEW)

The disambiguation rule: if the LHS at evaluation time is a NODE or
RELATIONSHIP, treat as property access; else list indexing. Today the
executor already makes this decision for `arr[expr]`, so the new path is
a straight extension.

### Runtime

```rust
fn access_property(value: &Value, access: &AccessKind) -> Result<Value> {
    match access {
        AccessKind::Static(k) => lookup(value, k),
        AccessKind::Dynamic(e) => {
            let k = eval(e)?;
            match k {
                Value::String(s) => lookup(value, &s),
                Value::Null => Ok(Value::Null),
                _ => Err(Error::InvalidKey(k.type_name())),
            }
        }
    }
}
```

## 3. `SET +=` map merge

### Semantic contrast

Given `n.props = {a: 1, b: 2}`:

| Clause             | Result                    |
|--------------------|---------------------------|
| `SET n = {c: 3}`   | `{c: 3}` (replace)        |
| `SET n += {c: 3}`  | `{a: 1, b: 2, c: 3}`      |
| `SET n += {a: 9}`  | `{a: 9, b: 2}` (overwrite matching, keep others) |

Setting a value to `NULL` in the RHS removes the key, matching Neo4j:

```cypher
SET n += {a: null}  -- removes key "a"
```

### Parser

The current grammar has:

```
set_item := lhs '=' expr
```

Extend to:

```
set_item := lhs ('=' | '+=') expr
```

and emit a distinct AST variant `SetItem::MapMerge` so the executor can
dispatch without re-parsing.

### Operator

New operator `SetPropertyMapMerge` in `executor/operators/write.rs`. It:

1. Loads the target's current property bag.
2. For each `(k, v)` in the RHS map:
   - If `v` is NULL, remove the key from the bag.
   - Else, insert/overwrite.
3. Writes the merged bag back through the existing property-chain writer.

No new WAL entry type — the operator produces the same record mutations
as a sequence of `SET lhs.k = v` updates, so WAL replay is a no-op
change.

## 4. Read-only dynamic labels

Grammar addition in `WHERE` / `RETURN` expression positions only:

```
label_predicate := var ':' ('$' ident | ident)
```

Evaluator desugars `n:$x` to `any(l IN labels(n) WHERE l = $x)`. This
keeps the write side unchanged: `CREATE (n:$x)` continues to raise a
parse error, pointing at the advanced-types task.

## 5. Error taxonomy additions

| Error code            | Raised when                                    |
|-----------------------|------------------------------------------------|
| `ERR_INVALID_KEY`     | Dynamic property key not a String or NULL      |
| `ERR_SET_NON_MAP`     | `SET n += v` where `v` is not a map or NULL    |
| `ERR_DYN_LABEL_WRITE` | `CREATE (:$x)` — pointing to advanced-types task |

All three are recoverable at the query layer (return as `400 Bad
Request` from `/cypher` with the standard error envelope).

## 6. TCK harness

openCypher publishes a Technology Compatibility Kit at
`github.com/opencypher/openCypher/tree/master/tck/features`. Only a
subset is relevant to this task; the harness selects features by tag:

- `@type-checks`
- `@list-coercion`
- `@dynamic-access`
- `@set-merge`

The harness is a new `tests/quickwins_tck.rs` that parses `.feature`
files with the `cucumber` crate and runs each scenario against an
in-process `Engine`. Features failing due to *out-of-scope* behaviour
(e.g. requiring QPP) are excluded via explicit tag filters.

## 7. Rollout

Phase 6a quick-wins ships as a single release (`v1.1.0`). No feature
flag: every addition is additive and opt-in at the query level. The
coverage matrix in `NEO4J_COMPATIBILITY_REPORT.md` is re-generated at
release time and expected to move from 55% → ~65%.

## 8. Testing matrix

| Feature             | Unit | Integration | TCK  | Neo4j diff |
|---------------------|------|-------------|------|------------|
| Type predicates     | ✓    | ✓           | ✓    | extended   |
| List converters     | ✓    | ✓           | ✓    | extended   |
| `isEmpty`           | ✓    | ✓           | ✓    | extended   |
| `left`/`right`      | ✓    | ✓           | ✓    | extended   |
| Dynamic access      | ✓    | ✓           | ✓    | extended   |
| `SET +=`            | ✓    | ✓           | ✓    | extended   |
| `exists(prop)`      | ✓    | ✓           | ✓    | extended   |
| Dynamic labels (RO) | ✓    | ✓           | ✓    | extended   |

Target: ≥95% line coverage on new modules, 100% pass on TCK-selected
scenarios, 300/300 legacy diff tests unchanged.
