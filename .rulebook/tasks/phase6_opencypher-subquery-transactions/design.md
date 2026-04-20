# Subquery Transactions, Nesting, and `COLLECT {}` — Technical Design

## Scope

Close the remaining subquery-layer gaps: batched transactional
subqueries, nested CALL scoping, and full collect-subquery semantics.

## Grammar

```
call_in_transactions :=
    'CALL' '{' inner_query '}'
    ('IN' ('CONCURRENT')? 'TRANSACTIONS')?
    ('OF' INT 'ROWS')?
    ('REPORT' 'STATUS' 'AS' ident)?
    ('ON' 'ERROR' ('CONTINUE' | 'BREAK' | 'FAIL' | 'RETRY' INT))?
```

Example:

```cypher
LOAD CSV WITH HEADERS FROM 'file:///people.csv' AS row
CALL {
    WITH row
    CREATE (:Person {id: row.id, name: row.name})
} IN TRANSACTIONS OF 10000 ROWS
  REPORT STATUS AS status
  ON ERROR CONTINUE
RETURN status.started, status.committed, status.rowsProcessed, status.err
```

## Batched transaction operator

```rust
struct CallInTransactions {
    inner: Box<PlanNode>,
    batch_size: usize,
    concurrency: usize,       // 1 = serial
    on_error: OnError,
    status_var: Option<String>,
}

enum OnError {
    Fail,
    Continue,
    Break,
    Retry(u32),
}
```

### Execution (serial, batch_size = N)

```
buffer = []
for row in source:
    buffer.push(row)
    if buffer.len() >= N:
        tx = engine.begin()
        try:
            for r in buffer:
                inner_plan.execute(tx, r)
            tx.commit()
            status[batch] = Committed { rows: buffer.len() }
        except e:
            tx.rollback()
            handle_on_error(e, buffer, status)
        buffer = []
flush_remaining_buffer()
```

### Error handling matrix

| On-error mode   | On exception                                                        |
|-----------------|---------------------------------------------------------------------|
| `FAIL` (default)| rollback current batch, abort outer query, re-raise                 |
| `CONTINUE`      | rollback current batch, record error in status, clear buffer, go on |
| `BREAK`         | rollback current batch, emit collected statuses, stop source loop   |
| `RETRY n`       | rollback, retry with same buffer up to n times, then RETRY→FAIL     |

### Concurrent variant

`IN CONCURRENT TRANSACTIONS OF N ROWS` spawns a worker pool. Each
worker owns its own transaction and consumes batches from a shared
input channel:

```
input_rx → [worker_0 | worker_1 | ... | worker_k] → status collector
```

Workers are independent: no cross-worker communication, no shared
state. Writes must not rely on ordering across batches. Workers are
capped at `nexus.cypher.concurrency` (default 4).

### WAL boundary

Each batch writes one WAL segment boundary. Crash recovery re-runs
`OP_WAL_BATCH_COMMIT` entries in order; partial batches are rolled
back on recovery by the existing undo log.

## `REPORT STATUS` stream

When `REPORT STATUS AS status` is set, the operator emits one row
per batch downstream:

```rust
struct BatchStatus {
    started:         DateTime,
    committed:       bool,
    rows_processed:  u64,
    err:             Option<String>,
}
```

The outer query receives this as the `status` variable. When the
clause is absent, no status rows are emitted; the operator is purely
pass-through for input rows.

## Nested `CALL {}`

### Problem

Today the variable resolver uses a single flat scope, so a nested
`CALL { ... }` can accidentally see or shadow outer variables that
should be hidden. The openCypher TCK exercises ~40 scenarios where
this produces wrong results.

### Fix

Convert the scope stack to a true lexical tree:

```rust
struct Scope {
    parent: Option<ScopeId>,
    bindings: HashMap<String, Type>,
    import_list: Option<Vec<String>>,   // Cypher 25 CALL (vars) { ... }
}
```

Inner scope is "isolated" by default (Cypher 25 rule): the only
outer variables visible are those in the import list. The legacy
form `WITH ... CALL { ... }` imports every variable on the WITH
line.

### Error taxonomy

- `ERR_SHADOW_VARIABLE`: inner scope declares a name already in
  scope via import.
- `ERR_SCOPE_NOT_IMPORTED`: inner references an outer variable not
  in its import list.

## `COLLECT {}` subquery

Full grammar (Cypher 25):

```
collect_subquery := 'COLLECT' '{' query_with_return '}'
```

The inner query may be:

- A simple `MATCH/RETURN` (simple case, already works).
- An aggregating `RETURN count(...)` (new case).
- A multi-column `RETURN a, b` (returns a list of maps).

Evaluator rule: collect all rows emitted by the inner plan; produce
a LIST value where:

- Single-column inner → LIST<T> of that column.
- Multi-column inner → LIST<MAP> mapping column name → value.

Aggregating RETURN: the inner plan runs once to completion (all rows
aggregated), producing exactly one row; the outer sees a
single-element LIST. This matches Neo4j 5.9+.

## Metrics

New Prometheus counters:

```
nexus_call_in_tx_batches_total{status="committed|failed|retried"}
nexus_call_in_tx_rows_total{status="committed|failed"}
nexus_call_in_tx_retry_attempts_total
```

## Configuration

```toml
[cypher]
default_batch_size    = 1000
concurrency           = 4
max_retry_attempts    = 5
```

## Benchmarks (targets)

| Scenario                                    | Target                   |
|---------------------------------------------|--------------------------|
| 1M-row CSV ingest via IN TRANSACTIONS       | ≥ 15k rows/sec/thread    |
| Concurrent ingest, 4 workers                | ≥ 50k rows/sec aggregate |
| Nested CALL (3-deep) overhead               | ≤ 1.1× flat plan         |
| `COLLECT {}` over 10k inner rows            | ≤ 20 ms p95              |

## Out of scope

- Distributed transactional subqueries across shards — V2.1 task.
- `CALL { }` as an expression (scalar subquery) — Cypher 25 spec
  defines it but flagged as draft; V1.1 item.
- `WITH * ` in nested scopes — tracked separately as a scoping clean-up.

## Rollout

- v1.3.0 ships subquery transactions and fixes nested scoping.
- Compatibility: existing `CALL { }` without `IN TRANSACTIONS` is
  unchanged.
- Neo4j diff harness extended with ~40 subquery scenarios.
