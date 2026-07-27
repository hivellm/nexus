# Tasks: phase0_fix-cypher-unbounded-alloc-oom

Five Cypher eval/executor sites allocate (or loop) proportional to a user-controlled size before
validating it, so an ordinary query can exhaust process memory or hang forever. Fixed with a
bounded guard at each site that returns a Cypher error before allocating past the bound.

SAFETY NOTE: the pre-fix reproductions (range 5B, range i64::MAX span, lpad 9B, dense/cyclic
var-length) OOM or infinite-loop the machine, so the defects were confirmed by CODE INSPECTION
(the proposal's line-by-line trace), NOT by executing an unfixed repro. The regression tests
assert the FIXED bounded-error behavior and run against fixed code only (they error before
allocating, so they complete instantly).

## 1. Reproduce each defect with a failing test
- [x] 1.1 `RETURN range(0, 5000000000)` returns a Cypher error (not OOM) —
  `range_rejects_element_count_over_cap_instead_of_oom`
- [x] 1.2 `RETURN range(0, 9223372036854775807, 3)` terminates with a Cypher error, no hang —
  `range_with_huge_span_and_step_terminates_with_error_not_infinite_loop`
- [x] 1.3 `RETURN lpad('a', 9000000000, 'x')` returns a Cypher error —
  `lpad_rejects_target_length_over_cap_instead_of_oom` (+ rpad variant)
- [x] 1.4 Unbounded var-length path over a cycle terminates (bounded depth) —
  `unbounded_var_length_path_over_a_cycle_terminates` (a small cycle, safe post-fix; a dense
  few-thousand-node graph is the pre-fix OOM repro, NOT run per the safety note)
- [x] 1.5 `materialize_rows_from_variables` cartesian combine over the byte budget returns a
  Cypher error — unit test `materialize_rows_rejects_cartesian_product_over_budget`
- [x] 1.6 Oversized base64 BYTES payload rejected before the `Vec<u8>` is allocated — unit test
  `reject_oversize_base64_rejects_before_decode_and_accepts_valid`

## 2. Fix range() (highest reachability)
- [x] 2.1 `range_element_count` computes the count with fully checked arithmetic (handles
  `i64::MIN` via `unsigned_abs`, `checked_sub`/`checked_div`/`checked_add`, `usize::try_from`)
  before allocating (`fn_list.rs`)
- [x] 2.2 `build_range` rejects counts above `MAX_RANGE_ELEMENTS = 2_000_000` (~96 MB bound) with
  a Cypher error before `Vec::with_capacity`
- [x] 2.3 The generation loop uses `checked_add(step)` and breaks on overflow — never the silent
  wrap that made the release build loop forever
- [x] 2.4 §1.1 and §1.2 tests pass

## 3. Fix lpad/rpad (highest reachability)
- [x] 3.1 `target_len` capped to `MAX_PAD_LEN = 1_000_000` (~4 MB bound) with a Cypher error
  before the padding loop (`fn_string.rs`)
- [x] 3.2 The O(n²) `while padding.chars().count() < need` re-scan replaced with a running
  `padding_char_count` accumulator
- [x] 3.3 §1.3 test passes

## 4. Fix var-length path depth cap
- [x] 4.1 `max_length` clamped to `MAX_VAR_LENGTH_PATH_DEPTH = 64` (mirrors
  `quantified_expand.rs`'s `MAX_QPP_DEPTH`) — covers `ZeroOrMore`/`OneOrMore` (`usize::MAX`) AND
  large `{m,n}` bounds (`path.rs`)
- [x] 4.2 §1.4 passes; `bounded_var_length_path_is_unaffected_by_the_depth_cap` confirms `[*1..2]`
  is unaffected

## 5. Fix materialize_rows_from_variables cartesian guard
- [x] 5.1 The cartesian branch now computes `total_combinations` with `checked_mul` + a
  byte-budget precheck against `config.cartesian_product_max_bytes`, returning `Error::OutOfMemory`
  when exceeded (`helpers.rs`). The function now returns `Result`; all callers propagate it
- [x] 5.2 §1.5 passes; `apply_cartesian_product`'s own guard/message untouched (a parallel check,
  not a shared call — the two size their columns differently)

## 6. Fix BYTES base64 pre-cap
- [x] 6.1 `reject_oversize_base64` rejects when `s.len() > MAX_BYTES_PER_PROPERTY * 4 / 3 + 4`
  before `B64.decode`, at both `bytes_value_to_vec` and `coerce_param_to_bytes` (and the
  string-function base64 decode site)
- [x] 6.2 §1.6 passes; a valid in-bounds base64 payload still decodes

## 7. Tail (docs + tests — check or waive with tailWaiver)
- [x] 7.1 Update or create documentation covering the implementation — CHANGELOG entry + four
  `docs/specs/cypher-subset.md` sections (range, lpad/rpad, var-length depth, BYTES) with the
  specific caps
- [x] 7.2 Write tests covering the new behavior — 8 query-level tests
  (`tests/cypher/unbounded_alloc_guard_test.rs`) + 2 unit tests (materialize budget, bytes
  pre-cap); each oversized case errors, each in-bounds case succeeds
- [x] 7.3 Run tests and confirm they pass — `cargo +nightly fmt --all` + clippy
  (`--workspace --all-targets --all-features -- -D warnings`) clean; full-workspace
  `cargo +nightly test --workspace` green (CARGO_EXIT=0, 0 failed — no flake this run)

## Related
- `phase0_fix-cypher-eval-panics` — sibling eval-robustness defects found in the same audit
- `phase0_fix-cypher-oom-process-abort` — prior OOM-guard work in this area
