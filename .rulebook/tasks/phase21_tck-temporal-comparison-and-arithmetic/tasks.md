## 1. Implementation
- [x] 1.1 duration scalar mul/div — already implemented by the temporal store
      round-trip changeset (commit `ff7c18e2`): `eval/temporal.rs`'s
      `scale_duration` backs `duration * number` / `duration / number` with
      fractional carry (reuses `temporal_parse::AVG_SECONDS_PER_MONTH`/
      `SECONDS_PER_DAY`), truncating (not rounding) the sub-nanosecond
      remainder per `Temporal8.feature` scenario 7.
- [x] 1.2 fractional duration components — `duration({years: 12.5, ...})`
      now carries fractional `years`/`months`/`weeks`/`days` down through
      the next-smaller unit exactly like the ISO string parser. Extracted
      `temporal_parse::carry_duration_components` (shared by
      `parse_iso_duration` and the new fractional branch in
      `projection/fn_temporal.rs`'s `duration({...})` map constructor) so
      both paths use byte-for-byte the same carry math. Integer-typed
      years/months/weeks/days keep the pre-existing checked-exact
      `i64` path (overflow still errors); only a genuinely fractional JSON
      number triggers the carry path. Verified against
      `Temporal8.feature` Scenario [1] row 3 and Scenario [6] row 3.
- [x] 1.3 all-kind +/- duration — already implemented by the temporal store
      round-trip changeset (commit `ff7c18e2`): `eval/temporal.rs`'s
      `apply_duration_to_time_of_day` (LocalTime/Time, 24h wraparound) and
      `apply_duration_to_tagged_instant` (dispatches by tagged kind: Date,
      LocalDateTime, DateTime, LocalTime, Time) implement +/-duration for
      every temporal kind, not just Date/DateTime.
- [x] 1.4 within-kind comparison/ordering — `eval/predicate.rs`'s
      `compare_values_for_sort` now special-cases the `(String, String)`
      arm: when both operands re-derive as a tagged `duration` (via
      `temporal_retag::retag_duration` + `temporal_value::duration_components`,
      handling both an already-tagged value and a stored canonical string),
      it compares the normalized `(months, days, seconds, nanos)` tuple
      lexicographically instead of the rendered ISO string (which sorted
      `"PT10H"` before `"PT9H"`). This comparator backs both `ORDER BY` and
      the `<`/`<=`/`>`/`>=` operators (see `projection/core.rs`'s `BinaryOp`
      match) — no TCK scenario in this corpus pins a separate
      "durations aren't comparable via `<`/`>`" WHERE-clause behavior, so
      no new error/null path was invented; equality already worked via
      canonical-string comparison and needed no change.

## 2. Tail (docs + tests — check or waive with tailWaiver)
- [x] 2.1 Update or create documentation covering the implementation — doc
      comments on `carry_duration_components`, the map-constructor
      fractional branch, and `compare_values_for_sort` explain the new
      behavior in place; no user-facing docs describe duration ordering
      today, so none needed updating.
- [x] 2.2 Write tests covering the new behavior — 6 new tests in
      `tests/cypher/test_temporal_arithmetic.rs` (fractional map carry vs.
      ISO-string carry equivalence, TCK Scenario [1]/[6] row-3 exact
      renderings, integer-path construction-overflow regression,
      fractional-path overflow) and 3 new tests in
      `tests/cypher/temporal_store_roundtrip_test.rs` (ORDER BY on stored
      durations, ORDER BY on a mix of stored and freshly-constructed
      durations, `<`/`>` component-wise comparison).
- [x] 2.3 Run tests and confirm they pass — `cargo test --package
      nexus-core --test cypher` 566/566 (was 557 pre-change, +9 new),
      `--test compatibility` 245/245, `--test executor` 272/272,
      `--test regression` 227/227; `cargo clippy --workspace
      --all-targets --all-features -- -D warnings` clean.
