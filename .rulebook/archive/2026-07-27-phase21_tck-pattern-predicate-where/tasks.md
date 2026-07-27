## 1. Implementation

Correlated `EXISTS { pattern [WHERE expr] }` predicates in WHERE now run a real
depth-first graph probe (`executor/eval/helpers.rs`, `Exists` arm in
`eval/projection/core.rs`) instead of the previous stub. Gated by a 3-round
independent opus review (round 1: 6 findings fixed incl. read-path catalog
pollution, per-row full-graph materialization, relationship isomorphism; round
2: NULL-inner-WHERE filter semantics; round 3: APPROVE), each round with the
full quality gate green.

- [x] 1.1 EXISTS-in-WHERE uses graph context (commit 56f96a5f) — correlated DFS probe over store adjacency: multi-hop chains, comma-separated pattern parts, label + inline property constraints (incl. correlated maps like `(b {id: a.id})`), relationship-variable reuse, triangle correlation, inner WHERE per candidate binding with outer scope visible, Cypher 3VL (NULL correlated var ⇒ Null; NULL inner WHERE filters like false), relationship isomorphism, no catalog writes on the read path, anonymous anchors enumerate ids via label bitmap/record scan (no OutOfMemory cliff).
- [x] 1.2 anon/undirected/var-length supported — anonymous nodes + undirected (`--`, per-hop Both orientation) landed with 1.1 (commit 56f96a5f); variable-length rels (commit 8431cde2): bare `*` = `*1..` (openCypher default, deliberately diverging from the MATCH-side pre-existing off-by-one), `+`/`?`/exact/ranges, zero-length acceptance when min 0 (constraints checked against the anchor), isomorphism across the segment, engine-wide 64-hop clamp shared with MATCH (`MAX_VAR_LENGTH_PATH_DEPTH` widened to `pub(in crate::executor)`). Named rel-var on var-length inside EXISTS ⇒ explicit `ERR_VAR_LENGTH_REL_VARIABLE_NOT_IMPLEMENTED` (would need LIST<RELATIONSHIP> binding).

## 2. Tail (docs + tests — check or waive with tailWaiver)
- [x] 2.1 Update or create documentation covering the implementation — CHANGELOG [3.0.0] entries + `docs/specs/cypher-subset.md` § EXISTS Subqueries (supported shapes, 3VL, 64-hop ceiling note, unsupported: named var-length rel-var, QPP explicit error) — commits 56f96a5f + 8431cde2.
- [x] 2.2 Write tests covering the new behavior — 33 in-module exists-probe unit tests (positive + negative per capability, incl. empirically-verified non-vacuous isomorphism test and error-path pinning).
- [x] 2.3 Run tests and confirm they pass — targeted exists filter 33/0; full nexus-core lib suite 2587 passed / 0 relevant failures (single known pre-existing parallel-load flake `property_index_survives_restart`, re-verified passing in isolation); fmt clean; clippy -D warnings zero. TCK conformance report refreshed at 925/3868 (23.9%), commit 8df6ea4b.
