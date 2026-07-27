## 1. Implementation

- [x] 1.1 Widen the `Create`-insertion sink predicate in
  `crates/nexus-core/src/executor/planner/queries/planner_core.rs` (~851-857) from
  `Operator::Project { .. }` to `Operator::Project { .. } | Operator::Aggregate { .. }`
  (mirror the WITH-insertion predicate at ~793). Update the comment to explain why.
  **Done**: implemented; `Create` now precedes any Project/Aggregate sink.

## 2. Tail (docs + tests — check or waive with tailWaiver)

- [x] 2.1 Update or create documentation covering the implementation.
  DONE — CHANGELOG `[3.0.0]` `### Fixed` entry added for the silent write-drop
  (MATCH+CREATE relationship with an aggregating RETURN/WITH).
- [x] 2.2 Write tests covering the new behavior.
  New `crates/nexus-core/tests/regression/match_create_rel_aggregating_return_test.rs`
  (registered in `tests/regression/main.rs`): `RETURN count(*)`, `WITH count(*) AS c`,
  and a guard for the already-working `RETURN a.k`.
- [x] 2.3 Run tests and confirm they pass.
  New tests 3/3 pass; full `regression` group 208/0; `cargo clippy -p nexus-core
  --all-targets -- -D warnings` clean; `cargo fmt --check` clean.
