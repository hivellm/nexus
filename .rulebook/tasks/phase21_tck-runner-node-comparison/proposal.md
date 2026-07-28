# Proposal: phase21_tck-runner-node-comparison

## Why
457 occurrences across the openCypher TCK failure log share a single cause that
has nothing to do with query semantics: scenarios that `RETURN n` (a whole node
or relationship) fail only at row-comparison time. The harness parses expected
cells into `{"@tck_node": true, "@labels": [...], props}` while Nexus rows carry
`{"_nexus_id": ..., "_nexus_labels": [...], props}` — so structurally-correct
results are counted as failures across many categories at once
(existentialSubqueries 7/7 remaining fails, clauses/create Create2[6]/[11]/[12],
and more). Scalar projections of the very same queries pass, proving the engine
side is often already right. This is likely one of the largest single levers
toward TCK 100%.

## What Changes
Teach the TCK runner's comparison layer (`crates/nexus-core/tests/tck_common/mod.rs`,
`values_equal`) to compare `@tck_node`-marked expected cells structurally
against Nexus node objects — labels as a set, properties exactly, storage id
ignored — mirroring the `@tck_path` special case that already exists
(`tck_path_matches`). Same treatment for relationship markers if the cell
parser produces them, and correct recursion so nodes nested inside lists, maps
and path objects compare too. The server/executor row format is NOT touched —
it is frozen by the Neo4j-compatibility constraint (CLAUDE.md #1); this is a
test-harness fidelity fix, the same class as the "harness drops fail reasons"
Step-0 workstream in docs/analysis/tck/01-measurement-and-methodology.md.

## Impact
- Affected specs: none (test harness only; conformance measurement fidelity)
- Affected code: crates/nexus-core/tests/tck_common/mod.rs (+ tck_opencypher.rs if wiring requires)
- Breaking change: NO
- User benefit: the TCK report stops under-counting genuinely-passing scenarios, giving an honest conformance number and unblocking per-category triage that is currently drowned in serialization noise.
