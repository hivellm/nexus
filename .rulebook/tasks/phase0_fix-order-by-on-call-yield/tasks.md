# Tasks: phase0_fix-order-by-on-call-yield

`CALL db.labels() YIELD label RETURN label ORDER BY label` returns unsorted
rows on a live server (`B, A, C, D, E`; Neo4j sorts). Set content correct,
`ORDER BY` silently ignored on the post-YIELD projection. Full context in
`proposal.md`.

## 1. Root cause and fix
- [ ] 1.1 Reproduce and map the affected shapes: CALL-YIELD `ORDER BY` on the
      engine path vs the read-only lock-free procedure path; also test
      `SKIP`/`LIMIT` after YIELD, and confirm plain `MATCH ... ORDER BY` sorts
      correctly on the same server (isolate the difference with evidence)
- [ ] 1.2 Root-cause where the sort is dropped (parser clause attachment vs
      procedure pipeline never applying the sort operator) with file:line
      evidence, then fix so ORDER BY/SKIP/LIMIT apply to procedure YIELD
      projections
- [ ] 1.3 Regression tests: sorted output for CALL-YIELD with ORDER BY (asc +
      desc), and SKIP/LIMIT coverage, at the server integration level

## 2. Tail (docs + tests — check or waive with tailWaiver)
- [ ] 2.1 Update or create documentation covering the implementation
- [ ] 2.2 Write tests covering the new behavior
- [ ] 2.3 Run tests and confirm they pass
