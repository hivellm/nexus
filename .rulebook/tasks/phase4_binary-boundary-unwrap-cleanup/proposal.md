# Proposal: phase4_binary-boundary-unwrap-cleanup

## Why

`.claude/rules/rust.md` bans `.unwrap()` / `.expect()` in non-test code
unless the invariant is obvious from the surrounding 5 lines. Today the
binary crates still ship `.unwrap()` at real user-input boundaries:

- `nexus-cli/src/commands/mod.rs:31` — six `.unwrap()` on
  `serde_json::to_string_pretty`, any non-UTF-8 or non-serialisable JSON
  panics the CLI.
- `nexus-cli/src/commands/query.rs:33` — `let _ = std::fs::create_dir_all(
  parent)`, silent.
- `nexus-cli/src/commands/query.rs:307` — `let _ = rl.load_history(...)`.
- Scattered `.unwrap()` in `nexus-server/src/api/` on
  `SystemTime::now().duration_since(UNIX_EPOCH).unwrap()` and similar.

None of these are catastrophic on paper but they all violate the rule
that the CLI / HTTP edge should never panic, and they all can be replaced
with `?` + `anyhow::Context` without subtle semantics changes.

## What Changes

- Sweep `nexus-cli/src/` and `nexus-server/src/` for `.unwrap()` /
  `.expect()` / `let _ = ` patterns whose error carries real information.
- Replace each with `?` + `.with_context(|| "...")` (CLI) or a typed
  error + `Result<_, Error>` return (server).
- Where `let _ = ` is genuinely correct (fire-and-forget), add a
  `// discarded: reason` comment so future readers stop treating it as
  a smell.

## Impact

- Affected specs: none
- Affected code: `nexus-cli/src/commands/`, `nexus-server/src/api/`
- Breaking change: NO; panics replaced by errors
- User benefit: CLI returns actionable error messages instead of
  backtraces; the server surfaces I/O failures instead of the classic
  "it just returned 200 but nothing happened" pattern
