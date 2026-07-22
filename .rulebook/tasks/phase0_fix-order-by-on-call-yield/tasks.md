# Tasks: phase0_fix-order-by-on-call-yield

`CALL db.labels() YIELD label RETURN label ORDER BY label` returns unsorted
rows on a live server (`B, A, C, D, E`; Neo4j sorts). Set content correct,
`ORDER BY` silently ignored on the post-YIELD projection. Full context in
`proposal.md`.

## 1. Root cause and fix
- [x] 1.1 Reproduce and map the affected shapes: CALL-YIELD `ORDER BY` on the
      engine path vs the read-only lock-free procedure path; also test
      `SKIP`/`LIMIT` after YIELD, and confirm plain `MATCH ... ORDER BY` sorts
      correctly on the same server (isolate the difference with evidence)

      Reproduced at the ENGINE level first (isolated `Executor`, no server —
      cheapest repro, as suggested): `CALL db.labels() YIELD label RETURN
      label ORDER BY label` against labels seeded in insertion order
      `B, A, C, D, E` returned `["A", "C", "E", "B", "D"]` (unsorted; set
      content correct). Since the server's lock-free read-only carve-out
      (`crates/nexus-server/src/api/cypher/execute/handler.rs:691`,
      `lock_free_executor.execute(&query)`) and the engine-write path
      (`engine.execute_cypher_with_params` at handler.rs:752) both bottom
      out in the SAME `Executor::execute` → `Executor::execute_inner` →
      `QueryPlanner::plan_query` call chain, the engine-level repro is
      sufficient to prove the defect is NOT specific to the a7e78078
      read-only routing carve-out — it is a planner bug shared by every
      caller. `SKIP` after YIELD was ALSO dropped (`CALL db.labels() YIELD
      label RETURN label ORDER BY label SKIP 1 LIMIT 2` returned `["A",
      "C"]` — LIMIT applied, SKIP silently ignored). Control:
      `MATCH (n) RETURN labels(n)[0] AS label ORDER BY label` on the SAME
      engine sorted correctly (`["A","B","C"]`), pinning the isolation to
      queries with no `MATCH`/`MERGE` pattern. A second control,
      `MATCH (n) RETURN labels(n)[0] AS label ORDER BY label SKIP 1`,
      showed SKIP is ALSO silently dropped on the pattern-based (MATCH)
      path — a separate, pre-existing, system-wide gap unrelated to the
      CALL/YIELD regression (see 1.2 scope note).

