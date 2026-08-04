# Proposal: phase21_tck-semantic-validation-entry-point-coverage

> Part of the openCypher TCK 100%-conformance epic. Plan: docs/analysis/tck/.
> Surfaced during an independent review of the TCK conformance commits.

## Why
The semantic-analysis pass is wired into exactly one entry point.
`semantic_validation::validate` has a single call site in the whole engine —
`crates/nexus-core/src/engine/query_pipeline.rs:186`, inside
`execute_cypher_with_context` (the path that parses the query text itself).

The sibling entry point `execute_cypher_ast_with_params`
(`query_pipeline.rs:129`) exists to skip that re-parse for callers that already
hold an AST, and it calls the shared body `execute_cypher_ast_with_context`
directly — **bypassing the validation**. That pre-parsed path is what the
RPC/Thunder `CYPHER` dispatcher uses
(`crates/nexus-server/src/protocol/rpc/dispatch/cypher.rs`), and its read-only
autocommit fast path goes straight to the lock-free executor, never entering the
`Engine` wrapper at all. HTTP `/cypher` goes through
`execute_cypher_with_params` and *is* validated.

The result is transport-dependent semantics:

```
RETURN b            → UndefinedVariable over HTTP /cypher
RETURN b            → executes silently over the RPC CYPHER command
```

All six checks are affected — undefined variable, node/relationship variable
type conflict, CREATE re-declaration of a bound variable, misplaced aggregation,
SKIP/LIMIT arguments, duplicate projection alias. None of them are properties of
the engine today; they are properties of one entry point, and it happens to be
the one the conformance harness drives. Validation belongs where every executed
AST must pass through it, not where the text-parsing caller happens to sit.

This reads as an oversight (the perf-motivated pre-parsed entry point predates
the validation pass and was never updated), not a deliberate carve-out — but the
effect is the same: an SDK client over the binary transport gets no semantic
validation at all.

## What Changes
Move the `validate` call into the shared body `execute_cypher_ast_with_context`
so every path that executes an AST validates it first, and confirm the HTTP path
does not end up validating twice (the pass is pure, but it is not free).

Audit the RPC read-only fast path that calls the lock-free executor directly,
bypassing `Engine` entirely, and decide where it validates without paying a
re-parse. Add a transport-parity test so the two surfaces cannot drift again.

## Impact
- Affected specs: docs/specs/cypher-subset.md (semantic validation applies to
  every transport, not just HTTP)
- Affected code: crates/nexus-core/src/engine/query_pipeline.rs,
  crates/nexus-server/src/protocol/rpc/dispatch/cypher.rs
- Breaking change: YES, observably — semantically invalid queries that execute
  today over RPC will start returning errors. That is the intended correction,
  but it is a behavior change for SDK clients on the binary transport and should
  be called out in the changelog.
- User benefit: identical semantics across HTTP and RPC; a semantically invalid
  query is rejected the same way no matter which SDK or transport sent it.
