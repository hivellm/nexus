# Write-Path Unification — Architecture Analysis & Migration Plan

> **Date**: 2026-07-11 · **Analyzed version**: 2.4.0 · **Author**: architecture
> review for the 2.5.0 release.
>
> Part of the [Nexus 2.5.0 competitive analysis](README.md).

## TL;DR

The "dual write path" is actually **five** divergent write implementations,
string-routed per transport. The correct, unit-tested implementation already
exists — `Engine::execute_cypher_with_params` → `execute_cypher_dispatch` →
`execute_write_query` (`crates/nexus-core/src/engine/write_exec.rs`, ~1,500
lines). The fix is to make **every transport a thin adapter over that one
engine entry point** and delete the forks. This single change eliminates bug
class B1–B8 (see [02-bug-inventory.md](02-bug-inventory.md)) and prevents its
recurrence.

## Current state map

```
                       nexus_core::Engine (embedded API)
   execute_cypher(&str)               ← params DROPPED (footgun)
   execute_cypher_with_params ────────← CORRECT, params kept
        └─ execute_cypher_dispatch
             ├─ DELETE           → execute_match_delete_query
             ├─ MERGE/SET/REMOVE/FOREACH → execute_write_query   (write_exec.rs — GOOD)
             ├─ CREATE           → execute_match_create_query / executor
             └─ reads            → Executor::execute
   execute_cypher_ast  ← DUPLICATE dispatch (PROFILE path drift risk)

  HTTP :15474          RPC :15475            RESP3            GraphQL          Streaming MCP
  handler.rs           rpc/dispatch          resp3/command    mutation.rs      streaming/handlers.rs
  string-prefix        AST predicate         engine, but      raw executor:    hand-rolled CREATE,
  routing:             (needs_engine_        params IGNORED   MERGE→MATCH stub literals only
  CREATE|MERGE ──►     interception)         (`_params`       SET/REMOVE/
  write_ops.rs         engine, but           unused)          FOREACH silently
  (1,109-line fork)    execute_cypher                          ignored
                       WITHOUT params
```

**Executor operator reality**: the executor has real `Create`/`Delete`/
`DetachDelete` operators, but the planner stubs `Clause::Merge` as a plain
MATCH (`planner_core.rs:415`) and has **no** Set/Remove/Foreach operators.
That is why the engine grew `write_exec.rs` — and why any transport calling
the raw executor for writes silently loses data.

## Root cause

The executor's planner was only ever taught `Create` and `Delete`. To ship
MERGE/SET/REMOVE/FOREACH, the engine grew a parallel AST-walking write
interpreter (`write_exec.rs`), and `execute_cypher_dispatch` became the true
handles-everything entry point. The HTTP handler, however, was written
against the engine's low-level CRUD methods with string-prefix routing before
that dispatch matured — that fork is `write_ops.rs`. Newer transports wired
to `engine.execute_cypher` but nobody retired the HTTP fork or reconciled
parameter threading. Every HTTP-only bug is `write_ops.rs` re-implementing —
incompletely — what `write_exec.rs` already does correctly.

## Target architecture

- **Single entry point**: `nexus_core::Engine::execute_cypher_with_params`
  (module: `crates/nexus-core/src/engine/query_pipeline.rs`). Optionally a
  typed façade `Engine::execute(QueryRequest { cypher, params, user_ctx,
  isolation })` so future fields don't churn call sites.
- **Transports do three things**: decode envelope → call engine → encode
  `ResultSet`. No parsing, no clause inspection, no direct CRUD. The one
  legitimate transport-level branch is admin/DB-management (DatabaseManager,
  RBAC) via the shared `route_admin(ast)` helper RPC already factored.
- **Cross-cutting concerns at the boundary**: audit logging becomes an
  engine-level write hook or a single adapter wrapper (today it lives ONLY
  inside `write_ops.rs` — deleting the fork without relocating it would be a
  compliance regression).
- **Reads stay lock-free**: no-write-clause queries keep the
  `executor.clone()` + `spawn_blocking` path; only writes take
  `engine.write().await` (same as today — no new serialization).
- **Post-2.5.0 epic (own ADR)**: promote MERGE/SET/REMOVE/FOREACH to real
  planner operators so the executor becomes the single pipeline and
  `write_exec.rs` collapses into operator implementations.

## Migration plan (each step independently shippable)

