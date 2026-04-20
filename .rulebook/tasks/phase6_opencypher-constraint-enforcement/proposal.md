# Proposal: Constraint Enforcement (NOT NULL, EXISTS, NODE KEY, Relationship Constraints)

## Why

Nexus accepts constraint DDL (`CREATE CONSTRAINT`) and reports back via
`db.constraints()`, but the write path only enforces the **UNIQUE**
variant. Every other constraint type is advertised yet unenforced:

- `REQUIRE p.email IS NOT NULL` — writers can still store nodes
  without `email`.
- `REQUIRE p.email IS UNIQUE` — enforced (the one that works).
- `REQUIRE (p.id, p.tenantId) IS NODE KEY` — not validated.
- `REQUIRE r.weight IS NOT NULL` on relationships — not validated.
- Property-type constraints (`REQUIRE p.age IS ::INTEGER`) — not
  parsed, not enforced.

A database that claims to enforce a constraint but doesn't is a
correctness hazard: applications written against Neo4j rely on
`ERR_CONSTRAINT_VIOLATED` on `CREATE` of an invalid node. Nexus
silently accepts the write, so data drift is invisible until a later
reader fails.

This task closes the gap: every constraint type advertised in DDL is
enforced on the write path, and violations raise a consistent error
with the constraint name, violated columns, and violating values.

## What Changes

- New module `nexus-core/src/constraints/` with one file per
  constraint kind (`unique.rs`, `not_null.rs`, `node_key.rs`,
  `rel_not_null.rs`, `property_type.rs`).
- **Write-path hook**: every mutating operator (`Create`, `Merge`,
  `SetProperty`, `SetLabels`, `RemoveLabels`) calls
  `ConstraintEngine::check_pre_commit(tx)` before the tx commits. The
  engine short-circuits on the first violation.
- **Parser**: extend constraint DDL to cover:
  - `CREATE CONSTRAINT FOR (n:L) REQUIRE n.p IS NOT NULL`
  - `CREATE CONSTRAINT FOR ()-[r:T]-() REQUIRE r.p IS NOT NULL`
  - `CREATE CONSTRAINT FOR (n:L) REQUIRE (n.p1, n.p2) IS NODE KEY`
  - `CREATE CONSTRAINT FOR (n:L) REQUIRE n.p IS ::INTEGER` (Cypher 25)
- **Backfill validator**: when a constraint is created on a
  non-empty dataset, the creator runs a one-shot scan; if any existing
  row violates the new constraint, the CREATE fails atomically with
  a report of offending rows (capped at 100 examples).
- **`db.awaitIndex(name)` symmetric procedure** — same semantics,
  blocks until the backfill validator completes.
- **Multi-db scoping**: each constraint belongs to exactly one
  database; no cross-db enforcement.

**BREAKING**: yes-ish. Workloads that today rely on the silent
acceptance of constraint violations will start failing. We ship a
compatibility flag `relaxed_constraint_enforcement = false` (default)
but that flag is an escape hatch, not a supported mode, and is
slated for removal at v1.5.

## Impact

### Affected Specs

- NEW capability: `constraints-engine`
- NEW capability: `constraints-not-null`
- NEW capability: `constraints-node-key`
- NEW capability: `constraints-relationship`
- MODIFIED capability: `constraints-unique` (backfill validator)
- MODIFIED capability: `cypher-ddl` (expanded constraint grammar)

### Affected Code

- `nexus-core/src/constraints/mod.rs` (NEW, ~200 lines)
- `nexus-core/src/constraints/unique.rs` (~250 lines, refactored)
- `nexus-core/src/constraints/not_null.rs` (NEW, ~180 lines)
- `nexus-core/src/constraints/node_key.rs` (NEW, ~260 lines)
- `nexus-core/src/constraints/rel_constraint.rs` (NEW, ~200 lines)
- `nexus-core/src/constraints/property_type.rs` (NEW, ~150 lines)
- `nexus-core/src/constraints/backfill.rs` (NEW, ~220 lines)
- `nexus-core/src/executor/operators/write.rs` (~80 lines, pre-commit hook)
- `nexus-core/src/executor/parser/ddl.rs` (~150 lines, expanded grammar)
- `nexus-core/tests/constraints_tck.rs` (NEW, ~900 lines)

### Dependencies

- Requires: `phase6_opencypher-system-procedures` (so `db.constraints()`
  exposes the full constraint surface).
- Unblocks: data-quality workflows, APOC procedures that rely on
  constraint violations.

### Timeline

- **Duration**: 2–3 weeks
- **Complexity**: Medium — enforcement logic is straightforward;
  backfill scan at scale requires careful streaming to avoid OOM.
- **Risk**: Medium — behaviour change for existing databases.
