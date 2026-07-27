# 04 — Expression evaluation

Pure-expression categories: boolean, precedence, comparison, list, literals, string,
typeConversion, map, aggregation, graph, mathematical, null, conditional. ~570
failing scenarios — most concentrated in a handful of **contained** defects, not
broad subsystems. Instances of **W-A/A1** (F-009) + **W-B** (F-010) + **W-E**
(F-013). `expressions/conditional` is already **13/13 (100%)** and is the reference
for a correct value path (`eval/projection/core.rs:748`).

Two evaluators must be kept in lock-step throughout: `eval/projection/core.rs`
(RETURN/WITH/aggregate/filter) and `eval/predicate.rs` (WHERE).

---

## F-021 — The boolean 4% root cause: four named defects (highest-value single finding)

`expressions/boolean` = 150 scenarios, 6 pass, 144 fail. It is **not** a subsystem —
it is ~150 scenarios sunk by four precise defects in the logical-operator path:

1. **Operand type errors never raised (~117 scenarios, 78% — dominant).** Every
   `BooleanN.feature` ends with `Scenario Outline: Fail on … non-booleans` expecting
   `SyntaxError … InvalidArgumentType` (e.g. `Boolean1.feature:199-231`, `123 AND
   true`). Nexus evaluates AND/OR via `value_to_bool` (`core.rs:405-414`) which
   coerces `123→true` and **returns a value instead of erroring** → the harness
   panics "expected a SyntaxError but the query succeeded" (`tck_opencypher.rs:177-186`).
2. **3VL broken (null → false).** `value_to_bool` maps `Null → false`
   (`predicate.rs:490`), so `true AND null` → `false` (must be `null`); `null OR
   null` → `false`. Sinks `Boolean1/2 [1][2][3][5][7]`, `Boolean5 [2][4][6]`.
3. **`XOR` does not exist** anywhere — no AST variant, no lexing/precedence, no eval
   arm (`ast.rs:1286-1327`, `precedence.rs:11-44`, falls to `_ => Null` `core.rs:457`).
   `RETURN true XOR true` is a hard **parse error** → all of `Boolean3` (30) fail.
4. **`NOT null` → `true`.** `Not => Bool(!value_to_bool(v))` with `value_to_bool(null)
   = false` gives `true` (must be `null`) (`core.rs:463`). Sinks `Boolean4 [1]`.

**Decisive harness leverage.** The phase (compile vs runtime) is captured but **not
asserted** (`tck_opencypher.rs:173-176`), and `Error::CypherSyntax("InvalidArgumentType:
…")` classifies as `SyntaxError` and carries the token (`error.rs:240`). **A runtime
operand type-guard is therefore sufficient — no static type-checker needed for
boolean.** Fixing defects 1–4 as one coherent change flips boolean from ~4% to
near-100% and clears large slices of `precedence` and `comparison/null`.

**Confidence.** High (each defect anchored; the ~6 passes are exactly the null/XOR/
type-error-free interop laws).

---

## F-022 — Theme: "coerce instead of error/propagate" is the largest expression lever

**Evidence.** `value_to_bool` and `value_to_string` swallow null and wrong types
(`predicate.rs:485-494,563-572`). This one helper-pair mindset is behind boolean
operand errors (F-021), `STARTS/ENDS/CONTAINS` null-handling (`string[6]`s), `IN`
3VL (`core.rs:449`), list-slice null bounds (`core.rs:256,288`), ordering on null
(`predicate.rs:530-531`), and `toX` invalid-type handling (F-026).

**Impact.** Replacing coercion with 3VL + typed errors across both evaluators is
**~200+ scenarios** (boolean, typeConversion, string, map, precedence, list, null,
comparison). This is workstream **W-B** (F-010) and the highest scenario-per-effort
change available.

**Confidence.** High.

---

## F-023 — Parser precedence & literal breadth (workstream W-E within expressions)