> Rule: **no fork is deleted before the parity harness (Step 0) is green.**

| Step | What | Deletes | Effort |
|---|---|---|---|
| **0** | **Parity test harness** — query battery (CREATE node/rel ± RETURN projections, MERGE node/rel ± ON CREATE/ON MATCH, SET all forms incl. `r.*`, REMOVE, DELETE/DETACH, `$params`, UNWIND+MERGE, MATCH+CREATE, in-transaction) run through write_ops AND engine; diff responses AND side-effects; document every divergence | nothing | M |
| **1** | **Thread params through RPC + RESP3** — `rpc/dispatch/cypher.rs:273` and `resp3 run_cypher` → `execute_cypher_with_params` | nothing | S |
| **2** | **HTTP: route CREATE/MERGE to the engine** — handler.rs write branch → same `execute_cypher_with_params` call the MATCH/UNWIND branches use; **move audit logging to the adapter wrapper first** | nothing yet | M |
| **3** | **AST-predicate routing** — delete string heuristics; lift RPC's `needs_engine_interception` into shared `api/cypher/routing.rs`, used by HTTP + RPC; routing unit-test table (mixed queries, comments, lowercase) | string heuristics | M |
| **4** | **Delete `write_ops.rs`** (1,109 lines) — also removes the `delete_rel().unwrap()` panic | the fork | S |
| **5** | **GraphQL mutations → engine** — mutation.rs + mutating resolvers call `execute_cypher_with_params`; read resolvers stay on lock-free executor | MERGE-as-MATCH silent loss | M |
| **6** | **Streaming MCP → engine** — delete literal-only CREATE loop in `streaming/handlers.rs` | 5th fork | S/M |
| **7** | **Collapse engine dispatch duplication** — unify `execute_cypher_dispatch` + `execute_cypher_ast` into one private `dispatch(ast, query, opts)`; deprecate params-dropping `execute_cypher(&str)` (rename or take params) | internal 3rd fork + footgun API | M |
| **8** | *(post-2.5.0, own ADR)* Executor-native Merge/Set/Remove/Foreach operators; `write_exec.rs` → operators | write_exec as separate path | L |

Steps 1–7 are git-revert reversible. Step 8 is the only hard-to-reverse
change — gate behind its own ADR.

## Risks

| # | Risk | Mitigation |
|---|---|---|
| 1 | Clients depend on write_ops quirks (MERGE match semantics differ: naive all-props scan vs engine's `find_nodes_by_node_pattern` + Neo4j null-key rejection; `CREATE…RETURN n` object shape) | Step 0 harness enumerates every divergence; engine/Neo4j-correct behavior wins; CHANGELOG documents intentional changes; compat suite updated |
| 2 | Write-throughput regression fear | Both paths already take the same `engine.write().await` + `flush_async` — no new serialization. Add before/after write bench as the gate |
| 3 | Transaction semantics drift (`BEGIN…CREATE…ROLLBACK` over HTTP) | Parity battery includes in-transaction section; switching actually fixes the `.unwrap()` panic and unifies tx behavior |
| 4 | RETURN projection surface changes (`CREATE…RETURN <expr>` that returned null may now return values or error) | Dedicated RETURN-shape block in harness; document under "Fixed" |
| 5 | **Audit-log coverage regression** (only write_ops emits write audits today) | Blocking sub-task in Step 2: relocate audit before Step 4 deletes the fork; audit-emission test in harness |

## Key files

- Fork to delete: `crates/nexus-server/src/api/cypher/execute/write_ops.rs`
- HTTP router: `crates/nexus-server/src/api/cypher/execute/handler.rs`
- Canonical entry point: `crates/nexus-core/src/engine/query_pipeline.rs`
- Source of truth: `crates/nexus-core/src/engine/write_exec.rs`
- RPC adapter: `crates/nexus-server/src/protocol/rpc/dispatch/cypher.rs`
- RESP3 adapter: `crates/nexus-server/src/protocol/resp3/command/cypher.rs`
- GraphQL: `crates/nexus-server/src/api/graphql/mutation.rs`, `resolver.rs`
- Streaming MCP: `crates/nexus-server/src/api/streaming/handlers.rs`
- Executor operator gap (Step 8): `crates/nexus-core/src/executor/operators/dispatch.rs`, `crates/nexus-core/src/executor/planner/queries/planner_core.rs`
