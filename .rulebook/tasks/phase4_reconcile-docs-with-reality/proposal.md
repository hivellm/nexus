# Proposal: phase4_reconcile-docs-with-reality

## Why

Several docs carry numbers that contradict each other or the current code:

- `CLAUDE.md` says "293/300 Neo4j Compatibility Tests Passing (97.99%)"
  in one line and "300/300 Neo4j compatibility tests passing" in
  another.
- `docs/ROADMAP.md` marks phases as "completed" with dates that do not
  line up with `CHANGELOG.md`.
- `.dockerignore` used to claim the build relied on a cargo-chef recipe
  stage that didn't exist (fixed in commit `5e6a12ec` — check the rest
  of the docs for similar fiction).
- `README.md` boasts "Production Ready, 2949+ tests passing" without a
  link to the proof.

Claims the docs make become sales copy once they stop matching reality.

## What Changes

- For each number (test count, compatibility percentage, roadmap date),
  identify the source of truth (test output, CHANGELOG entry) and
  update the claim to match, or remove it.
- Establish one canonical "status" document that the rest of the docs
  link to instead of duplicating.
- Flag remaining "Production Ready" style claims that the maintainers
  want to keep — with a short line explaining the criterion.

## Impact

- Affected specs: none
- Affected code: `CLAUDE.md`, `README.md`, `docs/ROADMAP.md`,
  `CHANGELOG.md`, `docs/NEO4J_COMPATIBILITY_REPORT.md`, any `//!` docs
  that echo the same statistics
- Breaking change: NO
- User benefit: new contributors and evaluators can trust the docs;
  existing contradictions stop confusing ongoing work
