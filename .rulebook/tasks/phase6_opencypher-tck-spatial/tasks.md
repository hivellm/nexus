# Implementation Tasks — openCypher TCK Spatial Import

## 1. Vendor `spatial.feature` corpus

- [ ] 1.1 Fetch the openCypher distribution at a pinned commit (`github.com/opencypher/openCypher`) and extract every spatial `*.feature` file. Confirm the upstream path against the pinned commit; the canonical location at the time of carve-out is under `tck/features/`.
- [ ] 1.2 Land the files under `crates/nexus-core/tests/tck/spatial/` with a `VENDOR.md` recording the upstream commit hash, the vendor date, and the SHA-256 of every file so future bumps stay reproducible.
- [ ] 1.3 Update `LICENSE-NOTICE.md` (or create it) with the openCypher Apache 2.0 attribution and a pointer at the vendored corpus.

## 2. Cucumber harness

- [ ] 2.1 Add `cucumber = "0.21"` to the workspace `[dev-dependencies]` block. Confirm the default features cover the parser surface the runner needs (`gherkin`, `tracing`).
- [ ] 2.2 Implement `crates/nexus-core/tests/tck_runner.rs` mapping the standard openCypher steps (`Given a graph "<name>"`, `When executing query: ...`, `Then the result should be ...`) onto `Engine::execute_cypher`. Discover `.feature` files under `tck/spatial/`.
- [ ] 2.3 Wire the runner into `cargo test -p nexus-core --test tck_runner`. Add a CI lane in `.github/workflows/` if the project uses GitHub Actions; otherwise document the local invocation in `docs/compatibility/`.

## 3. Fix every failing scenario

- [ ] 3.1 Run the harness and capture the failing-scenario list.
- [ ] 3.2 For each failure, triage as Cypher coverage bug (file an unblocking task) OR implementation bug (fix on the spot). Target 0 failing scenarios before archiving.
- [ ] 3.3 Update `docs/compatibility/NEO4J_COMPATIBILITY_REPORT.md` with the TCK pass count and the upstream commit hash.

## 4. Tail (mandatory — enforced by rulebook v5.3.0)

- [ ] 4.1 Update or create documentation covering the implementation — `crates/nexus-core/tests/tck/spatial/VENDOR.md`, `LICENSE-NOTICE.md` Apache attribution, `docs/compatibility/NEO4J_COMPATIBILITY_REPORT.md` TCK section, CHANGELOG entry.
- [ ] 4.2 Write tests covering the new behavior — the vendored `.feature` files plus the `tck_runner.rs` harness ARE the tests; no separate unit-test layer required.
- [ ] 4.3 Run tests and confirm they pass — `cargo +nightly test -p nexus-core --test tck_runner --all-features` reports 0 failures.
