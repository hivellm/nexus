# Tasks: phase0_fix-storage-oob-panics

Three independent bounds defects in the record/property stores let a
corrupt-or-crafted on-disk value slip past its length guard and panic on an
out-of-bounds slice instead of returning a storage error: (#2) record-store
offset arithmetic overflows/wraps in release (`record_store_ops.rs` read/write
node/rel), (#3) property-store header reads over-read the last ~12 bytes
before EOF (`property_store.rs` `get_entity_info_at_offset` /
`load_properties_at_offset`), (#4) file grow is not sized to the target write
offset (`record_store.rs` `grow_nodes_file`/`grow_rels_file`). All three are
reachable from ordinary graph reads over data that is merely corrupt, not
from API misuse.

Order matters: reproduce all three with failing tests first (§1) so each fix
has an objective before/after signal against the exact wrap/over-read/
under-grow condition described in the proposal — these are arithmetic edge
cases that are easy to "fix" without actually closing the gap. Fix #2 before
#3: `read_node`'s corrupt-`prop_ptr` trigger for #3 is reached through
`read_node`, so hardening #2's own offset arithmetic first means the #3 test
in §3 is exercising only the property-store defect, not an incidental #2
wraparound on the same call path. #4 is independent of #2/#3 (write path,
not read path) and can follow in any order relative to them, but is placed
last because it is the lowest-severity of the three (requires a large id
jump, not a single crafted small value).

## 1. Reproduce all three defects first
Done: tests/storage/record_store_bounds_test.rs (#2 read/write node+rel, #4
sparse grow) + in-module property_store test (#3). Confirmed RED for the right
reason: #2 `attempt to multiply with overflow` at the exact offset lines; #4
`range start index 10400000 out of range for slice of length 3145728`; #3
returned garbage `Some((0,Node))` at len-12 (and would panic at >= len-8).
NOTE on test profile: debug/test builds have overflow-checks ON, so pre-fix #2
PANICS on the multiply (rather than wrapping as in release) — the checked
arithmetic fix closes BOTH profiles.
- [x] 1.1 Write a failing test for #2: pick a `node_id` such that
  `node_id as usize * NODE_RECORD_SIZE` wraps to land `offset+32` at or near
  0 (per the proposal's derivation, the multiple of 32 nearest `2^64` is
  `2^64-32`), call `RecordStore::read_node`, and confirm it panics today
  instead of returning `Err`. Repeat for `read_rel` (`* REL_RECORD_SIZE`,
  i.e. `* 52`) and, if feasible without corrupting the test fixture's real
  data, for `write_node`/`write_rel`
- [x] 1.2 Write a failing test for #3: construct a `PropertyStore` whose
  mmap length is `L`, call `get_entity_info_at_offset(offset)` and
  `load_properties_at_offset(offset)` with `offset` in `[L-12, L-1]`
  (inside the current single-byte guard's blind spot), and confirm both
  panic today instead of returning `None`
- [x] 1.3 Write a failing test for #4: create a `RecordStore`, then call
  `write_node`/`write_rel` with an id whose target offset is far enough
  beyond the current file size that one `grow_*_file()` application (grow
  factor applied to current size, floored at `min_growth` = 2 MB) is
  insufficient to cover it, and confirm the write panics today instead of
  succeeding
- [x] 1.4 Confirm (read-only, no fix yet) that all three tests in 1.1-1.3
  fail for the reason described in the proposal (wrap-to-zero bounds check
  passing, header read past `mmap.len()`, undersized grow) and not for an
  unrelated reason, so the later fix commits are validated against the
  right failure mode

## 2. Fix #2 — record-store overflow-safe offsets
- [x] 2.1 In `read_node` (`record_store_ops.rs:180-191`) and `write_node`
  (`:69-80`), replace the unchecked `node_id as usize * NODE_RECORD_SIZE`
  and `offset + NODE_RECORD_SIZE` with `checked_mul`/`checked_add`; treat
  overflow as `Error::NotFound` (read) / a hard error (write), matching the
  existing `Result` shape of each function
- [x] 2.2 Apply the same `checked_mul`/`checked_add` treatment to `read_rel`
  (`record_store_ops.rs:286-298`) and `write_rel` (`:264-275`) with
  `REL_RECORD_SIZE`
- [x] 2.3 WAIVED (not the availability waiver — a safety decision). `next_*_id`
  IS available, but gating READS by it would change read_node/read_rel's
  established contract: an unallocated-but-in-range id currently returns
  `Ok(zeroed)` (callers detect absence via the FLAG_ALLOCATED bit), and the
  gate would flip that to `Err`, rippling to every caller (e.g. a fresh store
  where next_node_id==0 would make read_node(0) Err instead of Ok(zeroed)).
  Gating WRITES is impossible by design (write_node accepts a new/arbitrary id
  >= next). Keeping the gate coherent for raw sparse writes would additionally
  require advancing next_*_id via fetch_max on every write — a public-API
  semantics change on the hot path. Checked arithmetic (2.1/2.2) + the existing
  file-size guard fully close the panic and the wrap without any contract
  change; the id-range hardening is deferred as disproportionate to a
  bounds-panic fix.
- [x] 2.4 Confirm the `id as usize` truncation-on-32-bit-targets concern
  from the proposal is closed by the same `checked_mul`/`checked_add`
  change (or, if the workspace does not target 32-bit, note that
  explicitly rather than silently dropping the concern)
- [x] 2.5 Make the §1.1 tests pass

## 3. Fix #3 — property-store header bounds
- [x] 3.1 Change the bounds guard in `get_entity_info_at_offset`
  (`property_store.rs:263-277`, currently `offset as usize >= self.mmap.len()`)
  to check the full header length: `offset.checked_add(HEADER_LEN)` (13
  bytes: 8-byte entity_id + 1-byte entity_type + 4-byte data_size) must not
  exceed `self.mmap.len()`, returning `None` otherwise
- [x] 3.2 Apply the equivalent header-length-aware guard to
  `load_properties_at_offset` (`property_store.rs:233-259`)
- [x] 3.3 Add checked bounds inside `read_u64` (`:667-678`), `read_u32`
  (`:681-688`), and `read_u8` (`:691-693`) themselves (or a shared helper
  they call), so a future caller that omits its own pre-check is still
  protected — per the proposal, these two call sites exist specifically to
  defend against corrupt pointers and must not be defeatable by the input
  they are meant to sanitize
- [x] 3.4 Confirm `repair_corrupt_node_prop_ptrs` (`record_store_ops.rs:133-139`)
  and `load_node_properties_inner:1158` inherit the fix through their call
  to `get_entity_info_at_offset` without further changes
- [x] 3.5 Make the §1.2 tests pass

## 4. Fix #4 — size file grow to the target offset
- [x] 4.1 Change `grow_nodes_file` (`record_store.rs:273-291`) so
  `new_size = calculated_size.max(self.nodes_file_size + min_growth).max(offset + NODE_RECORD_SIZE)`,
  threading the caller's target offset through the function signature
- [x] 4.2 Apply the same fix to `grow_rels_file` (`record_store.rs:295-311`)
  with `REL_RECORD_SIZE`
- [x] 4.3 Update the `write_node`/`write_rel` call sites
  (`record_store_ops.rs:72-80`, `:267-275`) to pass the target offset into
  `grow_nodes_file`/`grow_rels_file`
- [x] 4.4 Confirm the new sizing matches the already-correct pattern in
  `property_store::ensure_capacity` (`property_store.rs:620-641`,
  `.max(required_size)`) so the three storage files share one growth
  discipline
- [x] 4.5 Make the §1.3 test pass

## 5. Tail (docs + tests — check or waive with tailWaiver)
- [x] 5.1 Update `docs/specs/storage-format.md` with the bounds-checking
  contract (checked arithmetic, id-range gating, header-length guard,
  offset-aware grow) for the record and property stores; add a CHANGELOG
  entry
- [x] 5.2 Tests: all §1 regression tests kept in the suite and passing;
  add a normal (non-adversarial) large-id-jump write test that exercises
  the §4 grow-sizing fix without any corruption, to confirm the fix does
  not regress legitimate sparse-id writes
- [x] 5.3 Run `cargo +nightly fmt --all`,
  `cargo clippy --workspace --all-targets --all-features -- -D warnings`,
  `cargo +nightly test --workspace` — all green
- [x] Update or create documentation covering the implementation
- [x] Write tests covering the new behavior
- [x] Run tests and confirm they pass

## Related
- `phase0_fix-store-size-per-clone-divergence` — a fourth, related
  record-store bounds defect (stale per-clone file-size snapshot) in the
  same two files, tracked separately because its trigger and fix are
  structurally different (shared-state divergence, not arithmetic)
