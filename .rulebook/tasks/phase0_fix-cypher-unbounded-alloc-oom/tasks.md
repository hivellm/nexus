# Tasks: phase0_fix-cypher-unbounded-alloc-oom

Five Cypher eval/executor sites allocate (or loop) proportional to a user-controlled size before
validating it, so an ordinary query can exhaust process memory or hang forever. Trigger examples:
`RETURN range(0, 5000000000)` (OOM), `RETURN range(0, 9223372036854775807, 3)` (unchecked
`i += step` wraps negative in release → infinite loop), `RETURN lpad('a', 9000000000, 'x')`
(multi-GB string), `MATCH (a)-[*]->(b) RETURN count(b)` on a dense graph of a few thousand nodes
(O(N³) frontier memory in the var-length BFS), and a var-length path source-row cartesian combine
through `materialize_rows_from_variables` with no combination-count guard.

Order matters: reproduce every defect with a failing test before touching any site, so each fix
is checked against a concrete regression. Fix `range()` and `lpad`/`rpad` first — they are
reachable from a single bare `RETURN`, no schema or graph state required, so they are the
highest-reachability crash vectors. The operator-level caps (var-length depth, cartesian
materialize, BYTES pre-cap) follow because they need more query surface (a pattern, graph data,
or a BYTES literal) to trigger.

## 1. Reproduce each defect with a failing test
- [ ] 1.1 `RETURN range(0, 5000000000)` — assert it returns a Cypher error (not an OOM); today it
  allocates ~5B elements (`fn_list.rs:131-145`)
- [ ] 1.2 `RETURN range(0, 9223372036854775807, 3)` — assert it terminates with a Cypher error;
  today `i += step` (`fn_list.rs:136`) wraps negative once `i` exceeds `i64::MAX` in release,
  making `while i <= end` loop forever
- [ ] 1.3 `RETURN lpad('a', 9000000000, 'x')` — assert a Cypher error, not a multi-gigabyte
  allocation (`fn_string.rs:563-568`)
- [ ] 1.4 A var-length query over a dense synthetic graph of a few thousand nodes,
  `MATCH (a)-[*]->(b) RETURN count(b)` — assert bounded memory (`path.rs:868-869,992-1103`)
- [ ] 1.5 A source-row combination through `materialize_rows_from_variables`
  (`helpers.rs:261-306`) sized past a reasonable byte budget — assert a Cypher error instead of
  full materialization
- [ ] 1.6 A BYTES literal whose base64 form decodes past `MAX_BYTES_PER_PROPERTY` — assert
  rejection before the full `Vec<u8>` is allocated (`bytes.rs:93-99`)

## 2. Fix range() (highest reachability)
- [ ] 2.1 In `crates/nexus-core/src/executor/eval/projection/fn_list.rs:131-145`, compute the
  element count as `(end - start) / step + 1` with checked arithmetic before allocating the
  result `Vec`
- [ ] 2.2 Reject (Cypher error) counts above a fixed size threshold instead of allocating
- [ ] 2.3 Replace the unchecked `i += step` (:136) and the negative-step decrement (:142) with
  `checked_add`/`checked_sub`; treat overflow as loop termination with a Cypher error, never a
  silent wrap
- [ ] 2.4 Confirm the §1.1 and §1.2 tests pass

## 3. Fix lpad/rpad (highest reachability)
- [ ] 3.1 In `crates/nexus-core/src/executor/eval/projection/fn_string.rs:546-568`, cap
  `target_len` to a fixed maximum and return a Cypher error above it, before the padding loop
  runs
- [ ] 3.2 Replace the O(n²) `while padding.chars().count() < need` re-scan (:566) with a running
  char-count accumulator so the fix does not leave the quadratic cost behind
- [ ] 3.3 Confirm the §1.3 test passes

## 4. Fix var-length path depth cap
- [ ] 4.1 In `crates/nexus-core/src/executor/operators/path.rs:868-869`, clamp `max_length` for
  `ZeroOrMore`/`OneOrMore` quantifiers to a bounded constant instead of `usize::MAX`, mirroring
  `quantified_expand.rs`'s `MAX_QPP_DEPTH = 64`
- [ ] 4.2 Confirm the §1.4 test passes and that a legitimate bounded-depth query (`[*1..5]`) is
  unaffected

## 5. Fix materialize_rows_from_variables cartesian guard
- [ ] 5.1 In `crates/nexus-core/src/executor/eval/helpers.rs:261-306`, use the already-computed
  `total_combinations` (:270) to guard the odometer loop (:277-303) with the same
  checked-multiplication count + byte-budget precheck that `apply_cartesian_product` (:114-147)
  already applies, returning a Cypher error when the budget is exceeded
- [ ] 5.2 Confirm the §1.5 test passes and that `apply_cartesian_product`'s existing guard is
  untouched (no regression to its budget or error message)

## 6. Fix BYTES base64 pre-cap
- [ ] 6.1 In `crates/nexus-core/src/executor/eval/bytes.rs:93-99`, reject the input when
  `s.len() > MAX_BYTES_PER_PROPERTY * 4 / 3 + 4` before calling `B64.decode`, so the oversized
  buffer is never allocated
- [ ] 6.2 Confirm the §1.6 test passes and that a BYTES literal at or under the cap still decodes
  correctly

## 7. Tail (docs + tests — check or waive with tailWaiver)
- [ ] 7.1 Update `docs/specs/cypher-subset.md` with the bounded-size contracts for `range()`,
  `lpad`/`rpad`, variable-length path depth, and BYTES literals; add a CHANGELOG entry
- [ ] 7.2 Tests: all six §1 regression tests pass; add a boundary test per site (largest accepted
  size succeeds, one past it errors)
- [ ] 7.3 Run `cargo +nightly fmt --all`,
  `cargo clippy --workspace --all-targets --all-features -- -D warnings`,
  `cargo +nightly test --workspace` — all green

## Related
- `phase0_fix-cypher-eval-panics` — sibling eval-robustness defects found in the same audit
- `phase0_fix-cypher-oom-process-abort` — prior OOM-guard work in this area
