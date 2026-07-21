# Tasks: phase0_fix-multi-hop-count-star-incorrect

`count(*)` over a multi-hop pattern returned 1 with both hops independently
at 0 (observed 2026-07-21; workaround note lives in
`tests/executor/write_refresh_visibility_test.rs` chained-targets test).
Details in proposal.md.

## 1. Root cause and fix
- [ ] 1.1 Minimal reproduction: 2-hop pattern with true match counts 0, 1, N —
      pin which shape yields the phantom count today (failing tests first)
- [ ] 1.2 Root-cause the phantom row (pattern-match emitting an empty/partial
      binding? aggregate counting an unmatched driver row?) with file:line
      evidence, then fix
- [ ] 1.3 Regression tests: count(*) and count(var) on 2-hop and 3-hop
      patterns at 0/1/N matches, including shared intermediate nodes; remove
      the per-hop workaround note/assertions in
      write_refresh_visibility_test.rs in favour of the direct multi-hop
      count once it is trustworthy (keep per-hop asserts as extra coverage)

## 2. Tail (docs + tests — check or waive with tailWaiver)
- [ ] 2.1 Update or create documentation covering the implementation
- [ ] 2.2 Write tests covering the new behavior
- [ ] 2.3 Run tests and confirm they pass
