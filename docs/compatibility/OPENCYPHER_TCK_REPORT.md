# openCypher TCK Conformance Report

Measured pass/fail/skip over the vendored upstream openCypher TCK corpus. This is a
**strict openCypher-TCK conformance metric** — distinct from the differential
Neo4j suite (`scripts/compatibility/test-neo4j-nexus-compatibility-200.ps1`, which
measures agreement with one specific implementation on a curated query set). The TCK
number reflects spec-compliance against the authoritative test corpus with no partial credit.

- **Pinned upstream commit:** `677cbafabb8c3c5eed458fd3b1ec0daec8d67d23`
- **Corpus:** `crates/nexus-core/tests/tck/opencypher/features/` (see `VENDOR.md`)
- **Reproduce:** `NEXUS_TCK=1 cargo +nightly test -p nexus-core --test tck_opencypher --all-features`
  or `scripts/compatibility/run-opencypher-tck.ps1`

## Outcome model

- **pass** — every step matched a definition and its assertion held.
- **fail** — a matched step's assertion failed (a real conformance gap).
- **skip** — a step matched no definition yet (a capability the runner does not
exercise) or the scenario was skip-listed. Skips are counted, never hidden.

## Totals

**3868 scenarios — 509 passed (13.2%), 3175 failed, 184 skipped.**

## Per-category

| Category | Pass | Fail | Skip | Total | Pass % |
|---|---:|---:|---:|---:|---:|
| `clauses/call` | 0 | 2 | 50 | 52 | 0.0% |
| `clauses/create` | 14 | 55 | 9 | 78 | 17.9% |
| `clauses/delete` | 0 | 39 | 2 | 41 | 0.0% |
| `clauses/match` | 12 | 339 | 1 | 352 | 3.4% |
| `clauses/match-where` | 1 | 30 | 3 | 34 | 2.9% |
| `clauses/merge` | 16 | 50 | 9 | 75 | 21.3% |
| `clauses/remove` | 6 | 27 | 0 | 33 | 18.2% |
| `clauses/return` | 12 | 49 | 2 | 63 | 19.0% |
| `clauses/return-orderby` | 13 | 20 | 2 | 35 | 37.1% |
| `clauses/return-skip-limit` | 7 | 16 | 8 | 31 | 22.6% |
| `clauses/set` | 1 | 50 | 2 | 53 | 1.9% |
| `clauses/union` | 6 | 6 | 0 | 12 | 50.0% |
| `clauses/unwind` | 5 | 7 | 2 | 14 | 35.7% |
| `clauses/with` | 3 | 25 | 1 | 29 | 10.3% |
| `clauses/with-orderBy` | 27 | 264 | 1 | 292 | 9.2% |
| `clauses/with-skip-limit` | 1 | 7 | 1 | 9 | 11.1% |
| `clauses/with-where` | 0 | 18 | 1 | 19 | 0.0% |
| `expressions/aggregation` | 2 | 20 | 13 | 35 | 5.7% |
| `expressions/boolean` | 6 | 144 | 0 | 150 | 4.0% |
| `expressions/comparison` | 30 | 42 | 0 | 72 | 41.7% |
| `expressions/conditional` | 13 | 0 | 0 | 13 | 100.0% |
| `expressions/existentialSubqueries` | 1 | 9 | 0 | 10 | 10.0% |
| `expressions/graph` | 17 | 40 | 4 | 61 | 27.9% |
| `expressions/list` | 82 | 89 | 14 | 185 | 44.3% |
| `expressions/literals` | 55 | 76 | 0 | 131 | 42.0% |
| `expressions/map` | 16 | 14 | 14 | 44 | 36.4% |
| `expressions/mathematical` | 3 | 3 | 0 | 6 | 50.0% |
| `expressions/null` | 34 | 3 | 7 | 44 | 77.3% |
| `expressions/path` | 0 | 7 | 0 | 7 | 0.0% |
| `expressions/pattern` | 0 | 50 | 0 | 50 | 0.0% |
| `expressions/precedence` | 35 | 86 | 0 | 121 | 28.9% |
| `expressions/quantifier` | 16 | 588 | 0 | 604 | 2.6% |
| `expressions/string` | 5 | 27 | 0 | 32 | 15.6% |
| `expressions/temporal` | 51 | 935 | 18 | 1004 | 5.1% |
| `expressions/typeConversion` | 19 | 27 | 1 | 47 | 40.4% |
| `useCases/countingSubgraphMatches` | 0 | 11 | 0 | 11 | 0.0% |
| `useCases/triadicSelection` | 0 | 0 | 19 | 19 | 0.0% |
| **total** | **509** | **3175** | **184** | **3868** | **13.2%** |

## Skip-list (deliberately un-evaluated)

Scenarios the runner cannot yet exercise because they need a capability it does
not provide. Counted as skips, never as fails. Features Nexus attempts but gets
wrong (e.g. temporal semantics) are NOT here — those remain real fails above.

| Reason | Scenarios |
|---|---:|
| control-query reference comparison not supported | 33 |
| named fixture graph (binary-tree-N) not supported | 19 |
| procedure registration not supported by the harness | 50 |
| query parameters not wired into the harness | 62 |
| **total deliberate skips** | **164** |

The remaining skips in the per-category table are scenarios that use a Gherkin
step the runner does not define yet (they skip at the unmatched step).
