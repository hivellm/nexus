# Proposal: phase6_opencypher-tck-spatial

Source: carved out from `phase6_spatial-planner-followups` §2 — that
parent task already shipped function-style `point.nearest` (§1) and
the +25 Neo4j compat-diff scenarios (§3); the TCK import is the
remaining slice and lives here so the parent can archive cleanly.

## Why

`phase6_opencypher-geospatial-predicates` and
`phase6_spatial-planner-seek` together delivered the spatial Cypher
surface (`point.*` predicates, `spatial.*` procedures, `SpatialSeek`
planner), but Nexus has no machine-checked alignment with the
openCypher TCK's `spatial.feature` corpus. Without that import, every
edge case the upstream TCK encodes is invisible to CI; regressions
land silently and parity drift accumulates.

Vendoring the TCK is operator-gated work that the previous follow-up
task could not execute end-to-end:

1. **Network access**: the openCypher reference distribution lives at
   `https://github.com/opencypher/openCypher` and must be fetched as
   a tarball at a pinned upstream commit; the implementing agent ran
   inside a sandbox without outbound internet.
2. **Dependency-tree change**: parsing Gherkin scenarios needs a
   crate (`cucumber 0.21` or a hand-written subset). Adding it
   touches `Cargo.toml`, `Cargo.lock`, and the workspace feature
   matrix; the previous follow-up's scope was bounded to authoring
   scenarios already executable through existing harnesses.
3. **License header reconciliation**: the openCypher distribution
   ships under Apache 2.0 with NOTICE-file requirements; vendoring
   requires updating `LICENSE-NOTICE.md` (or equivalent) and adding
   a `VENDOR.md` pinning the upstream commit hash.

This task carves those three concerns out so they land as a single
slice with proper review.

## What Changes

### 1. Vendor `spatial.feature` corpus

- Fetch the openCypher distribution at a pinned commit and extract
  every `*.feature` file under `tck/features/clauses/return/Return*spatial*`
  (or wherever upstream parks the spatial scenarios — confirm the
  exact path against the pinned commit).
- Land them under `crates/nexus-core/tests/tck/spatial/` with a
  `VENDOR.md` recording the upstream commit hash, the date of
  vendoring, and the SHA-256 of every file so future bumps are
  reproducible.
- Update `LICENSE-NOTICE.md` (or create it) with the openCypher
  Apache 2.0 attribution.

### 2. Cucumber harness

- Add `cucumber = "0.21"` to the workspace `[dev-dependencies]`
  block. The features the parser needs (`gherkin`, `tracing`) are
  already on by default at that version.
- Implement `crates/nexus-core/tests/tck_runner.rs` that:
  - Discovers `.feature` files under the `tck/spatial/` directory.
  - Maps the standard openCypher `Given a graph "<name>"` /
    `When executing query: ...` / `Then the result should be ...`
    steps onto `Engine::execute_cypher`.
  - Surfaces failures with `--fail-fast` semantics so CI gets a
    fast bail.
- Wire the runner into `cargo test -p nexus-core --test tck_runner`.

### 3. Fix every failing scenario

- Run the suite; for each failing scenario:
  - Triage as a Cypher coverage bug (file an unblocking task) OR
    an implementation bug (fix on the spot).
- Target: 0 failing scenarios at archive time.

## Impact

- **Affected specs**: NEW
  `crates/nexus-core/tests/tck/spatial/VENDOR.md` (vendor pin);
  MODIFIED `docs/compatibility/NEO4J_COMPATIBILITY_REPORT.md`
  (TCK row); MODIFIED `LICENSE-NOTICE.md` (Apache 2.0 attribution).
- **Affected code**: NEW `crates/nexus-core/tests/tck_runner.rs`;
  MODIFIED `Cargo.toml` (`cucumber` dev-dep);
  potentially patches across `crates/nexus-core/src/executor/`
  if any TCK scenario surfaces a real bug.
- **Breaking change**: NO — pure test surface addition.
- **User benefit**: machine-checked openCypher parity for the
  spatial Cypher surface; regressions land in CI instead of in
  user reports.
- **Dependencies**: parent `phase6_spatial-planner-followups`
  already archived (or about to); this task is independent of
  the function-style `point.nearest` and Neo4j-diff slices that
  shipped in the parent.

## Source

- Parent task: `phase6_spatial-planner-followups` (in-progress at
  carve-out time; archived once §1 + §3 land).
- Upstream openCypher distribution:
  `https://github.com/opencypher/openCypher` (commit hash to be
  pinned at vendoring time).
