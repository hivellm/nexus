# Proposal: phase0_fix-order-by-on-call-yield

## Why

`CALL db.labels() YIELD label RETURN label ORDER BY label` returns the rows
UNSORTED on a live server (observed `B, A, C, D, E` where Neo4j returns
`A, B, C, D, E` for the same seeded data — 2026-07-21, during the
schema-procedures post-fix verification; outputs in
`bench-out/schema-procedures-postfix/`). The set content is correct; only the
`ORDER BY` on the projection after a procedure `YIELD` is silently ignored.
Silent misordering breaks Neo4j compatibility and any client that relies on
sorted introspection output (UIs, diff-based tooling, pagination).

Scope note: observed specifically on `CALL … YIELD … RETURN … ORDER BY …`.
Whether the defect covers all post-YIELD projections or only certain shapes
(e.g. only on the read-only lock-free procedure path added in a7e78078) must
be established, not assumed.

## What Changes

- Reproduce and map the shape: does `ORDER BY` after `YIELD` sort correctly
  through the engine executor path vs the read-only procedure carve-out? Does
  a bare `MATCH … RETURN … ORDER BY` on the same server sort correctly (it
  should — isolate the difference)?
- Root-cause where the sort clause is dropped for the CALL-YIELD pipeline
  (parser attaching ORDER BY to the wrong clause, or the procedure execution
  path never applying the sort operator).
- Fix so `ORDER BY` (and by extension `SKIP`/`LIMIT` if also affected — test
  them) applies to procedure YIELD projections; regression tests comparing
  sorted output.

## Impact

- Affected specs: `docs/specs/cypher-subset.md` (Schema Introspection
  Procedures section documents these calls)
- Affected code: executor CALL/YIELD pipeline
  (`crates/nexus-core/src/executor/operators/procedures/`, projection/sort
  operators), possibly the read-only procedure path in
  `crates/nexus-server/src/api/cypher/`
- Breaking change: NO — output becomes correctly ordered where it was
  silently unordered
- User benefit: Neo4j-compatible, deterministic ordering for procedure
  results; sorted introspection output for clients and tooling
