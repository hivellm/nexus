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

### What is NOT yet known

**The minimal repro has not been isolated** — the bisect was cut short, so the exact
trigger is unconfirmed. Do not skip §1. The shapes to separate, in order:

1. comma-separated `MATCH (a), (b)` alone (a cartesian product whose estimated
   cardinality may be pre-multiplied into a capacity)
2. the same with `CREATE` attached
3. the same driven by `UNWIND` over parameters
4. whether the allocation size tracks node count, `$rows` length, or their product

The working hypothesis is a capacity derived from an estimated cartesian cardinality
and handed to `Vec::with_capacity` with no ceiling. That is a hypothesis, not a
finding; §1 exists to replace it with evidence before any code changes.

## What Changes

- Isolate the minimal repro and identify the allocation site from a backtrace
  (`RUST_BACKTRACE=1`, which the abort message itself suggests).
- Fix the cause — most likely bound or remove an allocation sized from an
  unvalidated cardinality estimate.
- Add a defensive ceiling so no planner estimate can be turned directly into an
  unbounded allocation: a query that would exceed a configured memory budget must
  return a typed error, never abort the process.
- Audit sibling call sites for the same pattern (`with_capacity` / `reserve` fed by
  estimated rather than actual counts).

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
