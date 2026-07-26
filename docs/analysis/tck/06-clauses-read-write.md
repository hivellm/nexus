# 06 — Clauses: Read & Write

`clauses/*`. The read side was measured by a **live harness run** (`NEXUS_TCK=1`)
with per-scenario panic-signature bucketing — so these breakdowns are ground truth,
not inference. Most of the mass is workstream **W-A** (error semantics) + **W-D**
(temporal, cross-dependency) on the read side, and **W-C** (result-shape/counting)
on the write side.

---

## F-035 — `clauses/match` (3%) is dominated by missing error detection, not broken matching

**Evidence (measured buckets, 339 fails).** error-expected-but-query-succeeded 172,
error-token-missing 69, row-not-found 38, query-errored 37, row-count 21. So
**~71% (241/339) are negative-test scenarios** where Nexus should raise an error and
instead returns rows (or the right kind with the wrong token). The aliased/simple
positive scenarios pass — the core matching machinery works.

**Impact.** `clauses/match` moves almost entirely on **W-A** (F-009): the missing
semantic-validation pass (A2) for `VariableTypeConflict` (37 in-cluster),
`VariableAlreadyBound` (28), `AmbiguousAggregationExpression`, `UndefinedVariable`,
etc., plus error-token emission (A1). Bracket-less relationships (F-036) account for
much of the remaining row-errored/row-not-found tail.

**Confidence.** High (measured).

---

## F-036 — Bracket-less relationships `-->` / `--` / `<--` are unsupported

**Evidence.** `parse_relationship_pattern` consumes the leading `-`/`<-` then
**unconditionally requires `[`** (`parser/clauses/pattern.rs:381`
`self.expect_char('[')?`); there is no anonymous-relationship branch. `MATCH
(a)-->(b)` errors "Expected '['". Confirmed: `Match3[6]` `(a)-->(b:Foo)`, `[9]`
`(n)-->(a)-->(b)`, `[10]` `(a)-->(b), (b)-->(b)`. Related: `[:A|:B]` colon-prefixed
rel-type alternation fails "Expected identifier" (`parse_types`).

**Impact.** ~54 scenarios across `match`/`match-where`, **M** (a single parser
branch), workstream **W-E**. High-value because it is a *positive*-query gap (unlike
the negative-test mass) — it converts hard errors to passes directly.

**Confidence.** High.

---

## F-037 — `clauses/with-orderBy` (9%) is temporal-gated, not ordering-gated

**Evidence (265 fails).** setup-CREATE-failed 60 (temporal constructors + list/map
properties dying in the `CREATE` setup), row-not-found 77 (temporal value
rendering), error-expected-but-ok 67 (negative tests), row-count 33, column-name 10,
order-wrong **10**. Only ~10 of 265 are genuine ordering failures.

**Impact.** This directory **cannot materially improve without the temporal
subsystem** (W-D, `03-*`). Its headline number is a temporal proxy. The in-cluster
piece is list/map-valued properties in inline CREATE — `expression_to_json_value`
rejects non-scalar values (`create.rs`/`engine/match_exec.rs`), **S** (F-041-adjacent).

**Confidence.** High (measured).

---

## F-038 — ORDER BY on a non-projected expression is silently dropped; cross-type/NaN ordering

**Evidence.** `execute_sort` resolves sort keys by column-name string match and
**silently `continue`s past unresolved keys** (`operators/project.rs:594-599`) — it
never re-evaluates the expression against bindings, so `ORDER BY n.age + 2` or
`ORDER BY <non-projected>` is a no-op. Cross-type total order in
`compare_values_for_sort` (`project.rs:~690`) may diverge from openCypher's
(number < string < boolean < list < map < node < rel < path); `0.0/0.0` errors
("division by zero") instead of yielding NaN, killing mixed-type sort scenarios.
(Null positioning is already correct — `cypher_null_aware_order`, `project.rs:603/675`.)

**Impact.** `return-orderby` and the ~10 genuine `with-orderBy` ordering fails, **M**.
Depends on nothing else; overlaps the non-finite-float item (F-028) for the NaN case.

**Confidence.** High.

---

## F-039 — DELETE (0%) is a single result-shape bug — the highest-ROI fix in the write clusters

**Evidence.** `MATCH (n) DELETE n` with no RETURN returns a **1-row `count` result**
(`query_pipeline.rs:694-702` → `ResultSet::new(vec!["count"], [Row{[deleted_count]}])`),
but the TCK asserts `the result should be empty` (`Delete1[1][2][3]`;
`tck_opencypher.rs:156-164` requires `rows.is_empty()`). One row ≠ zero rows →
**uniform fail before side-effects are even compared.** DELETE itself is fully
implemented and counts `-nodes`/`-relationships` correctly
(`engine/match_exec.rs:11`, `crud/nodes.rs:420,544`).

