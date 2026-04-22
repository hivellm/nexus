# Transaction Savepoints

Nexus 1.5 ships SQL-style savepoints for Cypher transactions
(`phase6_opencypher-advanced-types §5`). A savepoint is a named
marker inside an in-flight transaction that the caller can later
rewind to without aborting the transaction itself.

## Statements

| Statement                         | Effect                                                                  |
|-----------------------------------|-------------------------------------------------------------------------|
| `SAVEPOINT <name>`                | Push a marker on the current transaction's savepoint stack.             |
| `ROLLBACK TO SAVEPOINT <name>`    | Undo every mutation since `<name>` was pushed. Keeps `<name>` active.   |
| `RELEASE SAVEPOINT <name>`        | Pop `<name>` (and any inner markers). No undo.                          |
| `ROLLBACK` *(no `TO SAVEPOINT`)*  | Abort the whole transaction (unchanged pre-existing behaviour).         |

All three savepoint statements require an active explicit transaction
(`BEGIN TRANSACTION`). Issuing them outside one raises
`ERR_SAVEPOINT_NO_TX`.

## Example

```cypher
BEGIN TRANSACTION;
CREATE (:Person {name: "Alice"});
SAVEPOINT after_alice;
CREATE (:Person {name: "Bob"});
CREATE (:Person {name: "Carol"});
ROLLBACK TO SAVEPOINT after_alice;   -- Bob and Carol vanish
CREATE (:Person {name: "Dave"});      -- lands on top of Alice
COMMIT;
-- result: Alice and Dave committed; Bob and Carol never existed.
```

## Nested savepoints

Savepoints stack. Rolling back to an outer savepoint implicitly
discards every inner savepoint and its work:

```cypher
BEGIN TRANSACTION;
CREATE (:X {v: 1});
SAVEPOINT a;
CREATE (:X {v: 2});
SAVEPOINT b;
CREATE (:X {v: 3});
ROLLBACK TO SAVEPOINT a;              -- v=2 and v=3 undone; a still active.
COMMIT;                               -- only v=1 survives.
```

Duplicate savepoint names are allowed. Rollback and release both
resolve the name by picking the most-recent matching marker
(LIFO), matching PostgreSQL semantics.

## Error codes

| Code                     | When                                                 |
|--------------------------|------------------------------------------------------|
| `ERR_SAVEPOINT_NO_TX`    | Any savepoint statement outside an explicit tx.      |
| `ERR_SAVEPOINT_UNKNOWN`  | `ROLLBACK TO` / `RELEASE` with a name not on stack.  |

## WAL visibility

Savepoints are purely in-memory. A transaction that uses savepoints
and finally commits produces a WAL entry indistinguishable from a
transaction without savepoints — by commit time the undo log already
reflects only the surviving mutations. There is no way to recover a
rolled-back savepoint from the WAL, and there is no reason to
try — the whole point of a savepoint is that it rewinds the caller's
view before commit.
