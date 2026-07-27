# 02 — Cross-Cutting Workstreams (the spine)

Six workstreams that *most* failing scenarios share. Building these once pays off
across many categories at a time — they are where the conformance-per-effort ratio
is highest. The per-cluster files (03–06) hang their gaps off these.

Effort bands: **XS** ≈ hours · **S** ≈ days · **M** ≈ 1–2 weeks · **L** ≈ weeks ·
**XL** ≈ 1–2 months, one engineer.

---

## F-009 — W-A: Error semantics (the dominant lever) · **L→XL**

**What's missing.** The pipeline goes parser → planner → executor with **no
validation stage**. Nexus executes valid queries well but does not *reject* invalid
ones, and its error messages carry no openCypher detail token.

**Two distinct sub-capabilities** (do not conflate them):

- **A1 — Runtime operand type-guards + token emission** (**M**, cheap, high yield).
  Many "Fail when…" scenarios are catchable at eval time because the query would
  otherwise *succeed with a wrong value*. The harness does **not** assert the
  compile-vs-runtime phase (`tck_opencypher.rs:176`), and `Error::CypherSyntax(
  "InvalidArgumentType: …")` already classifies as `SyntaxError` and carries the
  token (`error.rs:240`). So a runtime guard in the evaluators is sufficient for:
  logical operands (`123 AND true`, F-021), `toInteger/toBoolean` of wrong types
  (F-026), list-predicate argument types (F-031), `length()` on non-paths.
- **A2 — Static semantic-analysis pass** (**L**, structural). Errors that cannot be
  caught at eval time because the query "succeeds" with wrong/empty rows need an
  AST scoping pass: `VariableAlreadyBound`, `UndefinedVariable`,
  `VariableTypeConflict` (node var reused as rel/path), `AmbiguousAggregationExpression`,
  `NestedAggregation`, aggregation-in-WHERE/ORDER BY, `NoExpressionAlias`,
  `ColumnNameConflict`, `DifferentColumnsInUnion`, `InvalidClauseComposition`,
  `NegativeIntegerArgument`/non-constant `SKIP`/`LIMIT`.

**Evidence.** `clauses/match` fails 172 "error-expected-but-ok" + 69 "token-missing"
(live run, `06-*`); `expressions/boolean` ~117 operand-type fails (F-021); the write
clusters need `VariableAlreadyBound`/`UndefinedVariable`/`InvalidDelete`
(`05/06-*`); `OpenCypherErrorKind` exists (`error.rs:286-305`) but `SemanticError`
is a **dead variant** never produced (`error.rs`), and no scoping pass exists.

**Reach.** match, match-where, return, return-skip-limit, union, with-where, set,
create, merge, delete, quantifier, path, typeConversion, map, boolean. Plausibly
**600–900+ scenarios** touch A1 and/or A2 (overlapping W-B and the error-token
gate F-006).

**Confidence.** High on direction; the A1/A2 split materially de-risks scope —
land A1 first (rides existing error infra), stage A2 by yield.

---

## F-010 — W-B: Three-valued (Kleene) logic & coercion discipline · **M**

**What's wrong.** Two helper functions silently swallow null and wrong types
instead of propagating/​erroring:
- `value_to_bool` maps `Null → false` and coerces `123→true`, `''→false`, `[]→false`
  (`eval/predicate.rs:485-494`); used by AND/OR/NOT (`core.rs:405-414,463`).
- `value_to_string` maps `Null → "null"` (`predicate.rs:563-572`); used by
  `STARTS WITH`/`ENDS WITH`/`CONTAINS`.

Consequences: `true AND null` → `false` (must be `null`); `NOT null` → `true` (must
be `null`); `x STARTS WITH null` → `false` (must be `null`); `x IN [null,…]` no-match
→ `false` (must be `null`, `core.rs:449`); list slice with null bound → `[]` (must be
`null`, `core.rs:256,288`); `null < 5` → `true` (must be `null`,
`predicate.rs:530-531`).

