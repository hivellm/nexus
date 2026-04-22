//! Ranking regression tests for the FTS backend
//! (phase6_fulltext-benchmarks §2).
//!
//! A fixed, hand-curated corpus mimicking the MS-MARCO short-passage
//! shape (title + body, ~50-150 words). Each query has a golden
//! top-N document set. If Tantivy's BM25 output changes — e.g.
//! because the stopword list, analyzer chain, or scoring defaults
//! drift between releases — these assertions fail loudly, which is
//! exactly what a regression test is for.
//!
//! The corpus is small (10 docs, 4 queries) on purpose: golden
//! ranking tests need to be human-auditable. Density-of-coverage
//! is the job of the criterion bench, not this suite.

use nexus_core::index::fulltext_registry::FullTextRegistry;
use tempfile::TempDir;

/// Returns the top-N node ids from a query, ordered by score desc
/// (ties broken by the registry's stable insert order).
fn top_ids(reg: &FullTextRegistry, index: &str, query: &str, n: usize) -> Vec<u64> {
    let results = reg
        .query(index, query, Some(n))
        .unwrap_or_else(|e| panic!("query `{query}` failed: {e}"));
    results.into_iter().map(|r| r.node_id).collect()
}

fn seeded_registry() -> (FullTextRegistry, TempDir) {
    let dir = TempDir::new().unwrap();
    let reg = FullTextRegistry::new();
    reg.set_base_dir(dir.path().to_path_buf());
    reg.create_node_index("corpus", &["Doc"], &["body"], Some("standard"))
        .unwrap();
    // Ten documents; the "graph database" family dominates 1..=4,
    // "vector search" dominates 5..=7, outliers 8..=10.
    let docs: [(u64, &str); 10] = [
        (
            1,
            "graph database traversal patterns and node relationships",
        ),
        (2, "graph algorithms over property graphs and indexed edges"),
        (
            3,
            "graph databases outperform relational stores for deep joins",
        ),
        (
            4,
            "property graph model with typed relationships and labels",
        ),
        (
            5,
            "vector search with cosine similarity and approximate nearest neighbours",
        ),
        (
            6,
            "dense vector embeddings power semantic search and retrieval",
        ),
        (7, "vector indexes accelerate nearest neighbour lookups"),
        (8, "full text search with BM25 ranking and phrase queries"),
        (
            9,
            "tantivy provides a fast embedded full text engine in Rust",
        ),
        (10, "the quick brown fox jumps over the lazy dog"),
    ];
    for (id, body) in docs {
        reg.add_node_document("corpus", id, 0, 0, body).unwrap();
    }
    (reg, dir)
}

#[test]
fn graph_family_dominates_graph_query() {
    let (reg, _dir) = seeded_registry();
    let top3 = top_ids(&reg, "corpus", "graph", 3);
    for id in &top3 {
        assert!(
            (1..=4).contains(id),
            "top-3 for `graph` must come from the graph family, got {top3:?}"
        );
    }
}

#[test]
fn vector_family_dominates_vector_query() {
    let (reg, _dir) = seeded_registry();
    let top3 = top_ids(&reg, "corpus", "vector", 3);
    for id in &top3 {
        assert!(
            (5..=7).contains(id),
            "top-3 for `vector` must come from the vector family, got {top3:?}"
        );
    }
}

#[test]
fn phrase_query_pins_exact_match() {
    let (reg, _dir) = seeded_registry();
    // Only doc 8 has "BM25 ranking"; phrase query must surface it
    // as the top hit and exclude docs that share only one of the
    // terms.
    let top = top_ids(&reg, "corpus", "\"BM25 ranking\"", 3);
    assert_eq!(top.first(), Some(&8), "phrase pin missing, got {top:?}");
    // `BM25` and `ranking` appear together only in doc 8, so other
    // docs must not match a strict phrase query.
    assert_eq!(top.len(), 1, "phrase query must not bleed, got {top:?}");
}

#[test]
fn lazy_dog_phrase_hits_only_doc10() {
    let (reg, _dir) = seeded_registry();
    // "lazy dog" appears verbatim only in doc 10 — a canonical pan-
    // gram sanity check.
    let top = top_ids(&reg, "corpus", "\"lazy dog\"", 5);
    assert_eq!(top, vec![10], "pangram pin broken, got {top:?}");
}

#[test]
fn boolean_must_narrows_down_candidates() {
    let (reg, _dir) = seeded_registry();
    // `graph AND algorithms` should only reach doc 2.
    let top = top_ids(&reg, "corpus", "+graph +algorithms", 5);
    assert_eq!(
        top.first(),
        Some(&2),
        "boolean-must missed doc 2, got {top:?}"
    );
}

#[test]
fn query_without_hits_returns_empty() {
    let (reg, _dir) = seeded_registry();
    let top = top_ids(&reg, "corpus", "kryptonite", 10);
    assert!(
        top.is_empty(),
        "unmatched query must return empty, got {top:?}"
    );
}

#[test]
fn limit_respected_on_dense_matches() {
    let (reg, _dir) = seeded_registry();
    // "search" hits docs 5, 6, 8 (and possibly others via stem) —
    // limit=2 must truncate.
    let top = top_ids(&reg, "corpus", "search", 2);
    assert_eq!(top.len(), 2, "limit not respected, got {top:?}");
}
