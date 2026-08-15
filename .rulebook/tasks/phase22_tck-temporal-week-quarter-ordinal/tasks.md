## 1. Implementation
- [x] 1.1 ISO week date: `week` + `dayOfWeek` resolved against the **week-based**
      year, not the calendar year — `eval/projection/calendar_fields.rs` via
      `NaiveDate::from_isoywd_opt`, pinned by
      `{year: 2019, week: 1, dayOfWeek: 1}` → 2018-12-31.
- [x] 1.2 `quarter` + `dayOfQuarter` — with an overshoot check, so a
      `dayOfQuarter` past the quarter's length is no date rather than a silent
      spill into the next quarter.
- [x] 1.3 `ordinalDay`
- [ ] 1.4 Reject mixing the families (and mixing with month/day) with the spec's
      error kind — currently the most specific family present wins silently.
- [x] 1.5 Cover `date`, `localdatetime`, and `datetime`, including the 53-week
      years in the corpus — one resolver shared by all constructors.

## 2. Tail (docs + tests — check or waive with tailWaiver)
- [ ] 2.1 Update or create documentation covering the implementation
- [ ] 2.2 Write tests covering the new behavior
- [ ] 2.3 Run tests and confirm they pass

## 3. Gates (every item, no exceptions)
- [ ] 3.1 `cargo +nightly fmt --all` and `cargo clippy --workspace --all-targets --all-features -- -D warnings` clean
- [ ] 3.2 `cargo +nightly test --workspace --no-fail-fast` green (a plain `--workspace` run aborts at the first failing target)
- [ ] 3.3 Neo4j differential suite still 300/300 (`scripts/compatibility/test-neo4j-nexus-compatibility-200.ps1`)
- [ ] 3.4 TCK re-run: this task's categories improved, no category regressed against an identical re-run
- [ ] 3.5 Regenerate `docs/compatibility/OPENCYPHER_TCK_REPORT.md`
