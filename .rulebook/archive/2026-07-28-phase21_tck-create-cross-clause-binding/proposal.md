# Proposal: phase21_tck-create-cross-clause-binding

## Why
Consecutive CREATE clauses in one query did not share variable bindings:
`CREATE (a:A), (b:B)` followed by `CREATE (a)-[:T]->(b)` minted fresh anonymous
nodes (5 where 3 are expected) and wired relationships onto the duplicates.
Most openCypher TCK fixtures build their graphs with exactly this multi-clause
CREATE idiom, so the bug silently corrupted scenario setup across many
categories — capping expressions/pattern despite pattern comprehensions being
correct, and failing clauses/create scenarios directly.

## What Changes
Make later CREATE clauses reuse variables bound by earlier CREATE/MATCH clauses
in the same query. Dual root cause, both fixed: the executor's standalone-CREATE
fast path (the path plain multi-clause CREATE actually routes through) threads
one binding accumulator across all Create operators; the engine write-clause
loop (CREATE mixed with MERGE/SET/REMOVE/FOREACH) consults the persistent
context for a variable bound to exactly one id before creating. Unbound names
still create fresh entities; MERGE/UNWIND semantics untouched.

## Impact
- Affected specs: docs/specs/cypher-subset.md CREATE semantics
- Affected code: executor/dispatch/operator_loop.rs, engine/write_exec/dispatch.rs, operators/path.rs (constant visibility), regression tests
- Breaking change: NO (previous behavior was simply wrong)
- User benefit: multi-clause CREATE builds the intended graph; TCK fixtures stop being corrupted at setup (clauses/create 24→25 then 34/78 with the follow-up phantom-row fix; Create3[13] passes end-to-end).

(Backfilled after archive — the task was executed and completed in commit 26fb15a4.)
