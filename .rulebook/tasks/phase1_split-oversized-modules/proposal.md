# Proposal: phase1_split-oversized-modules

## Why

22 source files in this repo exceed 1500 lines, with the worst offenders being
`nexus-core/src/executor/mod.rs` (15,260 LOC), `executor/parser.rs` (6,882 LOC),
`nexus-core/src/lib.rs` (5,531 LOC) and `graph/correlation/mod.rs` (4,638 LOC).
These oversized files hurt the project in four concrete ways:

1. **Merge-conflict magnet**: any parallel work in the executor or parser
   almost always collides, which has already blocked recent feature branches.
2. **Compile-time regression**: `executor/mod.rs` alone forces rustc to
   re-typecheck ~15k lines on every touch; incremental builds in that crate
   take noticeably longer than the rest of the workspace combined.
3. **AI-agent accuracy drops**: files beyond ~2k lines exceed what agents can
   reliably reason about in a single pass, which directly contradicts the
   project rule `sequential-editing.md` (1–2 files per sub-task).
4. **Reviewability**: reviewers cannot meaningfully audit changes scoped to
   a 15k-line file; unrelated logic gets pulled into every PR diff.

The split is a pure refactor — no behavior changes, no public API changes.
Coverage must stay at ≥95% throughout.

## What Changes

Decompose the 22 oversized files into cohesive submodules. Each split follows
the same pattern: the original file becomes a thin `mod.rs` façade that
re-exports the public API, and the implementation moves into topically
grouped submodules. This is executed in four priority tiers (see `tasks.md`)
so that Tier 1 (the four critical blockers totaling ~32k LOC) lands before
any further feature work on the executor / parser / correlation engine.

Proposed decomposition targets (full details per file in `tasks.md`):

- `executor/mod.rs` → `executor/operators/{match,expand,filter,project,aggregate}.rs`,
  `executor/functions/{string,math,aggregate,list,temporal}.rs`,
  `executor/{eval,context,result}.rs`.
- `executor/parser.rs` → `parser/{tokens,literals,expressions}.rs` +
  `parser/clauses/{match,where,return,with,merge,create,set,delete,unwind,call}.rs`.
- `lib.rs` → extract `types.rs`, `error.rs`, `config.rs`; keep only the crate
  root and its re-exports in `lib.rs`.
- `graph/correlation/mod.rs` → `correlation/{correlator,scoring,analyzer,reporter}.rs`.
- Tier 2 files (planner, data_flow, cypher API, algorithms) follow the same
  pattern — details in `tasks.md`.

## Impact

- Affected specs: none (pure refactor, no spec changes)
- Affected code:
  - `nexus-core/src/executor/` (Tier 1 + Tier 2)
  - `nexus-core/src/lib.rs` (Tier 1)
  - `nexus-core/src/graph/` (correlation, algorithms, clustering, procedures, core)
  - `nexus-core/src/storage/` (mod.rs, adjacency_list.rs)
  - `nexus-core/src/catalog/mod.rs`
  - `nexus-core/src/index/mod.rs`
  - `nexus-server/src/api/{cypher,streaming}.rs`
  - Test files: `tests/integration_test.rs`, `nexus-core/tests/{regression_extended,neo4j_compatibility_test,neo4j_result_comparison_test}.rs`
- Breaking change: **NO** — all public APIs preserved via `pub use` re-exports
- User benefit: faster incremental builds, parallelizable PRs, reviewable
  diffs, AI-agent-friendly file sizes (all files target <1500 LOC post-split)
