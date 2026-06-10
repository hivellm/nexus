## 1. Investigation
- [x] 1.1 Confirm the warn at mod.rs:3343 fires only after the while-loop completes; decide the in-loop threshold + fallback-entry log points — confirmed and fixed in 412f1acf (in-loop warn at hops == 1000); fallback-entry debug log added here (now `engine/write_exec.rs` post-split)

## 2. Implementation
- [x] 2.1 Log at chain-walk fallback entry (fast-path miss) and fire the hop-threshold warning during the loop, not after; keep it O(1) logging — in-loop warn shipped in 412f1acf; `tracing::debug!` at fallback entry added (debug level — entry is common on small graphs); both O(1). BONUS FIX surfaced by the test: the chain walk itself had an off-by-one (chain pointers are stored `rel_id + 1`, the walk read `read_rel(rel_ptr)` undecoded), so the authoritative fallback silently returned None / wrong ids whenever the exact-edge index missed — fixed (decode `rel_ptr - 1`, return the true rel id, ignore deleted records, matching the canonical decode in executor/operators/path.rs).

## 3. Tail (mandatory — enforced by rulebook v5.3.0)
- [x] 3.1 Update or create documentation (CHANGELOG / GH #20) — CHANGELOG [Unreleased] Fixed entry (telemetry + the off-by-one fix)
- [x] 3.2 Write tests: a high-degree fallback emits the warning at the threshold (capture via a tracing test subscriber or a counter), not only at completion — `chain_walk_warns_at_threshold_even_when_edge_is_found`: counting tracing layer, 1100-edge hub, both a shallow and a ≥1000-hop FOUND edge; asserts the warn fired (the old post-loop warn never fired for found edges)
- [x] 3.3 Run tests and confirm they pass — new test green; full `cargo test -p nexus-core --lib` 2382/2382 (two consecutive green runs); clippy 0 warnings; fmt applied
