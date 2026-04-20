# Advanced Types — Technical Design

## Scope

Five related additions that raise openCypher parity from ~80% to
~95%+: byte arrays, write-side dynamic labels, composite B-tree
indexes, typed collections, transaction savepoints, and single-graph
scoping.

## 1. Byte arrays

### Value enum

```rust
#[non_exhaustive]
pub enum Value {
    Null,
    Bool(bool),
    Int(i64),
    Float(f64),
    String(Arc<str>),
    Bytes(Arc<[u8]>),          // NEW
    List(Arc<[Value]>),
    Map(Arc<Map>),
    Node(NodeRef),
    Relationship(RelRef),
    Path(Box<Path>),
    Point(Point),
    Date(Date),
    Time(Time),
    DateTime(DateTime<FixedOffset>),
    LocalTime(NaiveTime),
    LocalDateTime(NaiveDateTime),
    Duration(Duration),
}
```

`#[non_exhaustive]` protects us against downstream code that does
exhaustive matches breaking on future additions.

### Storage encoding

Property-chain entries gain a new tag:

```
TYPE_BYTES  = 0x0F
  [tag:u8=0x0F][len:u32][bytes...]
```

No cross-boundary chunking for v1; bytes > 4 KiB live in an overflow
chain (existing structure reused from long strings).

### Wire format

Responses encode bytes as `{"_bytes": "<base64>"}`. Request parameters
accept the same shape plus a convenience plain-base64 string when the
parameter's declared type is `BYTES`.

### Functions

```
bytes(str: STRING) -> BYTES                 // UTF-8 encode
bytes.fromBase64(str: STRING) -> BYTES
bytes.toBase64(b: BYTES) -> STRING
bytes.toHex(b: BYTES) -> STRING
bytes.length(b: BYTES) -> INTEGER
bytes.slice(b: BYTES, start, len) -> BYTES
```

## 2. Dynamic labels on writes

### Parser

```
label_pattern      := ':' ident ( ':' ident )*
                   |  ':' '$' ident
                   |  ':' '$' ident ( ':' '$' ident )*
```

Also allow a list-valued parameter: `CREATE (n$labels)` where
`$labels` is `LIST<STRING>`. Both forms are unified in the AST as
`DynamicLabels(Vec<LabelSource>)` where `LabelSource` is either
`Static(String)` or `Param(String)`.

### Executor

At runtime the operator resolves every `Param` label via the
parameter map, then looks up the label ID in the catalogue (inserting
if missing) and ORs it into the node's `label_bits`.

### Errors

- `ERR_INVALID_LABEL` — parameter is NULL, empty string, empty list,
  or contains non-STRING elements.
- `ERR_LABEL_LIMIT` — the 64-label bitmap cap was reached (existing).

## 3. Composite B-tree index

### Key encoding

Composite key = concatenation of per-component encodings with a
0-byte sentinel between components. Components use the same
length-prefix encoding as single-property B-tree indexes to preserve
lexicographic ordering.

### Operator `CompositeSeek`

```rust
struct CompositeSeek {
    index_id: IndexId,
    prefix: Vec<Option<Value>>,   // None = open-ended for later components
    seek_type: SeekType,          // Equality, Range, PrefixScan
}
```

Planner treats a composite index as seekable when the query's
predicates form a prefix on the index's property list. Partial
predicates degrade to prefix-scan + residual filter.

### DDL

```
CREATE INDEX [name] FOR (n:L) ON (n.p1, n.p2, ..., n.pK)
```

## 4. Typed collections

### Storage

A list value `List([Int(1), Int(2), Int(3)])` is encoded as:

```
TYPE_LIST(typed)   = 0x0B
  [tag:u8=0x0B][elem_type:u8][count:u32][elem_0:encoded]...
```

Scalar element types (Int, Float, Bool) pack inline without
per-element tags; variable-width elements (String, Bytes) include a
u32 length prefix.

