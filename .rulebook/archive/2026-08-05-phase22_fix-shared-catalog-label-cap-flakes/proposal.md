# Proposal: phase22_fix-shared-catalog-label-cap-flakes

> The 3 failures every workspace gate in this epic has reported and waved through
> as "documented flakes that pass in isolation".

## Why

They are not flakes, and the recorded diagnosis was wrong. Measured:

```
PROBE label id for ExistsProbeZ = Ok(77)   (label_bits cap is 64)
```

`Catalog::with_map_size` redirects EVERY catalog to one shared LMDB directory per
process while under test (`catalog/store.rs`, to avoid the Windows `TlsFull` error
from hundreds of LMDB environments). So all ~2770 lib tests share one label-id
sequence. A node record stores its labels as `label_bits: u64` — 64 labels, total —
so once the running id passes 64 the label is **silently dropped** and every
label-scoped match over it returns zero rows.

That is the whole mechanism, and the code already knew it: the comment on that
shared directory says allocating "ids past the 64-bit `label_bits` cap (and
silently drop newly-registered labels)" was "the root cause of the load-dependent
`match_scopes_*` flake". The same cause resurfaced in three more tests and was
filed as three unrelated flakes.

Two consequences worth stating plainly:

1. **`--test-threads=1` still fails them.** A serial run of the lib suite fails
   both `exists_` tests identically (2765 passed / 2 failed). So the recorded
   classification — "flake only in full ~2600-test parallel runs, resource-pressure
   family, NOT shared-catalog" — is refuted on both counts. Nothing here is about
   parallelism; the failure is deterministic given the label-registration ORDER
   ahead of the test.
2. **Suite correctness currently depends on that order.** A test whose label lands
   at id < 64 passes and the identical test lands broken at id ≥ 64. Adding a test
   that registers labels can therefore break an unrelated test elsewhere, silently.

`property_index_survives_restart` is the same cause seen from the other side: it
passes serially and fails in the parallel run, because the parallel ordering puts
more labels ahead of it. Its assertion (`find_exact` returns 1 hit after a restart)
collapses for exactly the same reason — the rebuilt index cannot resolve a label
the bitmap dropped.

## What Changes

Give the three tests an isolated catalog, which is what the existing helpers were
built for and what the shared-catalog comment recommends:

- the two `exists_` tests → `create_isolated_test_executor()` (documented as
  "preventing interference from parallel tests that share the default catalog");
- `property_index_survives_restart` → `Engine::with_isolated_catalog(&path)` on
  both opens, so the restart half reads back its own catalog rather than the
  process-wide one.

## Explicitly NOT changed, and why

- **The 64-label cap itself.** `NodeRecord.label_bits: u64` is the storage format
  (documented in the project guide). Widening it is a format change, not a test fix.
- **Making `get_or_create_label` fail loudly past the cap.** This is the right
  product behaviour — silently dropping a label is a silent wrong answer — but it
  cannot land while the test harness shares one catalog across ~2770 tests: the
  shared sequence passes 64 midway through every run, so a hard error would fail
  the suite wholesale. It needs per-test catalog isolation first, and that in turn
  needs the `TlsFull` constraint re-measured. Recorded as the follow-up rather than
  attempted here.

## Impact

- Affected code: crates/nexus-core/src/executor/eval/helpers/tests.rs,
  crates/nexus-core/src/engine/tests/transactions.rs
- Breaking change: NO — test-only.
- User benefit: the workspace gate goes to zero failures, so a real regression
  stops having three permanent failures to hide behind. Every gate in this epic so
  far has had to argue "those 3 are the known ones" from memory rather than read a
  green result.
