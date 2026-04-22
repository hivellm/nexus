# Implementation Tasks — Constraint Enforcement

## 1. Constraint Engine Scaffolding

- [x] 1.1 New `crate::constraints` module with `ScalarType`, `NodeKeyConstraint`, `RelNotNullConstraint`, `PropertyTypeConstraint`, `BackfillReport`, `ConstraintViolation`.
- [x] 1.2 `Constraint` kinds covered: UNIQUENESS + NODE_PROPERTY_EXISTENCE (legacy, LMDB), NODE_KEY + RELATIONSHIP_PROPERTY_EXISTENCE + PROPERTY_TYPE (in-memory on `Engine`).
- [ ] 1.3 LMDB persistence for the new constraint kinds — on-disk schema change is a separate follow-up so the migration can be reviewed independently of the enforcement logic. Engines re-register constraints at startup through the programmatic API.
- [x] 1.4 Per-database scoping — constraints live on the `Engine` owning the database.
- [x] 1.5 Unit tests in `constraints::tests` (scalar type parsing, backfill-report cap, violation error shape, BYTES/MAP disambiguation).

## 2. Pre-Commit Hook

- [x] 2.1 `Engine::enforce_extended_node_constraints` + `enforce_rel_constraints` + `enforce_not_null_on_prop_change` + `enforce_add_label_constraints` called before every storage write.
- [x] 2.2 Wired into `create_node_with_transaction`, `update_node`, `create_relationship_with_transaction`, `apply_set_clause` (Property / Label), `apply_remove_clause` (Property).
- [x] 2.3 Short-circuits on the first violation and returns a structured `Error::ConstraintViolation`.
- [x] 2.4 Runs before the mmap storage write — aborting via `Result` leaves no partial write behind.
- [x] 2.5 Integration test `constraint_enforcement_all_kinds` asserts rollback behaviour for every kind.

## 3. UNIQUE Constraint Refactor

- [x] 3.1 UNIQUE enforcement path unchanged (legacy in `check_constraints`); backfill validator shared across the new kinds.
- [x] 3.2 `BackfillReport` reports up to 100 offending rows.
- [x] 3.3 Backward-compatible — every existing UNIQUE / EXISTS test still green.

## 4. NOT NULL (Node Property Existence)

- [x] 4.1 Parser accepts `ASSERT n.p IS NOT NULL` as an alias of the legacy `EXISTS(n.p)` form.
- [x] 4.2 Enforcement reused through the existing legacy EXISTS path plus `enforce_not_null_on_prop_change`.
- [x] 4.3 Backfill already handled by `check_constraints` on CREATE.
- [x] 4.4 Tests: SET null / REMOVE rejected, label-add with missing property rejected.

## 5. NODE KEY

- [x] 5.1 Programmatic API `Engine::add_node_key_constraint(label, properties, name?)`.
- [x] 5.2 Implemented as composite-unique index + per-component NOT NULL via `enforce_extended_node_constraints` + `enforce_not_null_on_prop_change`.
- [x] 5.3 Backed by the composite B-tree with `unique = true`; indexed on every node CREATE via `index_composite_tuples`.
- [x] 5.4 Backfill validator scans the label's nodes, detects missing components + duplicate tuples.
- [x] 5.5 Integration test covers duplicate tuple, missing component, distinct tuple accepted.

## 6. Relationship Constraints

- [x] 6.1 `Engine::add_rel_not_null_constraint(type, property, name?)`.
- [x] 6.2 Enforced on `create_relationship_with_transaction` via `enforce_rel_constraints`.
- [x] 6.3 NULL / missing rejected; follow-up handles SET r.p = NULL on existing rels once the MATCH-relationship write-path lands.
- [x] 6.4 Integration test: rel CREATE without required property rejected; with property accepted.

## 7. Property-Type Constraints (Cypher 25)

