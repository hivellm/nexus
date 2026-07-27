# Oversized Rust file split: directory module + facade mod.rs with minimal visibility widening

**Category**: architecture
**Tags**: refactor, rust, modules, visibility, phase5_split-oversized-files

## Description

Recipe used to split 17 files >1500 lines with zero behavior change: (1) convert foo.rs into foo/ + facade mod.rs that keeps struct defs/module decls and re-exports (pub use) every previously-reachable path so NO caller is edited; (2) move impl blocks byte-identical — never transcribe/retype (transcription introduced match-ergonomics bugs once; restoring byte-identical from a .bak fixed it); (3) when code moves one module level deeper, pub(super) loses one level — translate to pub(in super::super) or pub(in crate::<module>) to preserve the ORIGINAL effective boundary, never wider; (4) verify with cargo +nightly check -p <crate> --tests (plain check skips cfg(test) callers — parser/planner tests caught E0624s that check alone missed); (5) count #[test] attributes before/after — must match exactly; (6) format only the touched files with rustfmt --edition 2024, never cargo fmt --all while parallel agents share the tree.

## When to Use

Any file >1500 lines being decomposed; multi-agent parallel refactors on disjoint modules of the same crate.

## When NOT to Use

Files that are dead code (verify the target is actually compiled by some Cargo target first — tests/integration_test.rs was unwired and splitting it would have been wasted work); files carrying someone else's uncommitted WIP unless the move provably preserves it byte-for-byte and the result stays uncommitted.
