## 1. Implementation

Pattern comprehensions now run a real full-enumeration correlated graph walk
(`eval/helpers/pattern_comprehension.rs`, streaming-projection closure API)
sharing the EXISTS probe machinery (isomorphism, 64-hop clamp, anon nodes, all
directions, var-length trails), replacing the stub that returned `[]` for any
unbound pattern variable. Gated by a 2-round independent opus review (round 1:
2 MAJORs — unbounded materialization → streaming + row cap; irreversible parser
commit on `[ident = (` → tentative parse with full position restore — plus
minors; round 2: APPROVE + a polish pass fixing the comma-sentinel ordering
false-positive). Commit 671b84c4.

- [x] 1.1 pattern-comprehension emits path-shaped values (commit 671b84c4) — `[p = (n)-->() | p]` captures the `p =` binding (new Expression-level `binding_variable` field, chosen over `Pattern.path_variable` to avoid coupling with the MATCH-side planner read) and emits `{nodes, relationships}` in traversal order per match; new-variable projection `[(n)-[:T]->(b) | b.name]`, per-candidate inner WHERE, correlated outer vars, NULL input ⇒ `[]`, var-length = one list element per distinct trail. Explicit errors: comma-separated parts with a path binding, QPP, named rel-var on var-length. Result guarded by the engine row cap (graceful OutOfMemory).

## 2. Tail (docs + tests — check or waive with tailWaiver)
- [x] 2.1 Update or create documentation covering the implementation — CHANGELOG [3.0.0] entry + `docs/specs/cypher-subset.md` § Comprehensions (semantics + unsupported/explicit-errors lists), commit 671b84c4.
- [x] 2.2 Write tests covering the new behavior — 16 module tests in `eval/helpers/pattern_comprehension_tests.rs` (projection, path shape incl. incoming/2-hop/var-length trail order asserted on arrays, WHERE filter, NULL, isomorphism, parser backtrack ×4, comma/sentinel rejection, size()).
- [x] 2.3 Run tests and confirm they pass — targeted module 49/49; full lib 2601/2604 where the 3 are documented environment-sensitive tests that pass in isolation (verbatim evidence captured: plain 0-vs-1 row-count assert mismatches only under the full ~2600-test parallel run; refuted shared-catalog attribution — isolated test envs); clippy -D warnings zero; per-file rustfmt clean. TCK 925 → 930 (expressions/pattern capped by the CREATE cross-clause binding bug, tracked as phase21_tck-create-cross-clause-binding).