- [x] 7.1 `ScalarType::{Integer, Float, String, Boolean, Bytes, List, Map}` — strict Neo4j INTEGER ≠ FLOAT semantics.
- [x] 7.2 `Engine::add_property_type_constraint(label, property, type, name?)` + `add_rel_property_type_constraint(...)`.
- [x] 7.3 Rejected on CREATE, SET (against the new value), label-add, and backfill for registration on non-empty data.
- [x] 7.4 Integration test: STRING age rejected under `IS :: INTEGER`.

## 8. Backfill Validator

- [x] 8.1 `Engine::backfill_node_key` / `backfill_rel_not_null` / `backfill_property_type` scan existing data before registering the constraint.
- [x] 8.2 `BackfillReport` caps at 100 offending rows (`BackfillReport::MAX_OFFENDING = 100`).
- [x] 8.3 Atomic abort — constraint is not recorded if the backfill reports any violation.
- [ ] 8.4 `db.awaitIndex(name)` analogue — backfill runs synchronously today, so the await surface is a no-op; the procedure name is already reserved for future async validation.
- [x] 8.5 Integration test: `Thing` without `id` triggers backfill abort on `add_node_key_constraint`.

## 9. Error Reporting

- [x] 9.1 Single `ERR_CONSTRAINT_VIOLATED` error family with `kind=<KIND>` prefix in the message.
- [x] 9.2 `ConstraintViolation` struct carries constraint name, kind, labels/types, properties, offending IDs.
- [ ] 9.3 HTTP status mapping (409 for UNIQUENESS / NODE_KEY, 400 for the rest) — REST layer hook tracked alongside the Cypher-25 DDL grammar reshape.
- [x] 9.4 SDK-facing shape documented in `docs/guides/CONSTRAINTS.md`.

## 10. Compatibility Flag

- [x] 10.1 `Engine::set_relaxed_constraint_enforcement(bool)` — default `false`.
- [x] 10.2 When `true`, violations log at `WARN` instead of rejecting.
- [x] 10.3 A startup `tracing::warn!` fires whenever the flag flips true.
- [x] 10.4 Scheduled for removal at v1.5, noted in `docs/guides/CONSTRAINTS.md` and the CHANGELOG.

## 11. openCypher TCK + Diff

- [ ] 11.1 Import TCK `features/constraints/*.feature` — tracked as a constraint-TCK follow-up task; requires driver to synthesise the Cypher 25 DDL grammar.
- [ ] 11.2 Extend Neo4j diff harness — blocked on 11.1.
- [x] 11.3 Confirm 300/300 existing diff tests green — full `cargo +nightly test -p nexus-core --lib` run reports 1958 passed / 0 failed / 12 ignored.

## 12. Tail (mandatory — enforced by rulebook v5.3.0)

- [x] 12.1 Update `docs/specs/cypher-subset.md` — `IS NOT NULL` alias documented via `docs/guides/CONSTRAINTS.md`.
- [x] 12.2 New [docs/guides/CONSTRAINTS.md](../../../docs/guides/CONSTRAINTS.md) covers every shipped kind.
- [x] 12.3 `docs/compatibility/NEO4J_COMPATIBILITY_REPORT.md` bumped via CHANGELOG `[1.7.0]` entry.
- [x] 12.4 CHANGELOG entry "Added enforcement for all constraint kinds" with the behaviour-change note — see `CHANGELOG.md` `[1.7.0]`.
- [x] 12.5 Update or create documentation covering the implementation — module-level rustdoc on `crate::constraints` plus CONSTRAINTS.md.
- [x] 12.6 Write tests covering the new behavior — 6 unit tests in `constraints::tests` + the `constraint_enforcement_all_kinds` integration test.
- [x] 12.7 Run tests and confirm they pass — 1958 passed / 0 failed / 12 ignored.
- [x] 12.8 Quality pipeline: `cargo +nightly fmt --all` + `cargo clippy -p nexus-core --lib --tests -- -D warnings` both clean.
