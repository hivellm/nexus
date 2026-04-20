# Proposal: Advanced Types (Byte Arrays, Dynamic Labels, Composite Indexes, Typed Lists, Savepoints)

## Why

A group of small-but-load-bearing Cypher-25/GQL surfaces that each
individually need a few days of work, but together close the long
tail of openCypher incompatibility reports:

1. **Byte array type (`BYTES`)** — GQL and Cypher 25 standardise a
   binary blob type. Needed by any application that wants to store
   hashes, signatures, small images, or encrypted payloads without
   base64-encoding to STRING. The wire format (JSON) needs a
   convention (base64 in responses); internally it's a length-prefixed
   byte slice.
2. **Dynamic labels on writes** — `CREATE (n:$label)`, `SET n:$label`,
   `REMOVE n:$label`. Read-side dynamic labels ship in the quickwins
   task; write-side is more invasive because it touches the label
   index and MVCC write paths.
3. **Composite indexes** — `CREATE INDEX FOR (n:Label) ON (n.p1, n.p2)`.
   The NODE KEY constraint (shipped in constraint-enforcement) needs
   this under the hood, but composite indexes are also user-visible DDL.
4. **Typed collections** — `LIST<INTEGER>`, `LIST<STRING>` as
   first-class property types. Needed for compact storage of
   homogeneous lists and for the type-constraint surface.
5. **Transaction savepoints** — `SAVEPOINT name` / `ROLLBACK TO
   SAVEPOINT name`. Not in stock Cypher but required by every SQL-ish
   migration tool that treats Nexus as a target.
6. **Graph composition primitives** — `GRAPH[name]`, explicit
   single-graph scoping when a query could legitimately touch multiple
   graphs. This lays groundwork for Cypher 25's multi-graph queries
   without committing to full GQL multi-graph semantics.

Together these raise parity from ~80% (after the prior tasks) to
~95%+ on the openCypher/Cypher 25 surface.

## What Changes

### Byte arrays
- New `Value::Bytes(Arc<[u8]>)` variant.
- Property-chain encoder gets a `TYPE_BYTES` tag with `len:u32` prefix.
- JSON serialisation: `{"_bytes": "<base64>"}` (Neo4j/GQL convention).
- Functions: `bytes(str)`, `bytes.fromBase64(str)`, `bytes.toHex(b)`,
  `bytes.length(b)`.

### Dynamic labels on writes
- Parser: accept `$param` in label positions of `CREATE`, `MERGE`,
  `SET`, `REMOVE`.
- Writer: resolve the parameter to a STRING (or LIST<STRING> for
  multi-label) at execution time; look up label IDs through the
  existing catalogue; update the label bitmap.
- Type check: parameter must be STRING or LIST<STRING>, non-NULL,
  non-empty.

### Composite indexes
- DDL: `CREATE INDEX FOR (n:L) ON (n.p1, n.p2[, ...])`.
- New index backend `CompositeBtree` keyed by
  `(label_bits, tuple(p1, p2, ...))`.
- Planner recognises predicates on composite keys and seeks.

### Typed collections
- DDL: `REQUIRE n.p IS :: LIST<INTEGER>` (extends the property-type
  constraint surface).
- Storage: typed lists encoded with a 1-byte element-type tag in the
  list header; scalar elements stored inline without per-element tags.

### Transaction savepoints
- New SQL-style statements `SAVEPOINT name`, `ROLLBACK TO SAVEPOINT
  name`, `RELEASE SAVEPOINT name`.
- MVCC engine gains a nested-journal stack per transaction. Savepoint
  rollback replays the stack backwards to the named marker.

### Graph composition
- New keyword `GRAPH[<name>]` in a leading position constrains the
  query to that graph.
- Today every query operates on the current session database; this
  adds an explicit scoping mechanism.
- Multi-graph joins across graphs are OUT OF SCOPE; only single-graph
  explicit scoping is implemented.

**BREAKING**: introducing a new `Value` variant is a kernel API
change that breaks anyone depending on an exhaustive match on the
`Value` enum in downstream Rust code. Mitigated with
`#[non_exhaustive]` on `Value`.

## Impact

### Affected Specs

- NEW capability: `types-bytes`
- NEW capability: `cypher-dynamic-labels-write`
- NEW capability: `index-composite-btree`
- NEW capability: `types-typed-collections`
- NEW capability: `transactions-savepoints`
- NEW capability: `cypher-graph-scoping`

### Affected Code

- `nexus-core/src/types/value.rs` (~60 lines modified, new variant)
- `nexus-core/src/storage/property_chain.rs` (~150 lines, bytes encoding)
- `nexus-core/src/executor/eval/functions.rs` (~120 lines, bytes funcs)
- `nexus-core/src/executor/parser/clauses.rs` (~180 lines, dynamic labels)
- `nexus-core/src/executor/operators/write.rs` (~220 lines, dynamic labels)
- `nexus-core/src/index/composite_btree.rs` (NEW, ~700 lines)
- `nexus-core/src/executor/parser/ddl.rs` (~150 lines, composite DDL)
- `nexus-core/src/transaction/savepoint.rs` (NEW, ~400 lines)
- `nexus-core/src/executor/parser/tx.rs` (NEW, ~100 lines)
- `nexus-core/src/executor/parser/graph_scope.rs` (NEW, ~80 lines)
- `nexus-core/tests/advanced_types_tck.rs` (NEW, ~1400 lines)

### Dependencies

- Requires: `phase6_opencypher-quickwins` (read-side dynamic labels).
- Requires: `phase6_opencypher-constraint-enforcement` (composite
  indexes unlock NODE KEY's real implementation).
- Requires: `phase6_opencypher-system-procedures` (new index types
  surfaced via `db.indexes()`).

### Timeline

- **Duration**: 4–6 weeks (three mini-milestones)
- **Complexity**: Medium–High — five distinct subsystems, each with
  its own correctness surface.
- **Risk**: Medium — savepoints touch MVCC journaling; value-enum
  change has repo-wide ripple.
