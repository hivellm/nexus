# 8. Delete the unused JIT scaffold rather than finish the Cranelift codegen

**Status**: proposed
**Date**: 2026-04-28

## Context

`crates/nexus-core/src/execution/jit/` was carrying ~1320 lines of half-implemented JIT scaffold (mod.rs + codegen.rs + runtime.rs) plus a 173-line `cranelift_jit.rs.disabled` shadow file. The module exported `JitRuntime` + `QueryHints` through `execution::mod.rs::pub use jit::{JitRuntime, QueryHints}`, contained 12 `// TODO`-style markers (most under `// TODO: Re-enable after core optimizations`), and the executor's only call site (`executor/mod.rs:92 use crate::execution::jit::CraneliftJitCompiler`) had been commented out. Audit confirmed: no production caller anywhere in the workspace touches `JitRuntime` or `QueryHints` — `grep -rn` across `nexus-server`, `nexus-cli`, `nexus-protocol`, `nexus-bench`, and the integration test suite returned zero hits. The module's only consumers were its own unit tests. This violated the project's own Tier-1 rule (`AGENTS.md` §1: no TODOs / stubs / partial implementations).

## Decision

Delete the JIT module entirely (Option B from the `phase7_resolve-jit-module` proposal) instead of finishing the Cranelift codegen (Option A). Remove `crates/nexus-core/src/execution/jit/` and the `.disabled` shadow, drop the `pub use jit::{JitRuntime, QueryHints}` re-export from `execution/mod.rs`, and clean the commented-out executor import.

## Alternatives Considered

- Option A: finish the Cranelift codegen for all physical operators (NodeByLabel / Filter / Expand / Project / OrderBy+Limit / Aggregate / SpatialSeek), wire planner integration, prove ≥1.5× hot-path speedup vs the interpreter, add proptest parity. Estimated ~3 weeks. Rejected because (a) no concrete near-term consumer asked for compiled queries, (b) the columnar fast-path real-world ratio is already ~1.13× per `PERFORMANCE_V1.md` honesty doc — the gain a JIT would deliver is dominated by materialisation cost, not by interpreter overhead, (c) the planner lacks cardinality propagation (per `docs/analysis/nexus/02_architecture_assessment.md` §4) which is the higher-leverage perf work.

## Consequences

Loses ~1500 LOC of dead code, two `// TODO` markers from the public surface, and one Tier-1 rule violation. No public-API breakage (the re-exports were unused). Nexus continues to ship interpreted-only execution — no perf regression because the JIT path was never live. If a future task wants compiled queries, history is preserved in `.rulebook/decisions/` and the deleted-file diff in commit history; the next attempt can either restart from scratch or replace with a simpler bytecode VM.</consequences>
<parameter name="relatedTasks">["phase7_resolve-jit-module"]
