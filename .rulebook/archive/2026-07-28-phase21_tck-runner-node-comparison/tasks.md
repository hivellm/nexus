## 1. Implementation

Root cause was NOT the missing matchers the proposal assumed —
`tck_node_matches`/`tck_rel_matches` already existed and were correct
(ab53be1c). The real defect was one call-site down: `rows_equal`
(tests/tck_common/mod.rs) called `values_equal(got, want)` with the arguments
SWAPPED. `values_equal`'s marker dispatch inspects only its first (expected)
argument, and actual Nexus values never carry `@tck_*` keys — so node, rel,
path AND IEEE-float comparison silently never fired for any runner row and
fell to a generic object branch that can never match. One-line orientation fix
(+ explanatory comment). Opus review APPROVE (argument-role trace verified at
every call site; 3 direction guards + 1 over-match guard confirmed
discriminating).

- [x] 1.1 values_equal recognizes @tck_node/@tck_rel cells — already implemented pre-task; made REACHABLE by the rows_equal orientation fix (commit pending in this changeset). Marker-gating pinned (`bare_map_does_not_match_a_nexus_node`).
- [x] 1.2 nested occurrences + full re-measure — nested-in-list/map/path recursion verified correct (tests) since values_equal recurses with consistent argument order; full TCK re-run: **952 → 1119 (+167, 28.9%)**, fails 2826 → 2659, skips unchanged. Biggest category moves: match-where 6→28/34 (82.4%), string 12→26/32, set 2→17/53, match +32, existentialSubqueries 3→9/10, with-orderBy +17, with-where 0→11/19, create 34→42/78, pattern 6→14/50. expressions/path stayed 0/7 (verified genuine unrelated gaps, not comparison noise).

## 2. Tail (docs + tests — check or waive with tailWaiver)
- [x] 2.1 Update or create documentation covering the implementation — CHANGELOG [3.0.0] entry; TCK report regenerated at 1119/3868.
- [x] 2.2 Write tests covering the new behavior — 10 new tests in tests/tck_cells.rs (30 total): 4 rows_equal regression guards in the runner's exact (got, want) convention (3 pin the direction, 1 pins non-over-matching) + 6 values_equal coverage tests for the previously-unreachable matcher paths.
- [x] 2.3 Run tests and confirm they pass — tck_cells 30/30; tck_runner (shares tck_common) 22/22 unaffected; clippy -D warnings clean; rustfmt clean; full TCK re-measured (1119/3868, 28.9%).