**Impact.** Return an empty result for RETURN-less mutations → unblocks ~30+ of 41
delete scenarios (and delete-tail scenarios elsewhere). **S**, workstream **W-C**.
Secondary, surfacing after the row fix: `-labels` not counted on node delete;
error-kinds (`DeleteConnectedNode` → `ConstraintVerificationFailed`,
`DELETE n:Person` → `SyntaxError:InvalidDelete`); OPTIONAL-MATCH null-delete.

**Confidence.** High (runtime-proven).

---

## F-040 — SET (1.9%) is three stacked blockers

**Evidence.** (a) **Parse gap:** `parse_set_clause` emits only `Property`/`Label`/
`MapMerge(+=)`; `SET n = {map}` hits the else (`parser/clauses/write.rs:157-160`) →
hard parse error, killing all of `Set4`; same swallows `SET (n).prop`. (b)
**Counting:** `SET n.p = v` over an existing key does `+properties 1` only
(`write_exec.rs:1694`); the TCK counts an overwrite as `+properties 1` **and**
`-properties 1` (`Set1[1][2]`, `Set4[2][3]`). (c) **OPTIONAL MATCH rejected** in the
write path (`write_exec.rs:986-990`) → the "ignore null when writing" family fails.
The lone pass is the all-new-property shape (`Set1[11]`).

**Impact.** New `SetItem::Replace` variant + parser arm + engine apply (`M`, W-E),
overwrite `-properties` counting (`M`, W-C), OPTIONAL-in-write (`M`, shared with
delete/remove/merge). Recovers `set` from ~2% toward ~80%.

**Confidence.** High.

---

## F-041 — Side-effect counters exist and are wired; the divergence is counting *semantics*

**Evidence.** `SideEffects` has all 8 fields (`types.rs:161-189`), is a real field on
`ResultSet` (`:219`), and is populated for every clause family and stitched
unconditionally (`query_pipeline.rs:86-106`). The "0% because counters are absent"
hypothesis is **refuted**. What diverges (each fatal under exact `assert_eq!`,
`tck_opencypher.rs:218-228`): overwrite `+1/-1` (F-040); `+labels` counted per
(node,label) not once per statement (`record_store_ops.rs:587-588` uses
`count_ones()`, so `CREATE (:A),(:A)` counts `+labels 2`, TCK wants `1`); null-valued
keys counted (`record_store_ops.rs:552` uses `map.len()`, so `{id:12,name:null}`
counts `+properties 2`, TCK wants `1`).

**Impact.** A focused **M** "counting-semantics" pass in the create counters + SET/
REMOVE accumulators. Shared across create/merge/set/remove/delete (W-C). Land it
**before or with** the per-clause feature work so their scenarios can pass the exact
match. Missing compile-time errors (`VariableAlreadyBound`, `UndefinedVariable`,
`InvalidDelete`) are the **same W-A/A2 pass** as the read side (F-035).

**Confidence.** High.

---

## F-042 — CREATE / MERGE / REMOVE residuals, and why CALL is deprioritised

**Evidence.**
- **CREATE (18%):** 8 of `Create1`'s fails want `SyntaxError:VariableAlreadyBound`/
  `UndefinedVariable` (W-A/A2); `+labels` over-count (F-041); null-property over-count
  (F-041). Positive shapes pass.
- **MERGE (21%, best of the cluster):** find-or-create + `no side effects` on match
  work; fails on VariableAlreadyBound not detected (`Merge1[15]`), null-property
  error-kind (`Merge1[17]` wants `SemanticError`, Nexus raises `CypherExecution`,
  `write_exec.rs:681`), `MERGE p = (...)` path binding (`Merge1[13]`), `+labels`
  over-count.
- **REMOVE (18%):** `apply_remove_clause` consults only node `context`, never
  `rel_context` (`write_exec.rs:1830,1851`), so `MATCH ()-[r]->() REMOVE r.p` errors
  "Unknown variable 'r'" — a **S** fix (mirror the SET rel path). OPTIONAL rejected.
- **CALL:** 50 of 52 are harness-SKIP (procedure registration, F-007); the corpus has
  **zero `CALL { subquery }` scenarios** (all are `CALL proc()`). ~0 realistic
  conformance gain without procedure support, which is `L` engine work with low
  product value.

**Impact.** CREATE/MERGE/REMOVE ride the shared W-A/A2 + W-C workstreams plus small
per-clause fixes (rel-REMOVE routing `S`, path binding `S-M`). **Deprioritise
`clauses/call`** entirely for the conformance push.

**Confidence.** High.

---

## Plausible recovery (write clusters, if the shared workstreams land)

From the write-path audit: delete ~0→~90%, set ~2→~80%, remove ~18→~75%, create
~18→~65% (error-detection scenarios gated on W-A/A2), merge ~21→~70% — i.e. the ~221
failing write/mutation scenarios reduce to well under 50, with the **DELETE-row +
counting-semantics** pair (both W-C, both front-loadable) recovering the majority of
delete and a large share of set/remove on their own. Read-side recovery is gated on
**W-A/A2** (the negative-test mass) and **W-D temporal** (the with-orderBy proxy) and
is therefore back-loaded relative to the write quick wins.
