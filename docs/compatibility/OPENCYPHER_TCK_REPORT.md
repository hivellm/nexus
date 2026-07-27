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

**3868 scenarios — 564 passed (14.6%), 3214 failed, 90 skipped.**

## Per-category

| Category | Pass | Fail | Skip | Total | Pass % |
|---|---:|---:|---:|---:|---:|
| `clauses/call` | 0 | 2 | 50 | 52 | 0.0% |
| `clauses/create` | 14 | 64 | 0 | 78 | 17.9% |
| `clauses/delete` | 7 | 34 | 0 | 41 | 17.1% |
| `clauses/match` | 19 | 332 | 1 | 352 | 5.4% |
| `clauses/match-where` | 3 | 31 | 0 | 34 | 8.8% |
| `clauses/merge` | 17 | 55 | 3 | 75 | 22.7% |
| `clauses/remove` | 6 | 27 | 0 | 33 | 18.2% |
| `clauses/return` | 12 | 50 | 1 | 63 | 19.0% |
| `clauses/return-orderby` | 14 | 20 | 1 | 35 | 40.0% |
| `clauses/return-skip-limit` | 7 | 24 | 0 | 31 | 22.6% |
| `clauses/set` | 1 | 50 | 2 | 53 | 1.9% |
| `clauses/union` | 6 | 6 | 0 | 12 | 50.0% |
| `clauses/unwind` | 5 | 9 | 0 | 14 | 35.7% |
| `clauses/with` | 3 | 26 | 0 | 29 | 10.3% |
| `clauses/with-orderBy` | 27 | 265 | 0 | 292 | 9.2% |
| `clauses/with-skip-limit` | 1 | 8 | 0 | 9 | 11.1% |
| `clauses/with-where` | 0 | 19 | 0 | 19 | 0.0% |
| `expressions/aggregation` | 2 | 32 | 1 | 35 | 5.7% |
| `expressions/boolean` | 6 | 144 | 0 | 150 | 4.0% |
| `expressions/comparison` | 32 | 40 | 0 | 72 | 44.4% |
| `expressions/conditional` | 13 | 0 | 0 | 13 | 100.0% |
| `expressions/existentialSubqueries` | 1 | 9 | 0 | 10 | 10.0% |
| `expressions/graph` | 18 | 40 | 3 | 61 | 29.5% |
| `expressions/list` | 85 | 100 | 0 | 185 | 45.9% |
| `expressions/literals` | 55 | 76 | 0 | 131 | 42.0% |
| `expressions/map` | 18 | 17 | 9 | 44 | 40.9% |
| `expressions/mathematical` | 3 | 3 | 0 | 6 | 50.0% |
| `expressions/null` | 37 | 7 | 0 | 44 | 84.1% |
| `expressions/path` | 0 | 7 | 0 | 7 | 0.0% |
| `expressions/pattern` | 0 | 50 | 0 | 50 | 0.0% |
| `expressions/precedence` | 35 | 86 | 0 | 121 | 28.9% |
| `expressions/quantifier` | 41 | 563 | 0 | 604 | 6.8% |
| `expressions/string` | 5 | 27 | 0 | 32 | 15.6% |
| `expressions/temporal` | 51 | 953 | 0 | 1004 | 5.1% |
| `expressions/typeConversion` | 20 | 27 | 0 | 47 | 42.6% |
| `useCases/countingSubgraphMatches` | 0 | 11 | 0 | 11 | 0.0% |
| `useCases/triadicSelection` | 0 | 0 | 19 | 19 | 0.0% |
| **total** | **564** | **3214** | **90** | **3868** | **14.6%** |

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
