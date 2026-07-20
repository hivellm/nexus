# Proposal: phase0_fix-cypher-unbounded-alloc-oom

**Priority: CRITICAL — five distinct call sites let a single Cypher query allocate gigabytes
to petabytes of memory (or loop forever) from ordinary user-controlled numeric arguments,
aborting the process.** Found during a Cypher executor robustness audit; not previously
reported.

## Why

None of the five sites bound the allocation to the user-supplied size before allocating, and
the release profile has `overflow-checks` off (workspace `Cargo.toml` `[profile.release]` sets
none → Cargo default `false`), so i64 arithmetic that would panic in debug instead wraps
silently in production — in one case turning a would-be panic into an infinite loop.

- `range()` (`crates/nexus-core/src/executor/eval/projection/fn_list.rs:131-145`): builds the
  result via unchecked loops with no size guard:
  ```rust
  let mut result = Vec::new();
  if step > 0 {
      let mut i = start;
      while i <= end {
          result.push(Value::Number(i.into()));
          i += step;
      }
  } ...
  ```
  `i += step` at :136/:142 is unchecked; once `i` passes `i64::MAX` it wraps negative,
  `while i <= end` stays true, and the loop pushes forever. `RETURN range(0, 5000000000)` OOMs;
  `RETURN range(0, 9223372036854775807, 3)` loops forever.

- `lpad`/`rpad` (`crates/nexus-core/src/executor/eval/projection/fn_string.rs:563-568`):
  `target_len` (computed at :546-551 via `.unwrap_or(0).max(0) as usize`) is never capped
  before:
  ```rust
  let need = target_len - chars.len();
  let mut padding = String::new();
  while padding.chars().count() < need {
      padding.push_str(&pad);
  }
  ```
  `RETURN lpad('a', 9000000000, 'x')` allocates a multi-gigabyte string (also O(n²):
  `chars().count()` re-scans the growing buffer every iteration).

- Var-length path depth cap (`crates/nexus-core/src/executor/operators/path.rs:868-869`):
  `max_length` is set to `usize::MAX` for `ZeroOrMore`/`OneOrMore` quantifiers. The BFS
  (:992-1103) is bounded (cycle guard :1080, frame dedup by `(node,length)` :1092) so it
  terminates, but each frontier frame clones its path (`path_rels.clone()`/`path_nodes.clone()`,
  :1085/:1087), giving O(N³) memory on a dense graph. `push_with_row_cap` (:1056) caps only
  output rows, not queue/visited growth. The sibling `quantified_expand.rs` already clamps
  unbounded quantifiers to `MAX_QPP_DEPTH = 64`; this operator does not.
  `MATCH (a)-[*]->(b) RETURN count(b)` on a near-complete graph of a few thousand nodes
  exhausts memory.

- `materialize_rows_from_variables` (`crates/nexus-core/src/executor/eval/helpers.rs:261-306`):
  the combination odometer loop (:277-303) pushes into `rows` unconditionally; `total_combinations`
  is computed at :270 but never used to guard the loop. This is distinct from
  `apply_cartesian_product`, which already has an explicit byte-budget check (:114-147). Reached
  from the var-length path source-row provider (`path.rs:859`) whenever
  `context.result_set.rows` is empty and multiple same-length multi-element arrays are being
  combined.

- BYTES base64 decode (`crates/nexus-core/src/executor/eval/bytes.rs:93-99`): `B64.decode(s)`
  allocates the full decoded `Vec<u8>` before `bytes_from_vec` enforces
  `MAX_BYTES_PER_PROPERTY` (:72) — a smaller (~0.75x) but real pre-cap amplification.

## What Changes

- `range()`: compute the element count as `(end - start) / step + 1` using checked arithmetic,
  reject or cap the query above a fixed size threshold with a Cypher error (never a panic or
  silent wrap), and use `checked_add`/`checked_sub` on the step increment so a would-be overflow
  is rejected instead of wrapping into an infinite loop.
- `lpad`/`rpad`: cap `target_len` to a fixed maximum and return a Cypher error above it; compute
  the padding by tracking a running char count instead of re-scanning the buffer each iteration
  (also fixes the O(n²)).
- Var-length path: clamp `max_length` to a bounded constant (mirroring `quantified_expand.rs`'s
  `MAX_QPP_DEPTH`) immediately after it is set at :868-869.
- `materialize_rows_from_variables`: add the same checked-multiplication combination-count +
  byte-budget precheck that `apply_cartesian_product` already uses, returning a Cypher error
  instead of allocating when the budget is exceeded.
- BYTES decode: reject the input based on its pre-decode length
  (`s.len() > MAX_BYTES_PER_PROPERTY * 4 / 3 + 4`) before calling `B64.decode`.
- All five checks report a Cypher error to the caller; none of them may panic, wrap, or allocate
  past the bound first.

## Impact

- Affected specs: `docs/specs/cypher-subset.md` (range/lpad/rpad/BYTES function contracts,
  var-length path bounds)
- Affected code: `crates/nexus-core/src/executor/eval/projection/fn_list.rs` (range),
  `fn_string.rs` (lpad/rpad), `crates/nexus-core/src/executor/operators/path.rs` (var-length
  depth cap), `crates/nexus-core/src/executor/eval/helpers.rs`
  (materialize_rows_from_variables), `crates/nexus-core/src/executor/eval/bytes.rs` (base64
  decode)
- Breaking change: NO for well-formed queries within reasonable bounds; queries that previously
  would have OOM'd or hung now return a bounded Cypher error
- User benefit: no single query can abort the server process or exhaust host memory via
  `range()`, `lpad`/`rpad`, unbounded variable-length paths, cartesian row materialization, or
  oversized BYTES literals
- Related: `phase0_fix-cypher-eval-panics` (the sibling eval-robustness audit finding),
  `phase0_fix-cypher-oom-process-abort` (prior OOM-guard work in this area)
