# 03 — Performance Results

All numbers verbatim from `docs/PERFORMANCE.md`, `docs/performance/PERFORMANCE_V1.md`, `docs/performance/BENCHMARK_NEXUS_VS_NEO4J.md`, `docs/performance/BENCHMARK_RESULTS_PHASE9.md`, `docs/performance/PERFORMANCE_ANALYSIS.md`. Reference hardware: **Ryzen 9 7950X3D** (Zen 4: AVX-512F + VPOPCNTQ + AVX2 + SSE4.2 + FMA), 64 GB DDR5-6000, Windows 10 + MSVC, Rust nightly 1.85+ edition 2024.

## 1. Wire / protocol (RPC vs HTTP/JSON)

Acceptance thresholds (loopback, warm page cache):

| Scenario | HTTP baseline | RPC target | Required speedup |
|----------|---------------|------------|------------------|
| Point read (MATCH by id) | 320 µs p50 | ≤ 120 µs | ≥ 2.6× |
| 3-hop pattern | 1.2 ms p50 | ≤ 700 µs | ≥ 1.7× |
| KNN k=10 dim=768 | 4.8 ms p50 | ≤ 2.5 ms | ≥ 1.9× |
| Bulk ingest 10 K nodes/frame | 780 ms | ≤ 220 ms | ≥ 3.5× |
| Pipelined 1 K queries | n/a (HTTP serial) | ≤ 40 ms | — |

Headline: **3–10× lower latency, 40–60 % smaller payloads** vs HTTP/JSON. Wire-codec lower bound (encode + decode of MessagePack frame, no I/O) sits in the low microseconds.

## 2. SIMD kernels (runtime-dispatched)

Cascade: **AVX-512F → AVX2+FMA → SSE4.2 → NEON → Scalar**. Selected at first use, cached in `OnceLock<unsafe fn>`. All paths proptest-checked for parity vs scalar.

| Op | Scalar | AVX-512 | Multiplier | Goal |
|----|--------|---------|-----------|------|
| `dot_f32` @ d=768 | 438 ns | 34.5 ns | **12.7×** | ≥ 4× ✅ |
| `l2_sq_f32` @ d=512 | 285 ns | 21.0 ns | **13.5×** | ≥ 4× ✅ |
| `popcount_u64` @ 4 KiB | 1.52 µs | 136 ns (VPOPCNTQ) | **≈11×** | ≥ 5× ✅ |
| `sum_f64` @ 262 144 rows | 150 µs | 19 µs | **7.9×** | ≥ 4× ✅ |
| `sum_f32` @ 262 144 rows | 152 µs | 9.5 µs | **15.9×** | ≥ 4× ✅ |
| `lt_i64` filter @ 262 144 rows | 110 µs | 25 µs | **4.4×** | ≥ 3× ✅ |
| `eq_i64` filter @ 262 144 rows | 69 µs | 24 µs | **2.9×** | ≥ 3× — ✅ |
| RLE `find_run_length` @ 16 K uniform | 3.2 µs | 1.0 µs | **3.2×** | ≥ 3× ✅ |

## 3. Cypher parser & WAL CRC

| Op | Before | After | Multiplier |
|----|--------|-------|-----------|
| Parser @ 31.5 KiB query | ~1.07 s (O(N²) — `chars().nth()`) | 3.7 ms (O(N) — `chars().next()`) | **≈290×** |
| WAL CRC throughput @ 64 KiB | 3.5 µs (`crc32fast`) | 7.0 µs (`crc32c` HW) | **`crc32fast` wins 2×** on Zen 4 |

Honesty note: hardware CRC32C is *slower* than `crc32fast`'s 3-way PCLMUL on Zen 4 across 256 B / 4 KiB / 64 KiB buffers. Default kept at `crc32fast`; dual-format frame header allows opt-in CRC32C when AVX-512 VPCLMULQDQ lands or ZFS interop required.

## 4. Full-text search (Tantivy 0.22)

Corpus: 100 K synthetic 1 KB docs, 75-word vocab, standard analyzer (lowercase + English stopwords).