- [x] 1.2 Root-cause where the sort is dropped (parser clause attachment vs
      procedure pipeline never applying the sort operator) with file:line
      evidence, then fix so ORDER BY/SKIP/LIMIT apply to procedure YIELD
      projections

      Root cause: parser clause attachment is correct — `ORDER BY` parses
      into its own `Clause::OrderBy` regardless of what precedes it
      (`crates/nexus-core/src/executor/parser/clauses/subquery.rs:494-497`
      and the equivalent top-level clause parser). The defect is entirely
      in `QueryPlanner::plan_query`
      (`crates/nexus-core/src/executor/planner/queries/planner_core.rs`):
      the clause-walking loop collects `ORDER BY`/`LIMIT`/`SKIP` into local
      variables `order_by_clause` (was line 301, now populated at the
      `Clause::OrderBy` arm ~line 511) and `limit_count`/`skip_count`
      (`Clause::Limit`/`Clause::Skip` arms ~line 506-513), but those locals
      were previously turned into `Operator::Sort`/`Operator::Limit` ONLY
      by `plan_execution_strategy`, called at (then) line 618 `if
      !patterns.is_empty()` — i.e. only when the query has a `MATCH`/
      `MERGE` pattern. `CALL db.labels() YIELD label RETURN label ORDER BY
      label` has no `MATCH`, so `patterns` is empty; the query instead
      fell into the separate "no-pattern" branch (then lines 802-1249:
      `if patterns.is_empty() && (!return_items.is_empty() || ...) { ... }
      else if operators.iter().any(CallProcedure) { ... }`), which built
      `Project`/`Aggregate` operators plus `Operator::Limit` from
      `limit_count` — but NEVER consumed `order_by_clause`, so no
      `Operator::Sort` was ever emitted for this shape. `Clause::Skip` had
      NO handling anywhere in the planner at all before this fix (not even
      in the pattern-based `plan_execution_strategy` path — confirmed a
      pre-existing, system-wide gap via the `MATCH ... SKIP` control test
      in 1.1, out of scope for this fix; see deviations below).

      Fix (surgical, confined to the no-pattern branches — the scope this
      proposal names, "procedure YIELD projections"):
      - `crates/nexus-core/src/executor/types.rs`: added
        `Operator::Skip { count: usize }` (Limit already existed; Sort
        already existed but was unreachable from these branches).
      - `crates/nexus-core/src/executor/operators/project.rs`: added
        `execute_skip` (mirrors `execute_limit`'s materialize-then-slice
        shape; drops the first `count` rows via `Vec::drain`).
      - `crates/nexus-core/src/executor/dispatch.rs` and
        `crates/nexus-core/src/executor/operators/dispatch.rs`: wired
        `Operator::Skip` into the two exhaustive `match operator { .. }`
        dispatch sites (compiler-enforced — both are exhaustive matches
        with no wildcard arm, so the compiler caught every site that
        needed updating).
      - `crates/nexus-core/src/executor/planner/queries/cost.rs`: added a
        `Operator::Skip` arm to the (also exhaustive) cost-estimation
        match.
      - `crates/nexus-core/src/executor/planner/queries/planner_core.rs`:
        added `skip_count: Option<usize>` alongside `limit_count`, a
        `Clause::Skip` arm to populate it, and — in BOTH no-pattern
        branches — `Operator::Sort` (from `order_by_clause`) then
        `Operator::Skip` then `Operator::Limit`, in that order (standard
        openCypher `ORDER BY, SKIP, LIMIT` pipeline order).

      Deviation / scope decision on SKIP: `skip_count` is deliberately
      consumed ONLY in the two no-pattern branches, NOT threaded into
      `plan_execution_strategy` (the `MATCH`-pattern path). SKIP being
      unimplemented for `MATCH ... SKIP` is a separate, pre-existing,
      system-wide gap (confirmed via the 1.1 control test) that predates
      the CALL/YIELD routing change (a7e78078) and is unrelated to it;
      fixing it would mean threading `Operator::Skip` through
      `plan_execution_strategy`'s aggregation/UNION/WITH sub-paths — a much
      larger blast radius than "procedure YIELD projections" and outside
      this task's explicit scope (proposal.md: "Fix so ORDER BY ... applies
      to procedure YIELD projections"). Recorded as a new anti-pattern in
      `.rulebook/knowledge/anti-patterns/planner-no-pattern-branch-must-independently-apply-order-by-skip-limit.md`
      for a follow-up task.

- [x] 1.3 Regression tests: sorted output for CALL-YIELD with ORDER BY (asc +
      desc), and SKIP/LIMIT coverage, at the server integration level

      Primary coverage is ENGINE-level (`crates/nexus-core/tests/cypher/
      test_call_procedures.rs`, extending the existing CALL-procedure test
      module) since 1.1 established the root cause lives in the shared
      core planner, not the server's routing/carve-out layer — engine-level
      tests exercise the exact same code path the server's lock-free
      executor clone calls into. Added: ascending sort, descending sort,
      ORDER BY on a RETURN alias of the YIELD column, ORDER BY with no
      RETURN clause at all, SKIP after ORDER BY, LIMIT after ORDER BY,
      SKIP+LIMIT combined, and a plain-MATCH ORDER BY control (pins the
      pattern-path/no-pattern-path isolation). ALSO added one
      server-integration-level test —
      `schema_procedures_yield_projection_honours_order_by_via_http_handler`
      in `crates/nexus-server/src/api/cypher/schema_procedures_test.rs`
      (extended the existing spawned-state harness) — exercising the exact
      bug-report shape through the PUBLIC `/cypher` HTTP handler end to
      end, satisfying this item's literal "server integration level"
      wording even though the root-cause investigation showed it was not
      strictly required to prove the fix.

## 2. Tail (docs + tests — check or waive with tailWaiver)
- [ ] 2.1 Update or create documentation covering the implementation
- [x] 2.2 Write tests covering the new behavior
- [x] 2.3 Run tests and confirm they pass
