# openCypher TCK — re-baseline at 48.8% and the road to the ceiling

> Re-baseline of the [docs/analysis/tck/](../tck/) plan after Phases 0–4 and the
> temporal chain landed. That plan was written at **13.2%**; the corpus now sits at
> **48.8%**, which invalidates its bucket sizes and its sequencing. This directory
> replaces the *estimates* in `../tck/07-execution-plan.md` with **measured counts**:
> every one of the 1908 current failures is attributed to a named root cause.

- **Baseline:** `docs/compatibility/OPENCYPHER_TCK_REPORT.md` — 3868 scenarios,
  **1889 pass (48.8%)**, 1908 fail, 71 skip.
- **Evidence:** `crates/nexus-core/target/tck-failures-orderby1.jsonl` (1908 rows,
  the log that produced the committed report), plus live probes of every root cause
  against a debug `nexus-server` built at commit `97214538`.
- **Reproduce:** `NEXUS_TCK=1 cargo +nightly test -p nexus-core --test tck_opencypher --all-features`

> **Baseline drift while this was written.** A concurrent session landed
> `phase21_tck-cross-type-comparison-semantics` in the same working tree, moving the
> report to **1891 pass (48.9%) / 1906 fail**. Every count in this directory is over the
> **1908-row log** named above. A 2-scenario delta changes no attribution and no
> ordering; see F-165 in `06-execution-plan.md` for where that task sits.

## Executive summary

**The 1908 failures collapse onto 23 root causes. Five of them are 47% of the corpus
gap.** Nothing here is a missing feature of the scale the first analysis feared — the
remaining work is a list of concrete, individually reproducible defects.

| # | Root cause | Fails | Band |
|---|---|---:|---|
| RC01 | List quantifier in a **standalone projection** returns 0 rows | **479** | S |
| RC03 | Temporal **construction from another temporal** (`date({date: d})`) ignored | **176** | M |
| RC11 | Functions return `null` instead of raising `TypeError`/`ArgumentError` | **103** | M |
| RC04 | Sub-second components (`millisecond`+`microsecond`+`nanosecond`) do not compose | **102** | S |
| RC20 | Missing negative-test validation (misc: functions, literals, DELETE, MERGE) | **96** | M |
| RC06 | No `VariableTypeConflict` / `VariableAlreadyBound` validation | **96** | M |
| RC13 | SKIP/LIMIT semantics (`LIMIT 0`, params, float, dropped before aggregation) | **89** | S |
| RC14 | **Projection list silently truncated** by postfix forms (`f(x).p`, `x IS NULL =`) | **~90** | M |
| RC19 | Parser: integer bounds, backtick identifiers, list slices | **78** | M |
| RC10 | Write queries run on a **restricted pipeline** (no OPTIONAL MATCH / WHERE / WITH) | **67** | L |
| RC07 | Parser: `p = pattern` only allowed as the first element of a pattern list | **66** | S |
| RC02 | Aggregation **grouping key dropped** unless it is a property access | **64** | S |
| RC22 | OPTIONAL MATCH partial bindings + var-length relationship list identity | **57** | L |
| RC16 | Ordering across types, lists, and temporal **instants** | **49** | M |
| RC05 | Week / quarter / ordinal date components | **47** | M |
| RC12 | No validation for ORDER BY on an aggregate or an out-of-scope variable | **46** | S |
| RC08 | Parser: **positive** pattern predicate in `WHERE` (only `NOT (…)` parses) | **34** | S |
| RC21 | Side-effect counting (MERGE `labels_added`, DELETE) | **28** | S |
| RC15 | Three-valued logic in list/nested comparison | **24** | S |
| RC18 | Column-name fidelity residue (measurable only after RC14) | 47 | S |
| RC09 | Named path variable unbound for fixed-length patterns | **14** | M |
| RC17 | Chained `WITH` alias rename **yields null** (correctness hazard, few TCK rows) | ~0 | S |
| RC23 | Temporal tail: `datetime.fromepoch`, ±999999999 years, duration borrow | 8 | M |

RC14 overlaps RC18/RC19 in the raw log (it manifests as `column-count mismatch` and
`column[N] name mismatch`); its ~90 is carved out of those buckets, so the column does
not sum to 1908 exactly. The full attributed table that does sum is in
[01-measurement.md](01-measurement.md).

### Three things the numbers say that the old plan did not

1. **One defect is 25% of the gap.** `RETURN none(x IN [1] WHERE x)` returns **zero
   rows**; `UNWIND [1] AS z RETURN none(x IN [1] WHERE x)` returns the right answer.
   A standalone projection whose quantifier predicate references the quantifier's own
   bound variable loses the row. That single behaviour is 479 scenarios — the whole
   reason `expressions/quantifier` reads 8.1%. It is an **S**, not the XL the category
   percentage implies (F-101).

2. **Two silent-failure modes are actively hiding conformance.** The projection-list
   parser **drops the remaining columns** instead of erroring (`RETURN f(r).a AS s, x AS e`
   → one column), and a failed clause parse can drop the whole `RETURN` (a query
   returns the *previous* `WITH`'s columns). Both mean the engine answers the wrong
   question without saying so — worse than failing (F-102, F-110).

3. **Temporal is no longer XL — it is three finite defects.** 334 fails are 27
   distinct scenarios: construction-from-another-temporal (176), sub-second component
   composition (102), week/quarter/ordinal components (47), tail (8). The typed-value
   subsystem the first analysis called the gate **is built and working**; what is left
   is component coverage (F-120).

### Honest ceiling

Attributing every failure does **not** make 100% reachable. Roughly 210 of the 1908 are
negative tests requiring an error-kind and detail-token taxonomy the spec itself
under-specifies, and ~70 (procedures, ±10⁹-year ranges, historical timezones) have no
product value here. **Phases A–D below plausibly land 85–92%**; the residue is the
same disputed tail the first analysis named, now with a real size.

## Index

| File | Theme |
|---|---|
| [01-measurement.md](01-measurement.md) | The re-baseline; failure-family taxonomy; the attributed table that sums to 1908; what changed since 13.2% |
| [02-projection-and-expressions.md](02-projection-and-expressions.md) | RC01, RC02, RC11, RC14, RC15, RC17, RC18 — the projection/evaluator cluster (**F-101–F-113**) |
| [03-temporal.md](03-temporal.md) | RC03, RC04, RC05, RC23 and the instant-ordering half of RC16 (**F-120–F-125**) |
| [04-read-clauses.md](04-read-clauses.md) | RC06, RC07, RC08, RC09, RC12, RC13, RC16, RC22 (**F-130–F-140**) |
| [05-write-clauses.md](05-write-clauses.md) | RC10, RC20, RC21 — the restricted write pipeline (**F-150–F-155**) |
| [06-execution-plan.md](06-execution-plan.md) | Sequenced plan, the 23 rulebook tasks, expected trajectory (**F-160–F-164**) |

## Method & confidence

Every root cause below was **reproduced live**, not inferred: a debug `nexus-server`
was driven with the minimal query from the failure log and the narrowing variants that
isolate the cause. Where a mechanism is named but not yet proven in source, it is
labelled **lead**, and the owning task's first checklist item is to confirm it.

- **Fail attribution:** high confidence (mechanical, over the committed log).
- **Root-cause direction:** high confidence (each reproduced from a minimal query).
- **Source mechanism:** stated per finding as *confirmed* or *lead*.
- **Recovery per task:** the attributed count is an **upper bound** — several
  scenarios are gated by two causes and will only flip when both land.