**Evidence & sub-gaps** (all in `parser/expressions/`):
- **XOR** — add a precedence level OR < XOR < AND + AST variant + eval (F-021#3).
- **`^` power** sits in `parse_multiplicative_operator` left-assoc
  (`precedence.rs:472`); must bind tighter than `*`/unary-minus and be **right-assoc**
  (`2^3^2`, `-2^2`).
- **`%` sign** — `rem_euclid` (`arithmetic.rs:216,232`) gives `-3 % 2 → 1`; Cypher
  wants `-1`.
- **Comparison chaining** `a < b < c` — parser returns after one operator
  (`precedence.rs:151-265`).
- **Numeric literals** — `parse_numeric_literal` only handles decimal + `.`
  (`primary.rs:148-175`); `1e10`, `0x1A`, `0o17`, `1_000`, `.5` all fail to parse.
- **String escapes** — `parse_string_literal` handles only `\n\t\r\\`
  (`primary.rs:130-138`); `\uXXXX`/`\b\f\0`/hex escapes are lost.

**Impact.** Drives most of `literals` (76 fail) and `precedence` (86 fail) plus
parts of `string`, `math`, `comparison`. Many small `S`/`M` edits, one file.

**Confidence.** High.

---

## F-024 — Ordering comparisons on null/mixed types are wrong

**Evidence.** `<,<=,>,>=` route through `compare_values_for_sort` where `Null` sorts
`Less`/`Greater` (`predicate.rs:530-531`), so `null < 5` → `true` (must be `null`);
mixed-type comparisons stringify rather than following openCypher's type-order or
returning `null`.

**Impact.** `comparison`, `null`, and `precedence` scenarios. Part of W-B; the fix is
a null/unknown path in the comparison operators distinct from the sort comparator.

**Confidence.** High.

---

## F-025 — Small missing functions (pure additions, low risk)

**Evidence.** Missing: `reverse(string)` (only array reverse at `fn_list.rs:228`;
blocks `String3[1]`); `startNode(r)`, `endNode(r)`, `properties(entity)`
(`fn_graph.rs`, block `Graph2`/`Graph9`); `sign`, `rand`, `cot`, `haversin`
(`fn_math.rs`). `range()` accepts step=0 instead of erroring.

**Impact.** Each is **S** and unblocks `string`/`graph`/`math` scenarios directly
with near-zero risk — good early-momentum work.

**Confidence.** High.

---

## F-026 — typeConversion returns null instead of erroring; missing `*OrNull` variants

**Evidence.** `toInteger/toFloat/toString/toBoolean` (+`isInteger/isFloat`) exist
(`fn_list.rs:615-715`), but invalid types return `Null` instead of raising
`TypeError:InvalidArgumentValue`; `toBoolean(1.0)` wrongly succeeds (`:710`); the
`toIntegerOrNull`/`toFloatOrNull`/`toStringOrNull`/`toBooleanOrNull` variants are
absent.

**Impact.** `expressions/typeConversion` (27 fail). The invalid-type errors ride the
W-A/A1 runtime-guard path; the `*OrNull` variants are `S` additions.

**Confidence.** High.

---

## F-027 — Aggregation functions are complete; the gap is semantics + harness

**Evidence.** `operators/aggregate/core.rs` implements Count/CountStar, Sum, Avg,
Min, Max, Collect, **PercentileDisc, PercentileCont, StDev, StDevP** — all with
DISTINCT and null-skipping. The 2/35 pass rate is **not** missing functions: it is
(a) grouping/self-loop-count edges (`Aggregation1[2]`), null/DISTINCT edge semantics,
and (b) 13 scenarios skip-listed on **parameters** (F-007). Column naming (F-005)
also gates the un-aliased aggregation scenarios.

**Impact.** Do **not** budget aggregation as a "build functions" job. It is W-C
(naming) + W-F (parameters) + a small **M** of semantic edges. Re-measure after
those land.

**Confidence.** High.

---

## F-028 — Non-finite floats need a value-representation change (defer)

**Evidence.** `serde_json::Number` cannot hold `NaN`/`±Infinity`; `from_f64` errors
and `divide_values` errors on `/0` (`arithmetic.rs:76,195`). The harness matcher is
ready (`tck_common/mod.rs:249-272`) but **no Nexus value can match** because every
non-finite result becomes an error.

**Impact.** A handful of `math`/`literals` scenarios (and the `0.0/0.0` ordering
case in F-024). This is the **only genuinely architectural** expression item
(NaN-box or custom Number type) and has a low scenario count — **defer to last**
(W-D, F-012).

**Confidence.** High.

---

## Ordered sub-workstreams (expressions)

1. **[M, top yield] Logical 3VL + operand type-guards + XOR + null-ordering**
   (F-021+F-022+F-024, part of W-A/A1 & W-B) — flips boolean and a large slice of
   precedence/comparison in one coherent change.
2. **[M] Parser literals** (F-023: scientific/hex/octal/underscore ints, `\u`) —
   unlocks most of `literals` + part of `string`.
3. **[S-M] Precedence operators** (F-023: `^`, `%`, chaining) — finishes `precedence`.
4. **[S] 3VL for IN / STARTS-ENDS-CONTAINS / list-slice null** (F-022) — cheap
   string/list/comparison/null wins.
5. **[S] Missing functions** (F-025) — trivial, unblocks graph/string/math.
6. **[S-M] typeConversion errors + OrNull** (F-026) — rides the A1 error path.
7. **[M, harness] Wire parameters** (F-027/W-F) — exposes aggregation/map/math skips.
8. **[M] Aggregation semantics** (F-027) — after parameters exposes them.
9. **[L, defer] Non-finite floats** (F-028) — architectural, last.

**Highest scenario-per-effort: #1 and #2** — both Medium against the two largest
buckets, neither needing new infrastructure.
