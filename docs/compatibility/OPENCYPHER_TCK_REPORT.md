# openCypher TCK Conformance Report

Measured pass/fail/skip over the vendored upstream openCypher TCK corpus. This is a
*conformance* number against the specification — distinct from the differential
Neo4j suite in `scripts/compatibility/`. Regenerate with the entry point below.

- **Pinned upstream commit:** `677cbafabb8c3c5eed458fd3b1ec0daec8d67d23`
- **Corpus:** `crates/nexus-core/tests/tck/opencypher/features/` (see `VENDOR.md`)
- **Reproduce:** `NEXUS_TCK=1 cargo +nightly test -p nexus-core --test tck_opencypher --all-features`
  or `scripts/compatibility/run-opencypher-tck.ps1`.

## Outcome model

- **pass** — every step matched a definition and its assertion held.
- **fail** — a matched step's assertion failed (a real conformance gap).
- **skip** — a step matched no definition yet (a capability the runner does not
exercise) or the scenario was skip-listed. Skips are counted, never hidden.

## Totals

**3868 scenarios — 942 passed (24.4%), 2836 failed, 90 skipped.**

## Per-category

| Category | Pass | Fail | Skip | Total | Pass % |
|---|---:|---:|---:|---:|---:|
| `clauses/call` | 0 | 2 | 50 | 52 | 0.0% |
| `clauses/create` | 25 | 53 | 0 | 78 | 32.1% |
| `clauses/delete` | 7 | 34 | 0 | 41 | 17.1% |
| `clauses/match` | 102 | 249 | 1 | 352 | 29.0% |
| `clauses/match-where` | 6 | 28 | 0 | 34 | 17.6% |
| `clauses/merge` | 23 | 49 | 3 | 75 | 30.7% |
| `clauses/remove` | 6 | 27 | 0 | 33 | 18.2% |
| `clauses/return` | 17 | 45 | 1 | 63 | 27.0% |
| `clauses/return-orderby` | 15 | 19 | 1 | 35 | 42.9% |
| `clauses/return-skip-limit` | 15 | 16 | 0 | 31 | 48.4% |
| `clauses/set` | 2 | 49 | 2 | 53 | 3.8% |
| `clauses/union` | 6 | 6 | 0 | 12 | 50.0% |
| `clauses/unwind` | 6 | 8 | 0 | 14 | 42.9% |
| `clauses/with` | 4 | 25 | 0 | 29 | 13.8% |
| `clauses/with-orderBy` | 50 | 242 | 0 | 292 | 17.1% |
| `clauses/with-skip-limit` | 1 | 8 | 0 | 9 | 11.1% |
| `clauses/with-where` | 0 | 19 | 0 | 19 | 0.0% |
| `expressions/aggregation` | 16 | 18 | 1 | 35 | 45.7% |
| `expressions/boolean` | 129 | 21 | 0 | 150 | 86.0% |
| `expressions/comparison` | 44 | 28 | 0 | 72 | 61.1% |
| `expressions/conditional` | 13 | 0 | 0 | 13 | 100.0% |
| `expressions/existentialSubqueries` | 2 | 8 | 0 | 10 | 20.0% |
| `expressions/graph` | 21 | 37 | 3 | 61 | 34.4% |
| `expressions/list` | 91 | 94 | 0 | 185 | 49.2% |
| `expressions/literals` | 102 | 29 | 0 | 131 | 77.9% |
| `expressions/map` | 18 | 17 | 9 | 44 | 40.9% |
| `expressions/mathematical` | 3 | 3 | 0 | 6 | 50.0% |
| `expressions/null` | 40 | 4 | 0 | 44 | 90.9% |
| `expressions/path` | 0 | 7 | 0 | 7 | 0.0% |
| `expressions/pattern` | 6 | 44 | 0 | 50 | 12.0% |
| `expressions/precedence` | 39 | 82 | 0 | 121 | 32.2% |
| `expressions/quantifier` | 41 | 563 | 0 | 604 | 6.8% |
| `expressions/string` | 12 | 20 | 0 | 32 | 37.5% |
| `expressions/temporal` | 51 | 953 | 0 | 1004 | 5.1% |
| `expressions/typeConversion` | 20 | 27 | 0 | 47 | 42.6% |
| `useCases/countingSubgraphMatches` | 9 | 2 | 0 | 11 | 81.8% |
| `useCases/triadicSelection` | 0 | 0 | 19 | 19 | 0.0% |
| **total** | **942** | **2836** | **90** | **3868** | **24.4%** |

## Skip-list (deliberately un-evaluated)

Scenarios the runner cannot yet exercise because they need a capability it does
not provide. Counted as skips, never as fails. Features Nexus attempts but gets
wrong (e.g. temporal semantics) are NOT here — those remain real fails above.

| Reason | Scenarios |
|---|---:|
| named fixture graph (binary-tree-N) not supported | 19 |
| procedure registration not supported by the harness | 50 |
| **total deliberate skips** | **69** |

The remaining skips in the per-category table are scenarios that use a Gherkin
step the runner does not define yet (they skip at the unmatched step).
