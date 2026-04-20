# Implementation Tasks — Constraint Enforcement

## 1. Constraint Engine Scaffolding

- [ ] 1.1 Create `nexus-core/src/constraints/mod.rs` with `ConstraintEngine`
- [ ] 1.2 Define `Constraint` enum covering all supported kinds
- [ ] 1.3 Persist constraint metadata in LMDB
- [ ] 1.4 Register engine instance per database (multi-db scoping)
- [ ] 1.5 Unit tests for engine construction + registration

## 2. Pre-Commit Hook

- [ ] 2.1 Add `check_pre_commit(tx)` called before `commit()` by every write op
- [ ] 2.2 Wire hook into `Create`, `Merge`, `SetProperty`, `SetLabels`, `RemoveLabels`, `Delete`
- [ ] 2.3 Short-circuit on first violation, return structured error
- [ ] 2.4 Ensure hook runs under the tx's MVCC snapshot (not dirty)
- [ ] 2.5 Tests: violations are rolled back atomically

## 3. UNIQUE Constraint Refactor

- [ ] 3.1 Move existing uniqueness logic into `constraints/unique.rs`
- [ ] 3.2 Add backfill validator for creation on non-empty datasets
- [ ] 3.3 Emit up to 100 example violating rows in creation errors
- [ ] 3.4 Regression tests: 100% backward-compatible behaviour

## 4. NOT NULL (Node Property Existence)

- [ ] 4.1 Parse `REQUIRE n.p IS NOT NULL`
- [ ] 4.2 Implement `constraints/not_null.rs` enforcement
- [ ] 4.3 Backfill validator: scan label, reject if any node lacks property
- [ ] 4.4 Tests: create/merge/set/remove violations detected

## 5. NODE KEY

- [ ] 5.1 Parse `REQUIRE (n.p1, n.p2) IS NODE KEY`
- [ ] 5.2 Implement as composite uniqueness + NOT NULL on each property
- [ ] 5.3 Backed by a composite-key index (covered by advanced-types task)
- [ ] 5.4 Backfill validator + tests

## 6. Relationship Constraints

- [ ] 6.1 Parse `REQUIRE r.p IS NOT NULL` in the relationship form
- [ ] 6.2 Enforce on `CREATE (a)-[r:T {...}]->(b)` and `MERGE`
- [ ] 6.3 Enforce on `SET r.p = NULL` (raises violation)
- [ ] 6.4 Tests

## 7. Property-Type Constraints (Cypher 25)

- [ ] 7.1 Parse `REQUIRE n.p IS :: INTEGER` (also FLOAT, STRING, BOOLEAN, LIST)
- [ ] 7.2 Enforce type on writes
- [ ] 7.3 Reject `SET n.p = "text"` when type is INTEGER
- [ ] 7.4 Tests

## 8. Backfill Validator

- [ ] 8.1 Streaming scan of existing rows (chunks of 10k)
- [ ] 8.2 Report up to 100 offending rows in the error payload
- [ ] 8.3 Abort CREATE CONSTRAINT atomically on violation
- [ ] 8.4 `db.awaitIndex(name)` analogue for constraints
- [ ] 8.5 Tests with non-empty datasets

## 9. Error Reporting

- [ ] 9.1 Single error code family `ERR_CONSTRAINT_VIOLATED`
- [ ] 9.2 Structured payload: constraint name, kind, offending IDs, values
- [ ] 9.3 Map to HTTP 409 Conflict at the REST layer
- [ ] 9.4 SDK tests asserting shape of error

## 10. Compatibility Flag

- [ ] 10.1 Add `relaxed_constraint_enforcement: bool = false` to server config
- [ ] 10.2 When true, violations log a warning but do not reject the write
- [ ] 10.3 Emit startup warning when flag is enabled
- [ ] 10.4 Mark flag scheduled for removal at v1.5 in CHANGELOG

## 11. openCypher TCK + Diff

- [ ] 11.1 Import TCK constraint scenarios (~60)
- [ ] 11.2 Extend Neo4j diff harness with all constraint kinds
- [ ] 11.3 Confirm 300/300 existing diff tests green

## 12. Tail (mandatory — enforced by rulebook v5.3.0)

- [ ] 12.1 Update `docs/specs/cypher-subset.md` with full constraint grammar
- [ ] 12.2 Add `docs/guides/CONSTRAINTS.md`
- [ ] 12.3 Update `docs/compatibility/NEO4J_COMPATIBILITY_REPORT.md`
- [ ] 12.4 Add CHANGELOG entry "Added enforcement for all constraint kinds" with breaking-change note
- [ ] 12.5 Update or create documentation covering the implementation
- [ ] 12.6 Write tests covering the new behavior
- [ ] 12.7 Run tests and confirm they pass
- [ ] 12.8 Quality pipeline: fmt + clippy + ≥95% coverage