| Scenario | Median | SLO | Margin |
|----------|--------|-----|--------|
| Single-term (`limit=10`) | 150 µs | < 5 ms p95 | **≈33×** under SLO |
| 2-term phrase `"quick brown"` | 4.57 ms | < 20 ms p95 | **≈4.4×** under SLO |
| Bulk ingest sync @ 10 K docs | ~60 K docs/s | > 5 K docs/s | **≈12×** |
| Bulk ingest async @ 10 K docs | ~60 K docs/s + 1× refresh cadence | > 5 K docs/s | ceiling held |

Async writer (`enable_async_writers()`) decouples enqueue latency from Tantivy commit latency: enqueue returns in µs, background thread batches up to 256 docs and commits on `refresh_ms = 1000 ms` cadence. Sync per-doc path is floored by Tantivy segment-flush — bulk-sync wraps N docs in one writer + commit and hits the ≈60 K docs/s ceiling.

## 5. KNN / HNSW

Per-label HNSW, configurable `ef_construction` and `M`. Cosine / L2 / dot metrics. Bytes-native embeddings on the RPC wire (no base64 tax).

End-to-end protocol benchmark target (above): **KNN k=10 @ d=768 — HTTP 4.8 ms p50, RPC ≤ 2.5 ms p50** (≥ 1.9× speedup gate). Kernel-level dot @ d=768 already at 12.7× scalar; the gate is set at 1.9× because RPC latency is dominated by transport + dispatcher + result serialisation, not by the distance kernel.

**Gap** — no recall / quality numbers in the repo. KNN benchmarks are latency-only. Recall@k vs ground-truth ANN is a missing public artifact.

## 6. Concurrency model (the most important number in this report)

From [`docs/performance/PERFORMANCE_ANALYSIS.md`](../../performance/PERFORMANCE_ANALYSIS.md), workload: 1 K Person + 100 Company + 500 WORKS_AT, mixed read query, Nov 2025 build.

| Engine | Throughput | CPU util | Architecture |
|--------|-----------|----------|--------------|
| **Nexus** | **202.7 qps** | ~12 % (7 of 8 cores idle) | Single global `Arc<RwLock<Executor>>` exclusive lock |
| **Neo4j** | **507.2 qps** | ~80 %+ | Thread pool + per-tx MVCC isolation |
| Ratio | 2.5× slower | 6.7× lower | — |

**Root cause** (audit): `let mut executor = executor_guard.write().await; executor.execute(&query)` — every query takes the writer lock; there is no concurrent-reader execution path.

**Phase 9 fix** (Nov 20 2025) recovered baseline 162 qps → **603.91 qps (3.7× improvement)** by switching to `parking_lot::RwLock` + wrapping handlers in `tokio::task::spawn_blocking`. **However** — there is no published post-fix re-run of the 74-test vs-Neo4j suite. The category-level absolute-throughput numbers in §8 below are pre-fix.

**This is the single most important benchmark Nexus needs to re-publish.**

## 7. Phase 9 (Nov 2025) — read-heavy throughput recovery

| Metric | Baseline | Post-Phase-9 | Multiplier |
|--------|----------|--------------|-----------|
| Mixed read throughput (qps) | 162 | 603.91 | **3.7×** |

Phase 9 also reports residual gaps:
- COUNT / AVG / GROUP BY still 18–45 % slower than Neo4j (no diagnostic breakdown in repo).
- Relationship traversal still 37–57 % behind Neo4j on some shapes despite Phase 8 + 9 work.

These are the **next two attack surfaces**.

## 8. End-to-end vs Neo4j — 74-test suite (Dec 2025, v0.12.0, Neo4j Community)

| Metric | Value |
|--------|-------|
| Total tests | 74 |
| Compatible (identical row counts) | 52 (70.3 %) |
| Nexus faster | **73 (98.6 %)** |
| Neo4j faster | 1 (1.4 %) — `sqrt` (cold-cache run, results identical) |
| **Average speedup** | **4.15×** |
| **Max speedup** | **42.74× (relationship creation)** |

By category:

| Category | Tests | Avg Neo4j (ms) | Avg Nexus (ms) | Avg speedup | Compatible |
|----------|-------|----------------|----------------|-------------|------------|
| Creation | 2 | 67.83 | 2.27 | **25.46×** | 50 % |
| Match | 7 | 2.69 | 1.34 | **1.99×** | 71 % |
| Aggregation | 7 | 3.00 | 1.31 | **2.28×** | 86 % |
| Traversal | 5 | 40.01 | 3.04 | **11.31×** | 80 % |
| String | 8 | 2.39 | 1.12 | **2.14×** | 63 % |
| Math | 8 | 2.70 | 1.73 | **2.42×** | 100 % |
| List | 8 | 2.04 | 0.95 | **2.16×** | 100 % |
| Null | 5 | 2.11 | 0.98 | **2.15×** | 100 % |
| Case | 3 | 2.14 | 1.06 | **2.03×** | 67 % |
| Union | 3 | 3.95 | 1.51 | **2.57×** | 67 % |
| Optional | 2 | 5.04 | 1.85 | **2.60×** | 0 % |
| With | 3 | 2.29 | 1.14 | **2.01×** | 0 % |
| Unwind | 2 | 2.46 | 1.29 | **1.91×** | 100 % |
| MERGE | 3 | 2.56 | 0.44 | **5.69×** | 100 % |
| TypeConv | 4 | 2.61 | 1.13 | **2.32×** | 100 % |
| Write | 2 | 2.61 | 0.73 | **3.59×** | 0 % |

Standout wins:
1. **Create relationships — 42.74×** (linked-list O(1) insert vs Neo4j's adjacency append)
2. **Two-hop traversal — 31.98×** (linked-list pointer chase vs Neo4j's traversal API)
3. **Return path data — 8.90×**
4. **Create 100 nodes — 8.18×**
5. **MERGE — 5.69×**

**Caveat — compatibility column.** Categories at 0 % compatible (`Optional`, `With`, `Write`) are not "wrong answers" — they're row-count differences in projection / serialisation (Neo4j returns extra wrapper rows). Worth resolving in v1.x for cleaner messaging.

## 9. Memory profile

From [`MEMORY_TUNING.md`](../../performance/MEMORY_TUNING.md):

| State | RSS |
|-------|-----|
| Cold start (default) | ~220 MB |
| Cold start (tuned, minimum config) | ~22 MB |

Tunable via env vars (page-cache size, WAL buffer, planner cache). No under-load profile vs Neo4j is published.

## 10. Honest measurement caveats

| Decision | Reason |
|----------|--------|
| Default `crc32fast` over CRC32C HW | 2× faster on Zen 4 due to PCLMUL parallelism; HW CRC32C wins only on cores w/o PCLMUL |
| Default `serde_json` over `simd-json` for `/ingest` | DOM `Value` target negates simd-json's typed-deser advantage; simd-json was 1.32–1.48× *slower* on 70 KiB / 1 MiB |
| No SIMD record codec batch | LLVM auto-vectorises `ptr::copy_nonoverlapping` over `#[repr(C)]` PODs to `vmovdqu` already |
| Columnar fast-path real-world ratio ~1.13× | SIMD compare is saturated; wall time dominated by `serde_json::Value::Object` materialisation + `compute_row_dedup_key` |

This level of negative-result transparency is uncommon and is itself a marketing asset.

## 11. What's NOT benchmarked yet

1. **Post-Phase-9 vs Neo4j re-run** — repo numbers are pre-fix.
2. **V2 cluster perf** — no Raft consensus latency, cross-shard query, or replication-overhead benchmarks.
3. **KNN recall @ k vs ground-truth** — only latency.
4. **Variable-length paths `(a)-[*1..3]-(b)`** — no perf breakdown.
5. **RESP3 transport parity** — phase3 task expanded the suite but results not in repo.
6. **Memory under sustained concurrent load** — only cold-start RSS.
7. **vs Memgraph 3.4 / ArangoDB 3.12 / KuzuDB-fork (Bighorn) / FalkorDB** — no published comparison; Neo4j is the only benchmarked competitor.
8. **Billion-row KNN** — corpus capped at 100 K for FTS; no published HNSW @ 100 M+ vectors.
9. **GraphRAG end-to-end** — no LangChain/LlamaIndex round-trip benchmark vs Neo4j Aura Agent / Memgraph GraphRAG toolkit.

These gaps inform [10_improvement_roadmap.md](10_improvement_roadmap.md).
