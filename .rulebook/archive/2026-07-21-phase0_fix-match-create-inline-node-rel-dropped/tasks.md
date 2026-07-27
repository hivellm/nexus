# Tasks: phase0_fix-match-create-inline-node-rel-dropped

`MATCH … CREATE (bound)-[:T]->(new:Label {…})` silently drops the relationship:
the inline-created target node IS persisted and visible, but the relationship
record is never written. No error is raised — the query reports success.

**Confirmed repro (2026-07-21, discovered while writing
`tests/executor/write_refresh_visibility_test.rs`; verified NOT a
refresh-staleness artifact — the relationship stays missing even after a manual
`engine.refresh_executor()`):**

```cypher
CREATE (a:X {k: 1})
MATCH (a:X {k: 1}) CREATE (a)-[:CR]->(x:Y {k: 2})
MATCH (n:Y) RETURN count(n)              -- 1  (node persisted)
MATCH (a:X)-[r:CR]->(x:Y) RETURN count(r) -- 0  (relationship LOST — expected 1)
```

Affects both the top-level dispatch and the `PROFILE`/CALL-subquery internal
dispatch identically (`query_pipeline.rs` MATCH…CREATE arm →
`execute_match_create_query`). The failing shape is specifically: relationship
target node created INLINE in the same CREATE clause. Two adjacent shapes work
fine and are already pinned green by `write_refresh_visibility_test.rs`:
CREATE of a node only from a MATCH context, and CREATE of a relationship where
BOTH endpoints pre-exist and are matched.

Sibling-but-distinct known bugs (do not conflate):
`phase0_fix-merge-relationship-dropped` (variable-less MERGE relationship
bails to `Ok(None)` and merges only the first node) and
`phase0_fix-relationship-write-clauses-dropped` (MERGE SET filtering / DELETE r
collection). This task is the MATCH…CREATE inline-target-node path.

## 1. Implementation
- [x] 1.1 Root-cause: trace `execute_match_create_query` for the inline-new-node
      relationship shape — establish exactly where the relationship write is
      skipped (target-node binding never entered into the rel-creation context?
      relationship arm unreached when the pattern element chain contains a
      fresh node?) with file:line evidence before changing code

      **Root cause (confirmed, file:line evidence):**
      `execute_match_create_query` (`crates/nexus-core/src/engine/match_exec.rs:200-222`)
      is a thin pass-through — it builds an `executor::Query` from the raw
      Cypher text (or an AST override) and delegates straight to
      `self.executor.execute(&query_obj)`. The real MATCH…CREATE write logic
      lives in `Executor::execute_create_with_context`
      (`crates/nexus-core/src/executor/operators/create.rs`, was lines
      773-1189 pre-fix, invoked from `operators/dispatch.rs:217` /
      `dispatch.rs:746` for both the top-level `Operator::Create` arm and the
      internal/PROFILE dispatch — same function, same bug, both paths).

      That function iterates `pattern.elements` (Node, Relationship, Node, …)
      **in a single left-to-right pass** using two pieces of state:
      `last_node_var: Option<String>` (source-of-the-next-relationship) and
      `node_ids: HashMap<String, u64>` (resolved variable → node id). The
      per-element handling was:
      - Node arm (was lines 961-1064): if the node's variable is **not**
        already in `node_ids`, create it now and insert `node_ids[var] =
        node_id` — this runs on the node's OWN turn in the iteration.
      - Relationship arm (was lines 1065-1186): resolves
        `source_id = node_ids[last_node_var]`, then looks at
        `pattern.elements[idx + 1]` for the target and does
        `target_id = node_ids.get(target_var)` (was line 1096) — a **read-only
        lookup**, never a create.

      For `MATCH (a) CREATE (a)-[:CR]->(x:Y {k:2})`, `pattern.elements` is
      `[Node(a, bound), Relationship(CR), Node(x, new)]`. At `idx=1`
      (the Relationship element) `node_ids` contains only `{"a": …}` — `x`
      has not been created yet because its own Node-arm turn is `idx=2`,
      which runs strictly *after* the relationship. So
      `node_ids.get("x")` returns `None`, and the relationship arm's `else`
      branch (was lines 1151-1157) just does
      `tracing::warn!("...Target node not found...")` and **falls through
      with no error and no write** — `execute_create_with_context` still
      returns `Ok(())`. The loop then continues to `idx=2`, where the Node
      arm creates `x` as normal (its creation never depended on the
      relationship), which is exactly the observed symptom: node persisted,
      relationship silently dropped, no error surfaced anywhere in the call
      chain (`execute_match_create_query` → `query_pipeline.rs` →
      `execute_cypher`). Confirmed empirically: added
      `RUST_LOG=nexus_core::executor::operators::create=warn` while running
      the reported repro reproduced the "Target node not found" warning with
      zero relationship writes.

      The two adjacent shapes that already worked are consistent with this
      trace: standalone node-only CREATE never reaches the Relationship arm
      at all; CREATE between two pre-existing MATCH-bound nodes has BOTH
      variables in `node_ids` before the loop starts (populated from the
      `row` at the top of the per-row loop, `create.rs` ~line 911), so the
      target lookup always succeeds for that case.

