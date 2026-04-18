# Proposal: phase1_remove-blanket-lint-suppressions

## Why

`nexus-server/Cargo.toml` and `nexus-core/Cargo.toml` both ship with
`[lints.rust] unused = "allow"`, `dead_code = "allow"` and
`[lints.clippy] all = "allow"`. That means *every clippy warning in the
workspace is suppressed* — including the ones `.claude/rules/rust.md` calls
"non-negotiable" (e.g. `await_holding_lock`, `missing_safety_doc`,
`unwrap_used`). CI green does not mean the code is clean; it means the
signal is turned off. Every other audit finding became harder to spot
because of this single block.

## What Changes

- Remove `all = "allow"` from the `[lints.clippy]` tables across the
  workspace.
- Remove `unused = "allow"` / `dead_code = "allow"` from `[lints.rust]`.
- Re-add surgical `#[allow(dead_code)]` or `#[allow(unused_imports)]` on
  the *specific* items that genuinely need it, with a 1-line comment
  explaining why.
- Land the fallout in the same PR chain so `cargo clippy --workspace --
  -D warnings` stays green.

## Impact

- Affected specs: none
- Affected code: `nexus-server/Cargo.toml:11-27`, `nexus-core/Cargo.toml`
  (same block), plus whatever code has to be cleaned up as new warnings
  surface
- Breaking change: NO (only tightens the lint gate)
- User benefit: every other quality PR gets real feedback from clippy
  again; regressions can't hide behind the suppression block
