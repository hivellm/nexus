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

**3868 scenarios — 2415 passed (62.4%), 1382 failed, 71 skipped.**

## Per-category

| Category | Pass | Fail | Skip | Total | Pass % |
|---|---:|---:|---:|---:|---:|
| `clauses/call` | 0 | 2 | 50 | 52 | 0.0% |
| `clauses/create` | 48 | 30 | 0 | 78 | 61.5% |
| `clauses/delete` | 7 | 34 | 0 | 41 | 17.1% |
| `clauses/match` | 145 | 206 | 1 | 352 | 41.2% |
| `clauses/match-where` | 29 | 5 | 0 | 34 | 85.3% |
| `clauses/merge` | 29 | 43 | 3 | 75 | 38.7% |
| `clauses/remove` | 6 | 27 | 0 | 33 | 18.2% |
| `clauses/return` | 37 | 25 | 1 | 63 | 58.7% |
| `clauses/return-orderby` | 22 | 12 | 1 | 35 | 62.9% |
| `clauses/return-skip-limit` | 24 | 7 | 0 | 31 | 77.4% |
| `clauses/set` | 17 | 34 | 2 | 53 | 32.1% |
| `clauses/union` | 8 | 4 | 0 | 12 | 66.7% |
| `clauses/unwind` | 7 | 7 | 0 | 14 | 50.0% |
| `clauses/with` | 9 | 20 | 0 | 29 | 31.0% |
| `clauses/with-orderBy` | 147 | 145 | 0 | 292 | 50.3% |
| `clauses/with-skip-limit` | 6 | 3 | 0 | 9 | 66.7% |
| `clauses/with-where` | 14 | 5 | 0 | 19 | 73.7% |
| `expressions/aggregation` | 18 | 16 | 1 | 35 | 51.4% |
| `expressions/boolean` | 130 | 20 | 0 | 150 | 86.7% |
| `expressions/comparison` | 48 | 24 | 0 | 72 | 66.7% |
| `expressions/conditional` | 13 | 0 | 0 | 13 | 100.0% |
| `expressions/existentialSubqueries` | 10 | 0 | 0 | 10 | 100.0% |
| `expressions/graph` | 26 | 32 | 3 | 61 | 42.6% |
| `expressions/list` | 97 | 88 | 0 | 185 | 52.4% |
| `expressions/literals` | 102 | 29 | 0 | 131 | 77.9% |
| `expressions/map` | 18 | 17 | 9 | 44 | 40.9% |
| `expressions/mathematical` | 3 | 3 | 0 | 6 | 50.0% |
| `expressions/null` | 42 | 2 | 0 | 44 | 95.5% |
| `expressions/path` | 7 | 0 | 0 | 7 | 100.0% |
| `expressions/pattern` | 31 | 19 | 0 | 50 | 62.0% |
| `expressions/precedence` | 39 | 82 | 0 | 121 | 32.2% |
| `expressions/quantifier` | 509 | 95 | 0 | 604 | 84.3% |
| `expressions/string` | 26 | 6 | 0 | 32 | 81.2% |
| `expressions/temporal` | 691 | 313 | 0 | 1004 | 68.8% |
| `expressions/typeConversion` | 20 | 27 | 0 | 47 | 42.6% |
| `useCases/countingSubgraphMatches` | 11 | 0 | 0 | 11 | 100.0% |
| `useCases/triadicSelection` | 19 | 0 | 0 | 19 | 100.0% |
| **total** | **2415** | **1382** | **71** | **3868** | **62.4%** |

## Skip-list (deliberately un-evaluated)

Scenarios the runner cannot yet exercise because they need a capability it does
not provide. Counted as skips, never as fails. Features Nexus attempts but gets
wrong (e.g. temporal semantics) are NOT here — those remain real fails above.

| Reason | Scenarios |
|---|---:|
| procedure registration not supported by the harness | 50 |
| **total deliberate skips** | **50** |

The remaining skips in the per-category table are scenarios that use a Gherkin
step the runner does not define yet (they skip at the unmatched step).
