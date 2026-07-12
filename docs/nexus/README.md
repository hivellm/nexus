# Nexus Competitive Analysis & 2.5.0 Plan

> **Date**: 2026-07-11 · **Analyzed version**: 2.4.0 · **Branch**: release/2.5.0
>
> Four-track analysis (compatibility, bugs, performance, architecture) toward:
> 100% practical openCypher/Neo4j compatibility, superior performance, and
> zero known silent-failure bugs — making Nexus competitive in the graph-DB
> segment.

## Documents

| Doc | Contents |
|---|---|
| [01-compatibility-gaps.md](01-compatibility-gaps.md) | Code-verified openCypher 9 / Neo4j 5.x feature inventory: ~85% parity; supported / partial / missing with effort+priority |
| [02-bug-inventory.md](02-bug-inventory.md) | 8 confirmed bugs (4 data-loss class), 5 latent, structural bug factories, elimination map |
| [03-performance.md](03-performance.md) | Execution-model review, audited benchmark numbers (incl. contradictions), top-8 bottleneck ranking, competitive positioning vs Neo4j/Memgraph/Kùzu/FalkorDB |
| [04-write-path-unification.md](04-write-path-unification.md) | The five-fork write-path problem, root cause, target architecture, 8-step migration plan with risks |
| [05-v2.5.0-plan.md](05-v2.5.0-plan.md) | Synthesis: goals, rulebook task map (phases 1–8), deferred 2.6 epics, risk register |

## Executive summary

**Where Nexus actually stands** (better than the docs claim): ~85% openCypher
/Neo4j feature parity, 75+ functions, all index families (bitmap, B-tree,
spatial, full-text, HNSW), MVCC + savepoints, 30+ GDS procedures, real SIMD.

**The three things holding it back:**

1. **Transport-dependent correctness.** Five divergent write implementations
   mean `MERGE (a)-[r]->(b)` silently creates nothing over HTTP, GraphQL
   mutations silently ignore SET, and RPC drops `$params` — while the
   embedded engine does all of it correctly. One tested write path exists
   (`engine/write_exec.rs`); every transport must become a thin adapter over
   it and the forks deleted.
2. **A global engine lock** serializes nearly all real queries onto one core.
   The lock-free executor path exists — it just isn't reached by MATCH
   queries. Fixing routing is the single highest-leverage perf change
   (comparable class of fix already yielded 3.7x).
3. **An unpublishable benchmark story.** Contradictory CREATE-rel numbers
   (87.6% slower vs 42.7x faster), no KNN recall curves, headline gaps in
   traversal (41–57% slower) and aggregation (COUNT 44.7% slower) with known
   cheap fixes.

**2.5.0 in one sentence**: unify the write path, unlock concurrent reads,
close the traversal/aggregation gaps, re-baseline and publish honest
benchmarks, trim the compatibility tail — then 2.6 takes Bolt and columnar
execution.

## Task tracking

Implementation tasks live in `.rulebook/tasks/` (phases 1–8), created from
[05-v2.5.0-plan.md](05-v2.5.0-plan.md). Execute strictly in phase order; the
parity harness (phase 1) is the safety net for everything that follows.