**The fix.** One coherent change: a shared 3VL logical helper returning `Bool|Null`
(else a typed error) wired into **both** evaluators — `eval/projection/core.rs`
(RETURN/WITH) and `eval/predicate.rs` (WHERE), which are parallel and must stay in
lock-step. Same discipline applied to comparison/IN/string-predicate/slice paths.

**Reach.** `expressions/boolean` (~27 direct + the operand-type overlap with A1),
`expressions/null`, `expressions/comparison`, big slices of `expressions/list`,
`expressions/precedence`, `expressions/string`, `expressions/map` — **~200–300
scenarios**, the **highest scenario-per-effort item in the codebase**.

**Compat risk.** The strict-typing half (A1) must not regress the Neo4j diff-suite
(300/300) or internal tests that lean on lenient truthiness. Keep WHERE-filter
truthiness (`null` → drop row); make only the logical **operators** strict. Gate on
the full suite.

**Confidence.** High.

---

## F-011 — W-C: Result-shape, column-name & side-effect fidelity · **S→M**

**What's wrong.** A cluster of *mechanical* correctness gaps where the value is
right but the *shape* is not — each fatal under the harness's exact matching:
- **Aggregate/expression column naming** — bare `"count"` vs verbatim `count(*)`
  (`strategy.rs:671…`, `expressions.rs:77-184`). See F-005. **S**, high leverage.
- **DELETE returns a phantom row** — `MATCH (n) DELETE n` (no RETURN) yields a
  1-row `count` result instead of empty (`query_pipeline.rs:694-702`); the TCK
  asserts empty. **S**, unblocks ~all of `clauses/delete` (F-039).
- **Side-effect counting semantics** — counters exist and are wired
  (`types.rs:161-189`, stitched at `query_pipeline.rs:86-106`) but diverge:
  overwriting a property should count `+properties 1` **and** `-properties 1`
  (Nexus counts only `+1`); `+labels` should count once per statement, not per
  (node,label) (`record_store_ops.rs:587`); null-valued keys should not count
  (`record_store_ops.rs:552`). **M** (F-041).

**Reach.** return, return-orderby, aggregation, literals (naming); delete, set,
create, merge, remove (result-shape + counting) — **~150–250 scenarios**. Several
are `S` quick wins with outsized payoff (DELETE row; aggregate naming).

**Confidence.** High (DELETE and naming runtime-proven; counting confirmed in
source).

---

## F-012 — W-D: Typed value model · **XL** (temporal) + **L** (non-finite, deferred)

**What's wrong.** The executor's currency is `serde_json::Value`
(`executor/types.rs:103-106`); there is no typed temporal value and no way to hold
non-finite floats. Temporals are faked as `String` (date/time) or `Object`
(duration) — and a returned duration is an `Object` that can **never** equal the
expected ISO `String` (`tck_common/mod.rs:117-153`), a guaranteed fail. Non-finite
floats (`NaN`, `±Infinity`) cannot be represented by `serde_json::Number`
(`arithmetic.rs:76,195`), so Nexus errors where the TCK expects a value.

**The fix.** For temporal: introduce a typed temporal representation and canonicalise
it to an ISO `String` at the projection boundary (recommended: a tagged-JSON-object
convention mirroring the existing `_nexus_id`/`_nexus_rel_type`/`_nexus_labels`
markers, canonicalised at RETURN; **must not leak** into the `/cypher` response —
that would be a real format regression). Full detail in `03-temporal.md`. Non-finite
floats need a genuine value-representation change (NaN-box or custom Number) — low
scenario count, deferred (F-028).

**Reach.** temporal ~935 + the temporal-gated slice of `with-orderBy` (~120) and
other setup failures; non-finite floats a handful in `math`/`literals`.

**Confidence.** High (the Object-vs-String wall is structural and proven).

