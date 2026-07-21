# Proposal: phase0_fix-multi-hop-count-star-incorrect

## Why

`count(*)` over a multi-hop relationship pattern can report a nonzero result
when the pattern has zero matches. Observed 2026-07-21 while testing the
MATCH...CREATE inline-target fix (commit 87ecca67): with both hops
independently confirmed at 0 via single-hop counts, a
`MATCH (a)-[:R1]->(b)-[:R2]->(c) RETURN count(*)`-style query over the same
data returned 1. A count that invents matches is a correctness bug affecting
aggregations, existence checks, and any logic gated on "pattern present".
The affected test deliberately works around it by asserting per-hop counts
(`write_refresh_visibility_test.rs`, chained-inline-targets test — see the
in-test note).

## What Changes

- Reproduce minimally: seed a graph where the 2-hop pattern has 0, 1, and N
  matches; assert `count(*)` (and `count(x)`) over the multi-hop MATCH equals
  the true match count in each case; identify the shape that returns the
  phantom 1 today.
- Root-cause where the phantom row originates (executor pattern-match
  producing a spurious empty row? aggregation counting an unmatched-driver
  row? projection of a partial binding?) with file:line evidence.
- Fix, plus regression tests for 0/1/N matches on 2-hop and 3-hop patterns,
  including patterns that share intermediate nodes.

## Impact

- Affected specs: `docs/specs/cypher-subset.md` (aggregations / pattern
  matching)
- Affected code: `crates/nexus-core/src/executor/` (pattern match + aggregate
  operators; exact site TBD by root-cause)
- Breaking change: NO — corrects wrong counts
- User benefit: aggregations over multi-hop patterns return true counts;
  existence logic stops seeing phantom matches
