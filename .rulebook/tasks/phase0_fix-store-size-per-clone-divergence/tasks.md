# Tasks: phase0_fix-store-size-per-clone-divergence

`RecordStore::nodes_file_size`/`rels_file_size` (`record_store.rs:59-61`) are
plain per-clone `usize` fields copied by value on `Clone` (`:375-376`), while
the mmaps they bound-check against are shared via `Arc<RwLock<MmapMut>>`
(`:37-39`, `Arc::clone`d at `:368-369`). A clone's cached size can diverge
from the shared mmap's real length: a grow (`record_store.rs:273-311`)
replaces the shared mmap but updates only the caller's own size, so a stale
clone under-reports capacity (spurious `NotFound`, `record_store_ops.rs:182`/
`:288`); `clear_all` (`record_store_ops.rs:1064-1087`) shrinks the shared
mmap but likewise updates only the caller's own size, so a stale clone
over-reports capacity and slices past the now-smaller mmap (OOB panic).

Order matters: reproduce both divergence directions with a failing
concurrency test first (§1), because this defect only manifests across two
`RecordStore` clones observing the shared mmap at different times — a
single-instance test cannot show it. Fix the read path to stop trusting the
cached field at all (§2) before touching `clear_all`'s admin-flow refresh
(§3), because §2 is the structural fix that removes the divergence as a
category; §3 is defense-in-depth for the specific `clear_all` window and is
only meaningful once §2 establishes what "correct" bound-checking looks
like.

## 1. Reproduce both divergence directions
- [ ] 1.1 Write a failing test: create a `RecordStore`, clone it (as
  `refresh_executor` would), grow the ORIGINAL past the clone's cached
  `nodes_file_size` (write enough nodes via the original to force
  `grow_nodes_file`), then call `read_node` on the CLONE for a node id that
  exists in the grown region. Confirm it returns `NotFound` today even
  though the node exists in the shared mmap
- [ ] 1.2 Write a failing test: create a `RecordStore`, write nodes past
  `INITIAL_NODES_FILE_SIZE` so `nodes_file_size` grows beyond the initial
  value, clone it, then call `clear_all` on the ORIGINAL (shrinking the
  shared mmap back to `INITIAL_NODES_FILE_SIZE` while the clone's cached
  `nodes_file_size` stays at the pre-clear large value), then call
  `read_node` on the CLONE for a node id whose offset is beyond
  `INITIAL_NODES_FILE_SIZE` but within the clone's stale cached size.
  Confirm it panics today (OOB slice) instead of returning `NotFound`.
  Repeat for `read_rel` if the id-range gate (`next_rel_id`) does not
  already close this specific case — record which it is
- [ ] 1.3 Confirm both tests fail for the reason described in the proposal
  (stale-small size → spurious NotFound; stale-large size → OOB panic) and
  not for an unrelated reason (e.g. an id-range gate intercepting the
  request before the size check is ever reached) — if an id-range gate
  does intercept in 1.2, adjust the test to bypass it (e.g. by choosing an
  id below `next_node_id` but above the shrunk physical size) so the size-
  check divergence itself is what is being exercised

## 2. Bound-check against the live shared mapping, not a cached field
- [ ] 2.1 Change `read_node` (`record_store_ops.rs:180-191`) to acquire the
  `nodes_mmap` read lock first and bound-check `start`/`end` against
  `guard.len()` (the live mapping length), removing the
  `self.nodes_file_size` comparison from the request path — mirroring the
  existing correct pattern in `read_all_node_headers`
  (`record_store.rs:255-258`)
- [ ] 2.2 Apply the same live-length bound-check to `read_rel`
  (`record_store_ops.rs:284-298`)
- [ ] 2.3 Decide whether `write_node`/`write_rel`'s pre-grow-check
  (`record_store_ops.rs:69-80`, `:264-275`) also needs the same treatment,
  or whether the cached size field is acceptable there because the
  single-writer model (per `docs/specs/wal-mvcc.md`) means no concurrent
  clone can shrink the mmap out from under an in-flight write; document the
  decision in this task's checklist item, not silently
- [ ] 2.4 If any read/write path still needs a size value OUTSIDE the mmap
  lock (e.g. for a capacity-planning decision before acquiring the write
  lock), replace the plain `usize` fields with `Arc<AtomicUsize>` updated
  under the same mmap write lock at grow time, so every clone observes the
  update without needing a fresh `Clone`/`refresh_executor`
  [tailWaiver: only if no such outside-the-lock use remains after 2.1-2.3,
  in which case state that the fields are now read-only-under-lock and the
  Arc<AtomicUsize> change is unnecessary]
- [ ] 2.5 Make the §1.1 test pass (grown region now readable from the stale
  clone)

## 3. Harden the `clear_all` admin-flow window
- [ ] 3.1 Make the §1.2 test pass via the §2 fix alone if possible (live
  mapping length bound-check should already prevent the OOB panic
  independent of any stale cached field); confirm and record whether §2
  alone is sufficient
- [ ] 3.2 Regardless of 3.1, wire `Engine::clear_all_data`
  (`crates/nexus-core/src/engine/maintenance.rs:128-137`) to refresh any
  cached executor state after `self.storage.clear_all()` (belt-and-
  suspenders: closes the specific `clear_all` staleness window promptly
  instead of leaving it to the next natural `refresh_executor`)
- [ ] 3.3 Confirm the fix does not reintroduce the divergence for
  `next_node_id`/`next_rel_id` (already correctly shared via
  `Arc<AtomicU64>`, `record_store.rs:45-47`) — no change needed there, this
  is a check, not new work

## 4. Tail (docs + tests — check or waive with tailWaiver)
- [ ] 4.1 Update `docs/specs/storage-format.md` with the bound-checking
  contract: reads must check against the live mmap length under the same
  lock acquisition used to read the data, not a per-clone cached size; add
  a CHANGELOG entry
- [ ] 4.2 Tests: both §1 regression tests kept in the suite and passing;
  add a concurrent grow-then-read test and a concurrent clear_all-then-read
  test (real threads, not just sequential clone simulation) to cover the
  actual concurrency shape described in the proposal
- [ ] 4.3 Run `cargo +nightly fmt --all`,
  `cargo clippy --workspace --all-targets --all-features -- -D warnings`,
  `cargo +nightly test --workspace` — all green

## Related
- `phase0_fix-storage-oob-panics` — sibling record-store bounds defects
  (arithmetic overflow, property-store header over-read, undersized grow)
  in the same two files, found by the same audit pass but structurally
  independent of this shared-vs-cached-state divergence