Untyped lists (`TYPE_LIST_ANY = 0x0C`) keep the existing per-element
tag encoding as a fallback.

### Constraint integration

`REQUIRE n.p IS :: LIST<INTEGER>` validates every write: the value
must be a typed list with `elem_type == INTEGER` (or convertible to
one via type coercion when coercion is unambiguous).

## 5. Transaction savepoints

### Grammar

```
savepoint_stmt   := 'SAVEPOINT' ident
rollback_stmt    := 'ROLLBACK' ('TO' 'SAVEPOINT' ident)?
release_stmt     := 'RELEASE' 'SAVEPOINT' ident
```

Savepoints only make sense inside explicit transactions (`BEGIN`
statement). Issuing them in implicit-tx mode raises
`ERR_SAVEPOINT_NO_TX`.

### Journal model

MVCC already maintains an undo log per in-flight transaction. This
task adds a **savepoint stack**:

```rust
struct SavepointMarker {
    name: String,
    undo_log_offset: usize,
    staged_ops_offset: usize,
}
```

On `SAVEPOINT s`, push `{name: s, current offsets}`.
On `ROLLBACK TO SAVEPOINT s`, pop markers down to (but not including)
`s`, replay the undo log forward from `s.undo_log_offset`, truncate
staged ops to `s.staged_ops_offset`, leave `s` on the stack.
On `RELEASE SAVEPOINT s`, pop `s` without replaying.

Nested savepoints work because the stack is FIFO: releasing an inner
savepoint simply pops it; rolling back to an outer savepoint pops
all inner ones first.

### WAL interaction

Savepoints are purely in-memory until commit. At commit, the final
undo log reflects all surviving mutations; WAL-wise, a transaction
with savepoints is indistinguishable from one without.

## 6. Graph scoping

### Grammar

```
query := ('GRAPH' '[' name ']')? regular_query
```

Example:

```cypher
GRAPH[analytics] MATCH (n:Person) RETURN count(n)
```

Semantics: the entire query runs in the named database (Nexus's
multi-db unit). Today every query implicitly runs in the session's
current database; this clause overrides on a per-query basis.

### Type checking

The parser emits a `GraphScope { name }` preamble. The planner
resolves the name to a `DatabaseId` before anything else; if the
caller lacks read access (or the graph doesn't exist), the query
fails before any operator runs.

### Multi-graph queries (explicit non-goal)

Cypher 25 draft syntax `USE [g1, g2]` is NOT in scope. We only
allow a single graph per query, which already unlocks the majority
of patterns users care about.

## Rollout

Three releases:

- **v1.5.0**: bytes + dynamic labels + composite indexes.
- **v1.5.1**: typed collections.
- **v1.5.2**: savepoints + graph scoping.

Each sub-release is independently verifiable and doesn't block the
others, but all land before declaring v1.5 "openCypher parity v1".

## Error taxonomy

| Code                        | Raised when                                        |
|-----------------------------|----------------------------------------------------|
| `ERR_INVALID_LABEL`         | Dynamic label parameter bad                        |
| `ERR_LABEL_LIMIT`           | 64-label bitmap full                               |
| `ERR_SAVEPOINT_NO_TX`       | SAVEPOINT outside explicit tx                      |
| `ERR_SAVEPOINT_UNKNOWN`     | ROLLBACK/RELEASE to unknown name                   |
| `ERR_GRAPH_NOT_FOUND`       | `GRAPH[name]` references missing database          |
| `ERR_GRAPH_ACCESS_DENIED`   | Caller has no permission on the named database     |
| `ERR_BYTES_TOO_LARGE`       | BYTES value exceeds per-property 64 MiB cap        |

## Out of scope

- Full multi-graph queries (`USE [g1, g2]`).
- Typed MAP values (`MAP<STRING, INTEGER>`) beyond the already-
  shipped property-type constraint.
- User-defined types.