---

## F-013 — W-E: Parser breadth · **M**

**What's missing (grammar, not semantics).** A set of un-parsed forms, each failing
its scenarios at parse time:
- **Bracket-less relationships** `-->`, `--`, `<--`: `parse_relationship_pattern`
  unconditionally requires `[` (`parser/clauses/pattern.rs:381`). Every `MATCH
  (a)-->(b)` errors. ~54 scenarios (F-036).
- **Quantifier `pred(x IN list WHERE cond)`** for `any/all/none/single`: only
  `filter()` and list comprehensions get the `IN…WHERE` special form
  (`parser/expressions/identifier.rs:78`); the four predicate names fall through and
  the dangling `WHERE` errors. Gates **~all 604** quantifier scenarios (F-031).
- **`XOR`**: absent from AST/lexer/precedence/eval (`ast.rs:1286`,
  `precedence.rs:11-44`) — `RETURN true XOR true` is a parse error (F-021/F-023).
- **`^` associativity/precedence** (right-assoc, tighter than `*`/unary-minus) and
  **`%` sign** (Cypher wants truncated remainder, Nexus uses `rem_euclid`)
  (`precedence.rs:472`, `arithmetic.rs:216`) (F-023).
- **Comparison chaining** `a < b < c` (`precedence.rs:151-265`) (F-023).
- **Numeric/string literals**: hex `0x`, octal `0o`, scientific `1e10`, underscore
  `1_000`, leading `.5`, and `\uXXXX`/`\b\f\0` string escapes
  (`parser/expressions/primary.rs:130-175`) (F-023).
- **`SET n = {map}`** whole-entity replace, `SET (n).prop`, `[:A|:B]` rel-type
  alternation (`parser/clauses/write.rs:157`, `pattern.rs` `parse_types`)
  (F-040/F-036).

**Reach.** match (~54), quantifier (~588 via one fix), literals (76), precedence
(86), string, set — **700+ scenarios**, spread across many `S`/`M` edits in
`parser/expressions/` and `parser/clauses/`.

**Confidence.** High (each anchor verified).

---

## F-014 — W-F: Harness & measurement · **XS→S**

**What to build.** (1) The Step-0 per-scenario failure log (F-004, **XS**, do
first). (2) The `Given "parameters are:"` step wiring the existing
`execute_cypher_with_params` (F-007, **S**, unlocks 62). (3) The `When "executing
control query:"` alias (**S**, unlocks up to 33). (4) The `binary-tree-N` fixtures
(**S-M**, feeds triadic selection). (5) Optionally, relax or extend the error-token
gate (F-006) once A1/A2 emit tokens.

**Reach.** Converts ~95 skips to real pass/fail cheaply and makes the whole metric
trustworthy and triageable. It is the enabling substrate for everything else, not a
scenario-recovery lever in itself (except parameters, which pass many immediately).

**Confidence.** High.

---

## Dependency shape (summary)

```
W-F (Step-0 log) ─── must precede accurate sizing of everything
   │
   ├── W-F (parameters step) ── unlocks 62 immediately
   │
W-A/A1 (runtime type-guards)┐
W-B (3VL helpers)           ├─ share the two evaluators; land together
   │                        │
W-A/A2 (static analysis pass)  ── independent structure; stage by yield
   │
W-C (naming / DELETE row / counting)  ── mostly independent quick wins
   │
W-E (parser breadth)  ── independent; each edit self-contained
   │
W-D (typed temporal)  ── the XL subsystem; gates temporal + with-orderBy
```

The high-yield, low-coupling front (W-F Step-0 + parameters, W-C naming + DELETE
row, W-B+A1 3VL/type-guards, W-E quantifier `IN…WHERE`) can land largely in
parallel and plausibly moves the needle from 13% into the mid-tier before the two
big investments (W-A/A2 and W-D temporal) begin. Sequenced in `07-execution-plan.md`.
