# Constraints

phase6_opencypher-constraint-enforcement lands write-path enforcement
for every constraint kind Nexus models. Previously, UNIQUE was
enforced; every other variant was accepted by the DDL parser but
silently ignored on writes. That correctness gap is closed.

## Supported constraints (v1.6)

| Kind                                      | Registration                                                                        | Entity        |
|-------------------------------------------|-------------------------------------------------------------------------------------|---------------|
| UNIQUE                                    | `CREATE CONSTRAINT FOR (n:L) ASSERT n.p IS UNIQUE`                                  | NODE          |
| Node property existence (NOT NULL)        | `CREATE CONSTRAINT FOR (n:L) ASSERT n.p IS NOT NULL` (or legacy `EXISTS(n.p)`)      | NODE          |
| **NODE KEY**                              | `Engine::add_node_key_constraint(label, [p1, p2, ...], name?)`                      | NODE          |
| **Relationship NOT NULL**                 | `Engine::add_rel_not_null_constraint(type, property, name?)`                        | RELATIONSHIP  |
| **Property-type** (`IS :: INTEGER` etc.)  | `Engine::add_property_type_constraint(label, property, ScalarType, name?)`          | NODE          |
| **Property-type** (relationship)          | `Engine::add_rel_property_type_constraint(type, property, ScalarType, name?)`       | RELATIONSHIP  |

Bold kinds ship the programmatic-API form in this release; the
`FOR (n:L) REQUIRE (p1, p2) IS NODE KEY` / relationship / `IS :: T`
surface grammar extension lands in the follow-up DDL-reshape task.

## Enforcement surface

Every mutating path calls the appropriate check before the storage
write. Violations abort the transaction and raise
`ERR_CONSTRAINT_VIOLATED`:

- `CREATE (n:L {...})` — legacy `check_constraints` plus
  `enforce_extended_node_constraints` for NODE KEY + property-type.
- `CREATE (a)-[r:T {...}]->(b)` — `enforce_rel_constraints`.
- `SET n.p = expr` — property-type check against the new value;
  `enforce_not_null_on_prop_change` rejects NULL writes to
  EXISTS / NODE KEY components.
- `REMOVE n.p` — rejected when the property is EXISTS-constrained
  or a NODE KEY component.
- `SET n:L` — `enforce_add_label_constraints` rejects if the newly
  labelled node fails any constraint on `L`.

### Backfill

Registering a NODE KEY / relationship NOT NULL / property-type
constraint on a non-empty dataset runs a one-shot streaming scan
through `BackfillReport`. If any existing row violates the
constraint, the registration aborts with up to 100 offending IDs
cited; no constraint is recorded. The scan is capped at 10k-row
pages (spec §8.1) but for now runs in a single pass — the
chunking behaviour lands with the full storage-stream follow-up.

## Error shape

Every constraint violation surfaces as
`Error::ConstraintViolation` with a message of the form:

```
ERR_CONSTRAINT_VIOLATED: kind=<KIND> <details>
```

Known `kind` values:

- `UNIQUENESS`
- `NODE_PROPERTY_EXISTENCE`
- `NODE_KEY`
- `RELATIONSHIP_PROPERTY_EXISTENCE`
- `PROPERTY_TYPE`

HTTP mapping at the REST layer:

- `UNIQUENESS` and `NODE_KEY` → **409 Conflict**.
- `NODE_PROPERTY_EXISTENCE`, `RELATIONSHIP_PROPERTY_EXISTENCE`,
  `PROPERTY_TYPE` → **400 Bad Request**.

## Compatibility flag

A soft-landing escape hatch exists for users porting existing data:

```rust
engine.set_relaxed_constraint_enforcement(true);
```

When `true`, constraint violations log at `WARN` level but do not
reject the write. The server emits a prominent startup warning
whenever this flag is enabled. Flag scheduled for removal at v1.5.

## Interactions with other phase6 surfaces

- NODE KEY sits on the composite B-tree registry from
  `phase6_opencypher-advanced-types §3`. Every NODE KEY registration
  creates a UNIQUE composite index, surfaced through
  `CALL db.indexes() YIELD name`.
- Property-type `IS :: LIST<T>` shares its validator with
  `engine::typed_collections::validate_list` (same phase).
- Dynamic-label writes (`CREATE (n:$label)` / `SET n:$param`) go
  through the same enforcement pipeline — a label added via a
  parameter is subject to every constraint on the resolved name.

## Not yet shipped

- Full Cypher 25 `FOR (n:L) REQUIRE ... IS NODE KEY` / relationship
  / property-type DDL grammar — follow-up. Current grammar accepts
  `IS NOT NULL` as an alias for `EXISTS`.
- LMDB persistence of the extended constraint surface — the on-disk
  schema change is a separate follow-up so the migration can be
  reviewed independently of the enforcement logic. Engines
  re-register constraints at startup through the programmatic API.
- openCypher TCK `features/constraints/*.feature` import — tracked
  in the constraint-TCK follow-up.
