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

**3868 scenarios — 2355 passed (60.9%), 1442 failed, 71 skipped.**

## Per-category

| Category | Pass | Fail | Skip | Total | Pass % |
|---|---:|---:|---:|---:|---:|
| `clauses/call` | 0 | 2 | 50 | 52 | 0.0% |
| `clauses/create` | 48 | 30 | 0 | 78 | 61.5% |
| `clauses/delete` | 7 | 34 | 0 | 41 | 17.1% |
| `clauses/match` | 145 | 206 | 1 | 352 | 41.2% |
| `clauses/match-where` | 28 | 6 | 0 | 34 | 82.4% |
| `clauses/merge` | 29 | 43 | 3 | 75 | 38.7% |
| `clauses/remove` | 6 | 27 | 0 | 33 | 18.2% |
| `clauses/return` | 29 | 33 | 1 | 63 | 46.0% |
| `clauses/return-orderby` | 21 | 13 | 1 | 35 | 60.0% |
| `clauses/return-skip-limit` | 18 | 13 | 0 | 31 | 58.1% |
| `clauses/set` | 17 | 34 | 2 | 53 | 32.1% |
| `clauses/union` | 8 | 4 | 0 | 12 | 66.7% |
| `clauses/unwind` | 7 | 7 | 0 | 14 | 50.0% |
| `clauses/with` | 9 | 20 | 0 | 29 | 31.0% |
| `clauses/with-orderBy` | 146 | 146 | 0 | 292 | 50.0% |
| `clauses/with-skip-limit` | 5 | 4 | 0 | 9 | 55.6% |
| `clauses/with-where` | 13 | 6 | 0 | 19 | 68.4% |
| `expressions/aggregation` | 17 | 17 | 1 | 35 | 48.6% |
| `expressions/boolean` | 130 | 20 | 0 | 150 | 86.7% |
| `expressions/comparison` | 48 | 24 | 0 | 72 | 66.7% |
| `expressions/conditional` | 13 | 0 | 0 | 13 | 100.0% |
| `expressions/existentialSubqueries` | 9 | 1 | 0 | 10 | 90.0% |
| `expressions/graph` | 26 | 32 | 3 | 61 | 42.6% |
| `expressions/list` | 96 | 89 | 0 | 185 | 51.9% |
| `expressions/literals` | 102 | 29 | 0 | 131 | 77.9% |
| `expressions/map` | 18 | 17 | 9 | 44 | 40.9% |
| `expressions/mathematical` | 3 | 3 | 0 | 6 | 50.0% |
| `expressions/null` | 42 | 2 | 0 | 44 | 95.5% |
| `expressions/path` | 7 | 0 | 0 | 7 | 100.0% |
| `expressions/pattern` | 14 | 36 | 0 | 50 | 28.0% |
| `expressions/precedence` | 39 | 82 | 0 | 121 | 32.2% |
| `expressions/quantifier` | 509 | 95 | 0 | 604 | 84.3% |
| `expressions/string` | 26 | 6 | 0 | 32 | 81.2% |
| `expressions/temporal` | 670 | 334 | 0 | 1004 | 66.7% |
| `expressions/typeConversion` | 20 | 27 | 0 | 47 | 42.6% |
| `useCases/countingSubgraphMatches` | 11 | 0 | 0 | 11 | 100.0% |
| `useCases/triadicSelection` | 19 | 0 | 0 | 19 | 100.0% |
| **total** | **2355** | **1442** | **71** | **3868** | **60.9%** |

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
