# 01 — Measuring the relationship-ingest bottleneck

Nexus loads LDBC SF0.1 ~16× slower than Neo4j (~480 s vs ~30 s). Before
touching code, this localised the cost empirically. All numbers are from a
release build, `POST /ingest` with `use_batching: false`, localhost, fresh
`NEXUS_DATA_DIR`, on the bench box.

## What is slow

| Operation | Rate |
|---|---:|
| Node ingest | ~8 500 nodes/s |
| **Relationship ingest** | **~3 000 rel/s** |
| Neo4j relationship load (baseline) | ~60 000 rel/s |

Relationships are the bottleneck: ~3× slower per row than nodes on Nexus, and
~20× slower than Neo4j.

## Hypotheses tested and RULED OUT

Each was a controlled micro-benchmark; the numbers are what killed the
hypothesis.

1. **"It is O(degree) — hub nodes accumulate a long outgoing chain."**
   REFUTED. Creating 12 000 edges from one hub in growing slices held a
   constant ~2 700 rel/s (0→2k→…→12k). The outgoing chain insert is a prepend
   (O(1)), confirmed in `create_relationship` (`record.next_src_ptr =
   source_prev_ptr; node.first_rel_ptr = rel_id + 1`).
   - A first run measured 183 rel/s for a same-source burst; it did NOT
     reproduce (fresh node, node already at degree 10 000, and node id 0 all
     gave ~3 100 rel/s). It was warm-up noise. Re-measuring is why it was not
     mistaken for the cause.

2. **"Batching amortises it."** REFUTED. Rate is FLAT across request batch
   sizes: 100 → 2 307, 1 000 → 2 660, 5 000 → 2 182, 20 000 → 2 713 rel/s. The
   cost is PER ROW, not per request. The `/ingest` handler already holds one
   `engine.write()` for the whole batch, so the per-row cost is *inside*
   relationship creation, below the handler.

3. **"It degrades as the graph grows (a scan)."** REFUTED. Distinct-source
   edges held ~2 200–2 700 rel/s whether the graph had 10 k or 30 k edges. The
   backward `first_rel_ptr` scan (capped at 100, `record_store_ops.rs:766`)
   only fires for a node's *first* outgoing edge when its pointer reads 0, and
   did not move the number.

## Conclusion of the measurement pass

The cost is a **fixed per-relationship cost, ~0.33 ms/edge, that batching
cannot amortise and graph size does not affect.** That points below the
`/ingest` handler, into what `Engine::create_relationship` does for every
single row. See [02](02_root_cause.md).
