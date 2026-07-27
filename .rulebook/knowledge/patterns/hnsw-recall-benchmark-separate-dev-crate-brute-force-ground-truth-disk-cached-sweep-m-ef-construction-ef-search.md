# HNSW recall benchmark: separate dev crate, brute-force ground truth, disk-cached, sweep (M, ef_construction, ef_search)

**Category**: benchmarking
**Tags**: hnsw, ann, recall, benchmarking, vector-search, nexus

## Description

For HNSW (or other ANN) recall benchmarks, the right architecture is a *dev* crate separate from the production runtime. It depends on the engine crate directly (so it can sweep build-time parameters that aren't exposed via the RPC surface) and computes brute-force ground truth in cosine/L2 with disk caching keyed by `(base_shape, query_shape, k, first/last vector bits)`. The sweep grid is `M ∈ {8,16,32,64} × ef_construction ∈ {100,200,400,800}` (outer loop, rebuilds index) × `ef_search ∈ {50,100,200,400}` (inner loop, reuses index). Output JSON+CSV with the column triple `recall_at_1 / recall_at_10 / recall_at_100` to stay apples-to-apples with Pinecone, Weaviate, Qdrant, Milvus.

## Example

// Engine surface needed:
impl KnnIndex {
    pub fn search_knn_with_ef(&self, q: &[f32], k: usize, ef: usize) -> Result<Vec<(u64, f32)>>;
}

// Sweep loop:
for &m in &m_values {
    for &ef_c in &ef_construction_values {
        let idx = KnnIndex::with_config(dim, KnnConfig { max_connections: m, ef_construction: ef_c, ... })?;
        for v in &base { idx.add_vector(...)?; }
        for &ef_s in &ef_search_values {
            let mut samples = Vec::new();
            let mut tops = Vec::new();
            for q in &queries {
                let t = Instant::now();
                let r = idx.search_knn_with_ef(q, k, ef_s)?;
                samples.push(t.elapsed());
                tops.push(r.into_iter().map(|(id,_)| id as u32).collect());
            }
            cells.push(SweepCell { recall: summarise_recall(&tops, &gt.top_k), latency: summarise_latency(&samples), ... });
        }
    }
}

## When to Use

Whenever a vector DB needs to publish defensible recall numbers alongside latency. Existing latency-only benches mislead — recall and latency must be measured together.

## When NOT to Use

Do not bolt the recall harness onto a comparative RPC bench (like `nexus-bench`) that has a deliberate no-core-dep guardrail; HNSW parameter sweeps require in-process access to the engine. Don't compute ground truth on every run — cache it.
