## 1. Reproduce & confirm root cause
- [x] 1.1 Reliably reproduce the failure (full multi-binary `cargo +nightly test -p nexus-core`; failure rate tracks machine load)
- [x] 1.2 Instrument the scoped query: `count(:X)=4` for 2 nodes, internal ids `[0,1,2,3]` = X nodes AND Y nodes
- [x] 1.3 Ruled out in-process race (16x50 stress = 0) and double-CREATE (edge sanity = 8; total_nodes = 7)
- [x] 1.4 Confirmed: `MATCH (:X)` also matches `:Y` nodes — labels X and Y resolve to the SAME label-id under load
- [x] 1.5 Pinned the source: `get_or_create_label/_type/_key` allocate ids from a per-`Catalog`-instance in-memory counter; multiple instances on one shared LMDB env hand out duplicate ids

## 2. Implementation
- [x] 2.1 Allocate ids from the committed LMDB max INSIDE the write txn (`alloc_label_id`/`alloc_type_id`/`alloc_key_id`); LMDB serialises writers across instances/processes -> unique ids. Applied to single + batch label/type and single key/type allocators.
- [x] 2.2 Defense-in-depth: process-scope the shared test catalog + auth LMDB dirs (`..._<pid>`) so concurrent test binaries don't share/wipe one env
- [x] 2.3 `match_scopes_by_label_and_property_together` now passes deterministically: 8/8 green under full parallel `cargo test -p nexus-core`

## 3. Tail (mandatory — enforced by rulebook v5.3.0)
- [x] 3.1 Documentation: CHANGELOG Unreleased entry
- [x] 3.2 Tests: `match_scopes_by_label_and_property_together` is the load regression guard (8/8 under full parallel); catalog allocation correctness covered by the existing catalog tests + the integration assertion
- [x] 3.3 Run tests and confirm they pass (8/8 full parallel runs; nexus-core lib + integration green; clippy/fmt clean)

## Notes (resolution)
Root cause was a real product concurrency bug, not just a test artifact: id
allocation used a per-instance in-memory counter, so any two `Catalog`
instances (or processes) sharing one LMDB env could assign the same id to
different label/type/key names -> label confusion (`get_nodes(id)` returns
both labels' nodes). The shared TEST catalog made many instances hammer one
env, exposing it under load. Fix makes allocation atomic w.r.t. the LMDB env.
