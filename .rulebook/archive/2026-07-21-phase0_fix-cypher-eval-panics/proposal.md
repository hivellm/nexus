# Proposal: phase0_fix-cypher-eval-panics

**Priority: CRITICAL — three eval-layer defects let ordinary user-controlled Cypher arguments
crash query execution: chrono panics on datetime ± duration overflow, silent wraparound on
duration ± duration overflow, and an out-of-bounds array index in percentile_cont.** Found
during a Cypher executor robustness audit; not previously reported.

## Why

The release profile has `overflow-checks` off (workspace `Cargo.toml` `[profile.release]` sets
none → Cargo default `false`), so i64 arithmetic wraps silently in production rather than
panicking — except where the panic comes from an explicit library check (chrono), which panics
unconditionally regardless of that setting.

- **Datetime/date ± duration chrono overflow**
  (`crates/nexus-core/src/executor/eval/temporal.rs`): the add path multiplies user-controlled
  duration components at :194,209,221,236,250 (e.g.
  `let duration_secs = days*86400 + hours*3600 + minutes*60 + seconds;`) then feeds the
  unchecked i64 result into `chrono::Duration::seconds`/`::days` and a `DateTime`/`NaiveDate`/
  `NaiveDateTime` `+`/`-` at :210, :237, :265 (add) and :306, :333, :361 (subtract), dispatched
  via `try_datetime_add` (temporal.rs:54, called from arithmetic.rs:54). chrono 0.4
  (`Cargo.toml:83`) panics when `Duration::seconds`/`::days` exceeds their millisecond range, and
  panics again on `DateTime`/`NaiveDate`/`NaiveDateTime` add/sub when the result leaves chrono's
  representable year range (~±262143) — an explicit `panic!("... overflowed")`, independent of
  the overflow-checks flag. `duration({..})` components are fully user-controlled.
  `RETURN date('2020-01-01') + duration({days: 999999999})` and
  `RETURN datetime('2020-01-01T00:00:00Z') + duration({days: 100000000})` both panic.

- **duration ± duration overflow** (`temporal.rs:81-86` add, `:147-152` subtract): all six
  components (`years`, `months`, ...) are combined with plain `+`/`-`, e.g.
  `let years = y1 + y2;`, with no checked arithmetic. In release (overflow-checks off) this wraps
  silently to a wrong duration; in debug it panics.
  `RETURN duration({years: 9223372036854775807}) + duration({years: 1})` demonstrates both
  failure modes depending on build profile.

- **percentile_cont index panic**
  (`crates/nexus-core/src/executor/operators/aggregate/core.rs:866-877`):
  ```rust
  let position = *percentile * (values.len() - 1) as f64;
  let lower_idx = position.floor() as usize;
  let upper_idx = position.ceil() as usize;
  ...
  values[lower_idx] ...
  ```
  the percentile argument is never validated to `[0,1]`; for `percentile > 1.0`, `position`
  exceeds `len - 1`, so `upper_idx` (and potentially `lower_idx`) reaches or exceeds
  `values.len()`, and `values[lower_idx]` panics with an out-of-bounds index. The sibling
  `PercentileDisc` (:837-839) is already safe (`.min(values.len()-1)`).
  `WITH [1.0,2.0,3.0] AS v UNWIND v AS x RETURN percentile_cont(x, 1.5)` panics (`position` 3.0 →
  `values[3]` on a length-3 slice). `percentile < 0` does not panic today because Rust's
  float→int `as` cast saturates to 0, but is still semantically invalid input that should be
  rejected.

## What Changes

- Temporal add/sub: replace the unchecked component multiplies/sums with
  `checked_mul`/`checked_add`, and replace the chrono `+`/`-` calls with their checked
  equivalents (`checked_add_signed`/`checked_sub_signed`, or `Duration::try_seconds`/
  `try_days`); on overflow at either stage, return a Cypher error instead of panicking.
- duration ± duration: replace each of the six unchecked component combinations with
  `checked_add`/`checked_sub`, returning a Cypher error on overflow instead of wrapping silently
  or panicking.
- percentile_cont: validate `percentile` is within `[0,1]` up front and return a Cypher error
  otherwise (matching Neo4j's own error behavior), and additionally clamp both
  `lower_idx`/`upper_idx` with `.min(values.len()-1)` as defense in depth, mirroring
  `PercentileDisc`.
- Secondary: the parallel AVG merge in `executor/operators/aggregate/parallel.rs:286-309`
  averages per-chunk means unweighted by chunk size, which is wrong for unequal chunks — but
  `execute_parallel_aggregation`/`is_parallelizable_aggregation`/`execute_sequential_aggregation`
  have no call sites (dead code). Fix by computing the weighted mean (Σsum/Σcount from each
  chunk's (sum,count)) so the module is correct if/when it is wired in, or delete the unused
  module — do not leave a known-wrong dead function in the tree.
- None of these fixes may panic, wrap, or return a silently wrong value on invalid/overflowing
  input — all must surface a Cypher error.

## Impact

- Affected specs: `docs/specs/cypher-subset.md` (temporal arithmetic, duration arithmetic,
  percentile_cont argument contract)
- Affected code: `crates/nexus-core/src/executor/eval/temporal.rs` (datetime ± duration,
  duration ± duration), `crates/nexus-core/src/executor/operators/aggregate/core.rs`
  (percentile_cont), `crates/nexus-core/src/executor/operators/aggregate/parallel.rs` (dead-code
  AVG merge)
- Breaking change: NO for well-formed queries; queries that previously panicked or silently
  wrapped now return a bounded Cypher error
- User benefit: temporal arithmetic, duration arithmetic, and percentile_cont can no longer
  crash a query or silently return a wrong value from user-controlled numeric input
- Related: `phase0_fix-cypher-unbounded-alloc-oom` (the sibling eval-robustness audit finding),
  `phase0_fix-cypher-oom-process-abort` (prior crash-hardening work in this area)
