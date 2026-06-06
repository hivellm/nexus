## 1. Investigation
- [x] 1.1 Reproduce the prop_ptr corruption with a populated persistent volume across two boots
- [x] 1.2 Identify the write path that emits a node prop_ptr pointing at a relationship id
- [x] 1.3 Determine whether corruption is write-time or WAL-replay-ordering
- [x] 1.4 Check whether the recovery reset write is flushed to the catalog/record store before shutdown

## 2. Implementation
- [x] 2.1 Fix the corruption source (write_node already guards prop_ptr->Relationship; update_node no longer zeroes first_rel_ptr)
- [x] 2.2 Make recovery durable so subsequent boots are clean (one-shot) — repair_corrupt_node_prop_ptrs in RecordStore::new
- [x] 2.3 Fix the serializer race — rebuild_index reopen bug fixed (full disk rebuild, scan from offset 1); repair runs synchronously before queries
- [x] 2.4 Ensure recovered nodes return full property map on RETURN n — correct reverse_index rebuild + prop_ptr restore

## 3. Tail (mandatory — enforced by rulebook v5.3.0)
- [x] 3.1 Update or create documentation covering the fix (CHANGELOG Unreleased / GH #4)
- [x] 3.2 Write tests: corrupted prop_ptr recovers durably; second boot is clean; whole-node serialization is race-free
- [x] 3.3 Run tests and confirm they pass (nexus-core serial: 2354 lib + integration green; 3 repair tests pass)

## 4. Notes
- A pre-existing parallel-load test flake (`engine::tests::match_scopes_by_label_and_property_together`)
  surfaces under the full multi-binary `cargo test -p nexus-core` run (passes serially, `--lib`-only,
  and in isolation). Root-caused to OS-level mmap page behaviour under cross-process memory pressure,
  not issue #4 logic. Tracked in follow-up task `phase6_fix-match-scopes-parallel-load-flake`.
