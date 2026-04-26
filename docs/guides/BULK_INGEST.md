# Bulk ingest with `CALL { … } IN TRANSACTIONS`

This guide covers the recommended pattern for streaming ingest into
Nexus using `CALL { … } IN TRANSACTIONS`. The clause batches outer
rows under a single executor invocation, applies a per-batch error
policy, and (optionally) emits a per-batch status stream the caller
can consume for monitoring.

## When to use it

- Importing large CSVs, JSON arrays, or any other row stream into
  the graph.
- Driving large `CREATE` / `MERGE` / `SET` / `DELETE` workloads
  whose total volume would otherwise exceed a single transaction's
  comfortable size.
- Running ETL jobs that need to keep going past individual row
  failures, or that want to retry transient errors.

## Anatomy of the clause

```cypher
UNWIND $rows AS row
CALL {
  WITH row
  CREATE (:Person {id: row.id, name: row.name})
}
IN TRANSACTIONS OF 1000 ROWS
REPORT STATUS AS s
ON ERROR CONTINUE
RETURN s.committed AS committed,
       s.rowsProcessed AS rows,
       s.err          AS err
```

The four optional suffix clauses can appear in any order:

| Suffix                          | Meaning                                                                                    |
|---------------------------------|--------------------------------------------------------------------------------------------|
| `OF N ROWS`                     | Batch size. Defaults to 1000.                                                              |
| `REPORT STATUS AS s`            | Emits one MAP-typed row per batch bound to `s`: `{started, committed, rowsProcessed, err}`. |
| `ON ERROR CONTINUE`             | A failing batch is logged + skipped; processing continues.                                 |
| `ON ERROR BREAK`                | A failing batch stops further processing cleanly.                                          |
| `ON ERROR FAIL` *(default)*     | A failing batch aborts the outer query.                                                    |
| `ON ERROR RETRY N`              | A failing batch is retried up to `N` times before escalating to FAIL.                      |

`REPORT STATUS AS <var>` and a `RETURN` clause inside the inner block
are mutually exclusive — Nexus rejects the combination at parse time
with `ERR_CALL_IN_TX_RETURN_WITH_STATUS`.

## Recommended patterns

**Idempotent inserts** — wrap inner writes in `MERGE` so retries
don't double-insert:

```cypher
UNWIND $rows AS row
CALL { WITH row MERGE (:Person {id: row.id}) SET p.name = row.name }
IN TRANSACTIONS OF 5000 ROWS
ON ERROR RETRY 3
```

**Best-effort ingest with auditing** — keep going past per-row
errors, capture them via REPORT STATUS:

```cypher
LOAD CSV WITH HEADERS FROM $url AS row
CALL { WITH row CREATE (:Customer {id: row.id, email: row.email}) }
IN TRANSACTIONS OF 1000 ROWS REPORT STATUS AS s
ON ERROR CONTINUE
RETURN s.committed, s.rowsProcessed, s.err
```

**Bulk delete** — `DETACH DELETE` is supported inside the inner block:

```cypher
MATCH (n:StaleEvent) WHERE n.ts < datetime() - duration({days: 90})
WITH n
CALL { WITH n DETACH DELETE n }
IN TRANSACTIONS OF 10000 ROWS
```

## Tuning

- **Batch size**: a lower bound around 100 keeps transaction
  overhead manageable; an upper bound around 50000 prevents any one
  batch from monopolising the writer. Start at 1000 and tune from
  there.
- **Retry budget**: retries are useful for transient errors
  (lock contention, transient I/O). Deterministic failures (bad
  data, schema violations) cannot be resolved by RETRY — they
  escalate to FAIL after exhausting the budget. Pair RETRY with
  CONTINUE if you want to keep ingesting past unrecoverable rows.
- **Concurrency**: `IN CONCURRENT TRANSACTIONS` parses today but
  the executor only supports the single-worker mode. Multi-worker
  subquery execution requires per-worker MVCC isolation, which
  ships with the V2 distributed branch — for now, parallelise at
  the application level (multiple client connections each running
  their own `IN TRANSACTIONS` job).

## Status row schema

When `REPORT STATUS AS s` is set, the operator emits one row per
batch whose single column is a MAP under the declared name:

```text
s.started        STRING (RFC-3339 datetime)
s.committed      BOOLEAN
s.rowsProcessed  INTEGER
s.err            STRING?  (NULL on success)
```

Downstream `RETURN`, `WHERE`, and projection expressions can access
the fields via property-access syntax:

```cypher
… RETURN s.committed AS ok, s.rowsProcessed AS n, s.err AS err
```

## See also

- `docs/specs/cypher-subset.md` — full grammar surface.
- `phase6_opencypher-subquery-transactions/design.md` — execution
  model + WAL/MVCC interactions.
