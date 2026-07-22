# Cypher planner's no-pattern branch must independently re-apply ORDER BY / SKIP / LIMIT

**Category**: cypher-planner
**Tags**: executor, planner, order-by, skip, limit, call-procedure, yield

## Description

`QueryPlanner::plan_query` (`crates/nexus-core/src/executor/planner/queries/planner_core.rs`) collects `ORDER BY` into a local `order_by_clause: Option<(Vec<String>, Vec<bool>)>` and `LIMIT`/`SKIP` into `limit_count`/`skip_count` while walking `query.clauses`, but those locals are only turned into `Operator::Sort`/`Operator::Skip`/`Operator::Limit` by `plan_execution_strategy`, which the top-level function calls ONLY `if !patterns.is_empty()` — i.e. only when the query has at least one `MATCH`/`MERGE` pattern. Any query shape with no pattern (a bare `CALL proc() YIELD col RETURN col ORDER BY col`, a standalone `CALL proc() YIELD col ORDER BY col` with no RETURN, or any other no-MATCH `RETURN`/`UNWIND ... RETURN`) falls into a SEPARATE "no-pattern" branch further down (`if patterns.is_empty() && (!return_items.is_empty() || ...) { ... } else if operators.iter().any(CallProcedure) { ... }`) that independently builds `Project`/`Aggregate` + `Limit`. Before this fix that branch never consumed `order_by_clause`, so ORDER BY was silently dropped for every CALL/YIELD query — a set-content-correct but silently-unordered result that's easy to miss in ad hoc testing (small result sets often look "sorted enough" by luck) and was only caught by seeding data in explicitly non-alphabetical insertion order.

Root cause file:line: `crates/nexus-core/src/executor/planner/queries/planner_core.rs` — `order_by_clause` captured at (then) line 522, only consumed inside `plan_execution_strategy` (called only when `!patterns.is_empty()`, line ~618); the no-pattern branches (~802 and ~1243) built `Limit` but never `Sort`. `SKIP` (`Clause::Skip`) had NO handling anywhere in the planner at all — not even in the pattern-based `plan_execution_strategy` path — confirmed via a `MATCH ... ORDER BY ... SKIP 1` control test that also silently ignored SKIP. That is a separate, PRE-EXISTING, system-wide gap (not introduced by any CALL/YIELD-specific change) and was deliberately left unfixed for the MATCH/pattern path — only wired into the no-pattern branch, matching the narrower "procedure YIELD projections" scope of the task that found it.

## When to Use (i.e. what to check when touching this planner)

Whenever adding/moving a clause-collection variable (`order_by_clause`, `limit_count`, `skip_count`, `return_distinct`, etc.) in `plan_query`'s main clause loop, verify EVERY branch that can be the terminal branch for `operators` (there are at least three: `plan_execution_strategy` for pattern-based queries, the `patterns.is_empty() && !return_items.is_empty()` branch, and the `patterns.is_empty() && CallProcedure` branch) actually consumes it. A variable being *populated* in the shared loop does not mean every downstream branch *reads* it — Rust won't warn about this since the variable IS used somewhere.

## When NOT to Use

Do not assume fixing ORDER BY in one no-pattern branch also fixes SKIP/LIMIT system-wide, or vice versa — each clause needs its own audit per branch. Do not silently extend a narrowly-scoped fix (e.g. "procedure YIELD projections") to the MATCH/pattern path without a separate, explicitly-scoped task — the blast radius (aggregation output slicing, UNION post-processing, WITH pipelines) is much larger and deserves its own root-cause + test pass.
