# Proposal: phase7_resolve-jit-module

## Why

`crates/nexus-core/src/execution/jit/mod.rs` ships a Cranelift-based JIT compiler scaffold where ~80 % of the operator codegen is `// TODO` placeholder comments (e.g. `// TODO: Generate actual node creation code`, `// TODO: Re-enable after core optimizations`). All execution flows through the interpreted path; the JIT module is wired but non-functional. This violates the project's own Tier-1 prohibition (`AGENTS.md` §1: "No shortcuts, stubs, or placeholders. No TODO/FIXME/HACK, no return 0; // stub, no simplified algorithms, no partial implementations"). It is also dead weight that confuses readers and hides whether a future "compiled query path" is on the roadmap or abandoned.

## What Changes

Choose one of two paths and commit:

**Option A — finish the JIT.** Complete Cranelift codegen for all physical operators (NodeByLabel / Filter / Expand / Project / OrderBy+Limit / Aggregate / SpatialSeek), wire planner integration, prove ≥ 1.5× hot-path speedup vs interpreter on the 74-test workload, and add proptest-style parity tests against the interpreted path.

**Option B — delete the module.** Remove `crates/nexus-core/src/execution/jit/`, all references, the matching `Cargo.toml` features, and any docs/specs that promise JIT. Capture a `learn_capture` entry explaining why.

Either way the disabled-with-TODOs state must end. Effort: Option A ~3 weeks, Option B ~1 day. Recommendation in `docs/analysis/nexus/10_improvement_roadmap.md` is Option B unless there is a concrete near-term consumer for Option A.

## Impact

- Affected specs: `docs/specs/cypher-subset.md` (if it mentions JIT), any roadmap entries referencing the JIT.
- Affected code: `crates/nexus-core/src/execution/jit/` (entire module), any feature-gated callers.
- Breaking change: NO (the module is non-functional).
- User benefit: code base honors its own Tier-1 rule; either ships a real JIT or drops the dead weight.
