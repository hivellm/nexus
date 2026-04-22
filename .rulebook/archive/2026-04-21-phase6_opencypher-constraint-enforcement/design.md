# Constraint Enforcement — Technical Design

## Scope

Make every constraint kind advertised through DDL actually enforced on
the write path. Close the correctness gap between "constraint exists"
and "constraint is enforced".

## Supported constraints after this task

| Kind                             | Grammar                                                   | Entity   |
|----------------------------------|-----------------------------------------------------------|----------|
| Unique                           | `REQUIRE n.p IS UNIQUE`                                   | NODE     |
| Node property existence (NOT NULL)| `REQUIRE n.p IS NOT NULL`                                | NODE     |
| Node key                         | `REQUIRE (n.p1, n.p2) IS NODE KEY`                        | NODE     |
| Relationship property existence  | `REQUIRE r.p IS NOT NULL` in `()-[r:T]-()`                | REL      |
| Property type (Cypher 25)        | `REQUIRE n.p IS :: INTEGER` (INT/FLOAT/STRING/BOOL/LIST)  | NODE/REL |

`Node key` is semantically `(UNIQUE) AND (NOT NULL on each component)`,
implemented as one composite check against a composite-key index.

## Architecture

```
          CREATE / MERGE / SET / REMOVE
                  │
                  ▼
         MvccTx::stage_mutation
                  │
                  ▼
         ConstraintEngine::check_pre_commit(tx)
                  │
           ┌──────┴──────┐
           ▼             ▼
      per-kind       short-circuit
      validators     on first error
           │
           ▼
      tx.commit() or Err(ConstraintViolation)
```

Validators run **under the tx's MVCC snapshot plus its staged
mutations**. This means uniqueness must check both the committed
state at the tx's epoch AND the tx's uncommitted writes (otherwise
two concurrent inserts of the same key would each pass their check).
Serialisation is provided by the single-writer model.

## Per-kind validators

### Unique

Already implemented; refactored to sit behind `Constraint::Unique`.
Uses the existing B-tree index as a uniqueness bloom; on insert,
lookup + range-check for collisions.

### NOT NULL (property existence)

For a constraint on `(:L).p`:

- On `CREATE` / `MERGE` creating a node with label `L`: assert the
  node has property `p` with a non-NULL value.
- On `SET n.p = NULL` where `n:L`: reject.
- On `REMOVE n.p` where `n:L`: reject.
- On `SET n:L` adding the label to an existing node: assert the node
  has the property.
- On `REMOVE n:L`: no check (label removal is always fine).

### Node key

Composite (`p1, p2, ...`) uniqueness AND per-component NOT NULL.
Backed by a new composite-key B-tree (sees advanced-types task for
the index). Check on insert/set; lookup the composite tuple.

### Relationship constraints

Same as node NOT NULL but scoped to relationships of a given type.
The existing relationship store doesn't require new columns — only
the enforcement hook.

### Property-type constraint

On every write to a constrained property, assert the value's
dynamic type matches. The check is cheap (one match statement); the
interesting part is *what* counts as a match:

- `INTEGER`: Rust `Value::Int`. FLOAT is NOT a match.
- `FLOAT`: `Value::Float`. INTEGER is NOT a match (Neo4j behaviour).
- `STRING`: `Value::String`.
- `BOOLEAN`: `Value::Bool`.
- `LIST`: `Value::List` (element type is not constrained in v1).

## Backfill validator

Creating a constraint on a non-empty database requires proving the
existing rows satisfy it before accepting the constraint. The
validator is a streaming scan over the affected label/type:

```rust
struct BackfillReport {
    offending: Vec<OffendingRow>,   // capped at 100
    total_scanned: u64,
}
```

The scan streams 10k-row pages through the validator. On violation,
record the row and continue up to the cap, then abort the CREATE
CONSTRAINT atomically with the report attached to the error.

The validator is CPU-bound; on a 10M-node dataset with a single
constraint the wall-clock target is < 30 s on one core.

## Pre-commit hook

```rust
impl ConstraintEngine {
    fn check_pre_commit(&self, tx: &MvccTx) -> Result<(), ConstraintViolation> {
        for op in tx.staged_ops() {
            for c in self.affected_constraints(op) {
                c.check(op, tx)?;   // short-circuits on first violation
            }
        }
        Ok(())
    }
}
```

`affected_constraints` indexes constraints by label/type/property so
each op checks at most `O(k)` constraints where `k` is the number of
constraints touching the modified label or property — typically 0 or 1.

## Error shape

```json
{
  "error": "ERR_CONSTRAINT_VIOLATED",
  "constraint": {
    "name": "person_email_unique",
    "kind": "UNIQUENESS",
    "entity_type": "NODE",
    "labels_or_types": ["Person"],
    "properties": ["email"]
  },
  "violating_values": {"email": "alice@example.com"},
  "violating_node_id": 42
}
```

HTTP status: **409 Conflict** for uniqueness and node-key violations;
**400 Bad Request** for NOT NULL / property-type violations (the
request is malformed, not in conflict with existing data).

## Compatibility flag

```toml
[write_path]
# Legacy behaviour: non-unique constraints log warnings instead of rejecting.
# Scheduled for removal at v1.5.
relaxed_constraint_enforcement = false
```

When `true`, the hook logs violations at `warn` level but does not
propagate them. Startup emits a loud warning. This exists only to
help users port existing data to the new enforcement in stages.

## Interaction with replication / cluster mode

- Pre-commit hook runs on the primary (or Raft leader) before
  producing the WAL entry. Followers apply WAL entries without
  re-running the hook (consistency guarantees the same result).
- Replicas / followers may still catch a violated state if the hook
  on the leader is buggy. A consistency-check job verifies replicas
  periodically.

## TCK coverage

openCypher TCK `features/constraints/*.feature` covers ~60 scenarios;
target 100% pass rate post-task.

## Out of scope

- Cluster-level constraint catalogue sync (rides on existing metadata
  replication).
- Typed LIST constraints (`LIST<INTEGER>`) — tracked in advanced-types.
- User-defined check constraints (`REQUIRE <predicate>`) — not in
  the openCypher spec at time of writing.

## Rollout

- v1.3.0 ships constraint enforcement.
- Existing databases receive a one-time migration at startup that
  re-validates each existing constraint; failures are logged with an
  offending-row report but do NOT prevent server startup (users can
  clean up and retry).
- `relaxed_constraint_enforcement = false` by default. Users who need
  a soft landing set it true for one release, then back to false.