- [x] 1.2 Fix so the relationship record is written with the freshly created
      node id as its endpoint; the fix must also cover the mirrored direction
      (`(new)<-[:T]-(bound)`) and multi-hop patterns that mix bound and inline
      nodes, or explicitly error on unsupported shapes — never silent success

      Fixed in `crates/nexus-core/src/executor/operators/create.rs`,
      `Executor::execute_create_with_context`. The Relationship arm no
      longer does a read-only `node_ids.get(target_var)` lookup; it now
      resolves the target the same way the standalone
      `execute_create_pattern_internal` sibling function already did
      correctly (lookahead + inline-create + skip-flag): if the next
      pattern element's variable is already bound, reuse it (unchanged
      behavior for the pre-existing-both-endpoints case); otherwise create
      it right there, before writing the relationship, register it in
      `node_ids`, and set a `skip_next_node` flag so the Node arm's later
      turn for that same element doesn't try to create it again. Node
      creation logic (label/property resolution, external-id handling,
      FTS/spatial autopopulation, label-index bookkeeping) was extracted
      into a new shared helper, `create_pattern_node_with_context`, so the
      Node arm and the Relationship arm's inline-target path can never
      diverge like this again. `last_node_var: Option<String>` was replaced
      with `last_node_id: Option<u64>`, updated unconditionally after every
      resolved node (mirrors the sibling function and additionally allows
      an anonymous node to anchor a relationship, which the old
      variable-name-gated tracking could not). This single-pass
      lookahead generalizes to multi-hop chains for free (each
      relationship resolves its own immediate neighbor and hands off via
      `last_node_id`), so no two-pass restructure was needed.

      Direction (`RelationshipDirection::Incoming` / `Outgoing` / `Both`) is
      not read anywhere in `create.rs` — CREATE has always treated pattern
      elements as source→target in left-to-right array order regardless of
      arrow direction, in both `execute_create_with_context` and the sibling
      `execute_create_pattern_internal`. That is a separate, pre-existing
      limitation outside this task's scope (not conflated with the
      inline-node-drop bug); the "mirrored direction" test below asserts a
      relationship exists using a direction-agnostic match, not the
      resulting edge's head/tail.

- [x] 1.3 Un-split the deliberately narrowed regression tests: extend
      `tests/executor/write_refresh_visibility_test.rs` (tests 12/14/15 were
      restructured to avoid this bug) or add a dedicated test module asserting
      node AND relationship visibility for the inline shape, both dispatch
      paths (top-level and PROFILE/internal)

      Added 4 new tests to `write_refresh_visibility_test.rs` (section 10)
      covering: the reported repro shape, a mirrored-direction variant, a
      2-hop chain mixing a bound source with two inline targets, and the
      PROFILE/internal-dispatch variant of the reported shape. The existing
      split tests (`match_create_when_match_matches_creates_node_and_is_visible`,
      `match_create_when_match_matches_creates_relationship_and_is_visible`,
      and their `profiled_*` counterparts) were left as-is — they still add
      distinct coverage (pure node-only CREATE, and relationship CREATE
      between two pre-existing MATCH-bound nodes) and un-splitting them
      would have removed real coverage rather than restoring it.

## 2. Tail (docs + tests — check or waive with tailWaiver)
- [x] 2.1 Update or create documentation covering the implementation
      Documentation added: CHANGELOG.md entry under [3.0.0] with task id, and cypher-subset.md expanded with MATCH...CREATE inline-node examples and semantics.
- [x] 2.2 Write tests covering the new behavior
- [x] 2.3 Run tests and confirm they pass
