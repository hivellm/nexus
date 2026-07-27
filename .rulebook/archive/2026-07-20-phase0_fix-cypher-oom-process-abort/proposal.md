# Proposal: phase0_fix-cypher-oom-process-abort

**Priority: CRITICAL — a single Cypher query aborts the entire server process.**
Found while implementing `phase7_ldbc-snb-benchmark` item 1.3 (LDBC SNB bulk loader);
not previously reported and not tracked by any GitHub issue. This is an availability
bug reachable by anyone who can send a query, and authentication is disabled by
default for localhost binds.

## Why

A legitimate, unremarkable bulk-load query makes the server attempt a **~3.6 TiB**
allocation and die. There is no error response, no rollback, no recovery — the
process aborts and every other connected client loses its session.

### Confirmed empirically (Nexus 2.5.0, release build)

Setup: 5 000 `:P` nodes and 5 000 `:Q` nodes, with a property index on `:P(id)`.

```
UNWIND $rows AS r
MATCH (a:P {id: r.s}), (b:P {id: r.d})
CREATE (a)-[:KNOWS]->(b)
```

with `$rows` = 5 000 `{s, d}` pairs.

The HTTP connection is reset mid-request and the process is gone:

```
ConnectionResetError: [WinError 10054]
```

The server log contains exactly one line, then nothing:

```
memory allocation of 4000000000000 bytes failed
```

`4_000_000_000_000` bytes is ~3.6 TiB. Nothing in the query justifies that: the
input is 5 000 parameter rows over a 10 000-node graph. Note the number is suspiciously
round, which points at a computed capacity rather than genuine accumulated data.

### Mechanism — CONFIRMED (2026-07-19), original hypothesis REFUTED

The proposal originally guessed "a capacity derived from an *estimated* cardinality".
**That was wrong.** The estimate is not wrong — it is exact. The executor genuinely
tries to materialize the entire cartesian product in memory.

Shape bisect, 5 000 `:P` nodes, 5 000 param rows, fresh release server per case:

| # | Shape | Result |
|---|---|---|
| a | `MATCH (a:P {id:1}), (b:P {id:2}) RETURN …` | ok |
| b | a + `CREATE (a)-[:R]->(b)` | ok |
| c | `UNWIND` + comma MATCH + `RETURN` | **OOM, 4 000 000 000 000 bytes** |
| d | `UNWIND` + comma MATCH + `CREATE` | **OOM, same size** |
| e | `UNWIND` + single-pattern MATCH + `CREATE` | ok (141 s) |
| f | `UNWIND` + chained `(a)-[:R]->(b)` | ok (174 s) |

So the minimal trigger is **`UNWIND` + a comma-separated multi-pattern `MATCH`**.
`CREATE` is irrelevant — case (c) aborts without it.

**Allocation site**: `crates/nexus-core/src/executor/eval/helpers.rs:101`, in
`apply_cartesian_product`:

```rust
let mut new_arr = Vec::with_capacity(arr.len() * new_count);
```

`ExecutionContext.variables` holds each bound variable as a **fully materialized
columnar `Vec<Value>`**. Every new pattern multiplies every existing column:

1. `UNWIND` → `r` = 5 000 rows
2. `MATCH (a:P …)` → scan yields 5 000 candidates → every column becomes 5 000 × 5 000 = 25 000 000
3. `MATCH (b:P …)` → 5 000 candidates → every column becomes 25 000 000 × 5 000 = **1.25e11**

`1.25e11 × 32` (the size of `serde_json::Value` on x86-64) `= 4.0e12` — matching the
logged figure exactly. The number is round because the inputs are round, not because
it is a sentinel.

`with_capacity` is only where it dies first; removing it would merely move the abort
into the `push` loop at `:104`, which also clones each node value N×M times.

### Why the product is astronomically large: correlated predicates never seek

The deeper cause is that `{id: r.s}` — a property predicate whose value comes from
the `UNWIND` row — does **not** use the property index. Measured on 3 000 `:P` nodes
with an index on `:P(id)`:

- constant `MATCH (a:P {id: 42})` → indexed, instant
- correlated `UNWIND $rows AS r MATCH (a:P {id: r.s})` → **30 rows/s**, and scaling
  **superlinearly**: 200 rows took 6.7 s, 400 rows took 22.7 s (3.4× for 2× the input)

The scan returns *every* `:P` node per row and the predicate is applied after the
cross product, instead of one index seek per row. That is what turns a 5 000-row
load into a 1.25e11-cell table. It is a distinct defect and is filed separately as
`phase0_fix-correlated-predicate-index-seek`; this task is scoped to making the
abort impossible.

## What Changes

Scope: **make the abort impossible.** Streaming the cartesian product instead of
materializing it is the architecturally correct answer, but it is a large executor
refactor; making a legitimate query fast again is `phase0_fix-correlated-predicate-index-seek`.
A query that asks for more memory than exists must fail as a query, not take the
server with it.

- Bound the product in `apply_cartesian_product` before allocating: reject with a
  typed, catchable error when `current_count × new_count` exceeds a budget, and use
  checked multiplication so the count itself cannot overflow.
- Express the budget in bytes rather than rows, since the cost is
  `rows × size_of::<Value>() × columns`, and make it overridable for operators who
  knowingly want a larger product.
- Audit sibling call sites that size an allocation from a product of counts rather
  than from data actually in hand.

## Impact

- Affected specs: none directly; changes the executor's memory-safety behaviour
- Affected code: `crates/nexus-core/src/executor/` — the operator that materializes
  a multi-pattern MATCH, plus whichever site the §1 bisect identifies
- Breaking change: NO — queries that previously killed the process will return an
  error instead
- User benefit: the server stops being killable by one query; bulk-load workloads
  that combine `UNWIND` with a multi-pattern `MATCH` become usable
- Blocks: `phase7_ldbc-snb-benchmark` item 1.3 — this is the natural shape for
  loading SF0.1's 576 896 edges by LDBC id, so the loader must currently avoid it
