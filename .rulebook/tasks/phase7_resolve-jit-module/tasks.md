## 1. Decision
- [ ] 1.1 Audit current state of `crates/nexus-core/src/execution/jit/` (count TODOs, list disabled ops, list callers)
- [ ] 1.2 Pick Option A (finish) or Option B (delete) — capture decision via `rulebook_decision_create`

## 2. Option A path (if chosen)
- [ ] 2.1 Implement Cranelift codegen for NodeByLabel
- [ ] 2.2 Implement Cranelift codegen for Filter
- [ ] 2.3 Implement Cranelift codegen for Expand
- [ ] 2.4 Implement Cranelift codegen for Project
- [ ] 2.5 Implement Cranelift codegen for OrderBy + Limit
- [ ] 2.6 Implement Cranelift codegen for Aggregate
- [ ] 2.7 Implement Cranelift codegen for SpatialSeek
- [ ] 2.8 Wire planner threshold to opt-in / opt-out of JIT per query
- [ ] 2.9 Add proptest parity tests (interpreted vs JIT — same input → same output for every op)
- [ ] 2.10 Bench ≥ 1.5× speedup vs interpreter on 74-test workload

## 3. Option B path (if chosen)
- [ ] 3.1 Remove `crates/nexus-core/src/execution/jit/` directory
- [ ] 3.2 Remove all references to the module from `crates/nexus-core/src/lib.rs` and callers
- [ ] 3.3 Remove `cranelift` deps from `Cargo.toml` if unused elsewhere
- [ ] 3.4 Remove any roadmap / spec references promising a JIT
- [ ] 3.5 Capture learning via `rulebook_learn_capture` explaining the deletion rationale

## 4. Tail (mandatory — enforced by rulebook v5.3.0)
- [ ] 4.1 Update or create documentation covering the implementation
- [ ] 4.2 Write tests covering the new behavior
- [ ] 4.3 Run tests and confirm they pass
