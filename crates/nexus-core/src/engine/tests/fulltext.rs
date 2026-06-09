//! Tests for full-text search: DDL, query, WAL replay, auto-population,
//! eviction on DELETE/REMOVE/SET, analyzer catalogue, and null-key MERGE.

use super::*;

// phase6_opencypher-fulltext-search — end-to-end FTS DDL + query.
#[test]
fn fulltext_search_ddl_and_query_roundtrip() {
    let (mut engine, _ctx) = crate::testing::setup_test_engine().unwrap();

    // Register the index via CALL.
    let r = engine
        .execute_cypher("CALL db.index.fulltext.createNodeIndex('docs', ['Doc'], ['body'])")
        .expect("createNodeIndex must succeed");
    assert!(!r.rows.is_empty(), "createNodeIndex must return a row");
    assert_eq!(r.rows[0].values[0], serde_json::json!("docs"));
    assert_eq!(r.rows[0].values[1], serde_json::json!("ONLINE"));

    // db.indexes() must list the FULLTEXT row.
    let ixs = engine.execute_cypher("CALL db.indexes()").unwrap();
    let has_fts = ixs.rows.iter().any(|row| {
        row.values[1] == serde_json::json!("docs") && row.values[5] == serde_json::json!("FULLTEXT")
    });
    assert!(has_fts, "db.indexes() must include the docs FULLTEXT row");

    // Feed two documents through the registry (bypassing the
    // MATCH/SET wiring — the registry's public add API is exercised
    // here; the executor's CREATE-hook follow-up auto-populates).
    let registry = engine.indexes.fulltext.clone();
    registry
        .add_node_document("docs", 1, 0, 0, "the quick brown fox")
        .unwrap();
    registry
        .add_node_document("docs", 2, 0, 0, "a sleepy cat on a mat")
        .unwrap();

    // Query through the Cypher procedure surface.
    let r = engine
        .execute_cypher("CALL db.index.fulltext.queryNodes('docs', 'fox')")
        .unwrap();
    assert!(
        !r.rows.is_empty(),
        "queryNodes should return at least one row for `fox`"
    );
    let node = &r.rows[0].values[0];
    assert_eq!(node["_nexus_id"], serde_json::json!(1));

    // Drop removes the index.
    let r = engine
        .execute_cypher("CALL db.index.fulltext.drop('docs')")
        .unwrap();
    assert_eq!(r.rows[0].values[1], serde_json::json!("DROPPED"));

    // Subsequent query errors out.
    let err = engine
        .execute_cypher("CALL db.index.fulltext.queryNodes('docs', 'anything')")
        .expect_err("dropped index must raise ERR_FTS_INDEX_NOT_FOUND");
    assert!(err.to_string().contains("ERR_FTS_INDEX_NOT_FOUND"));
}

// phase6_fulltext-analyzer-catalogue — listAvailableAnalyzers surface.
#[test]
fn fulltext_list_available_analyzers_exposes_catalogue() {
    let (mut engine, _ctx) = crate::testing::setup_test_engine().unwrap();
    let r = engine
        .execute_cypher("CALL db.index.fulltext.listAvailableAnalyzers()")
        .unwrap();
    let names: Vec<String> = r
        .rows
        .iter()
        .map(|row| row.values[0].as_str().unwrap().to_string())
        .collect();
    for expected in [
        "english",
        "french",
        "german",
        "keyword",
        "ngram",
        "portuguese",
        "simple",
        "spanish",
        "standard",
        "whitespace",
    ] {
        assert!(
            names.iter().any(|n| n == expected),
            "listAvailableAnalyzers missing {expected:?}, got {names:?}"
        );
    }
    // Alphabetical order.
    let mut sorted = names.clone();
    sorted.sort();
    assert_eq!(names, sorted, "analyzer rows must be alphabetical");
}

// phase6_fulltext-analyzer-catalogue — config map picks the analyzer.
#[test]
fn fulltext_create_index_honours_config_analyzer() {
    let (mut engine, _ctx) = crate::testing::setup_test_engine().unwrap();
    engine
        .execute_cypher(
            "CALL db.index.fulltext.createNodeIndex('imgs', ['Image'], ['caption'], \
             {analyzer: 'ngram', ngram_min: 2, ngram_max: 3})",
        )
        .expect("createNodeIndex with ngram config must succeed");
    let ixs = engine.execute_cypher("CALL db.indexes()").unwrap();
    let analyzer_cell = ixs
        .rows
        .iter()
        .find(|row| row.values[1] == serde_json::json!("imgs"))
        .expect("imgs index should appear in db.indexes()");
    // The `options` column (last) carries the resolved analyzer for
    // FTS rows.
    let options = analyzer_cell.values.last().expect("options column");
    let analyzer = options
        .get("analyzer")
        .and_then(|v| v.as_str())
        .expect("analyzer key in options map");
    assert_eq!(analyzer, "ngram(2,3)");
}

// phase6_fulltext-wal-integration §4 — CREATE auto-populates the
// matching FTS index without any explicit add_node_document call.
#[test]
fn fulltext_create_node_auto_populates_matching_index() {
    let (mut engine, _ctx) = crate::testing::setup_test_engine().unwrap();
    engine
        .execute_cypher(
            "CALL db.index.fulltext.createNodeIndex('movies', ['Movie'], ['title', 'overview'])",
        )
        .unwrap();
    // Creating a Movie with matching properties should automatically
    // land the node in the FTS index.
    engine
        .execute_cypher(
            "CREATE (:Movie {title: 'The Matrix', overview: 'A computer hacker discovers reality'})",
        )
        .unwrap();
    let r = engine
        .execute_cypher("CALL db.index.fulltext.queryNodes('movies', 'matrix')")
        .unwrap();
    assert!(
        !r.rows.is_empty(),
        "expected the auto-populated Movie to surface via queryNodes"
    );
}

// phase6_fulltext-wal-integration §5 — WAL replay (simulated crash
// recovery). Emits a sequence of FTS WAL entries, feeds each one
// through `FullTextRegistry::apply_wal_entry` on a fresh registry,
// and confirms every committed row is queryable. Mirrors the
// crash-during-bulk-ingest scenario without needing a sub-process
// harness.
#[test]
fn fulltext_wal_replay_reconstructs_registry_and_content() {
    use crate::index::fulltext_registry::FullTextRegistry;
    use crate::wal::WalEntry;
    use tempfile::TempDir;

    let dir = TempDir::new().unwrap();
    let reg = FullTextRegistry::new();
    reg.set_base_dir(dir.path().to_path_buf());

    let entries = vec![
        WalEntry::FtsCreateIndex {
            name: "posts".to_string(),
            entity: 0,
            labels_or_types: vec!["Post".to_string()],
            properties: vec!["body".to_string()],
            analyzer: "standard".to_string(),
        },
        WalEntry::FtsAdd {
            name: "posts".to_string(),
            entity_id: 1,
            label_or_type_id: 0,
            key_id: 0,
            content: "first post body".to_string(),
        },
        WalEntry::FtsAdd {
            name: "posts".to_string(),
            entity_id: 2,
            label_or_type_id: 0,
            key_id: 0,
            content: "second post body".to_string(),
        },
        WalEntry::FtsDel {
            name: "posts".to_string(),
            entity_id: 1,
        },
        // Simulate a node-create interleaved in the log — replay
        // must skip it without aborting the FTS recovery loop.
        WalEntry::CreateNode {
            node_id: 99,
            label_bits: 0,
        },
    ];

    for e in &entries {
        reg.apply_wal_entry(e).expect("replay FTS WAL entry");
    }

    // Only doc 2 survives after the replayed delete.
    let hits = reg.query("posts", "body", None).unwrap();
    let ids: Vec<u64> = hits.iter().map(|h| h.node_id).collect();
    assert!(ids.contains(&2));
    assert!(
        !ids.contains(&1),
        "replayed FtsDel should have removed node 1"
    );
}

// phase6_fulltext-wal-integration §4 — CREATE against a label the
// FTS index does not cover must NOT populate the index.
#[test]
fn fulltext_create_node_skips_non_matching_label() {
    let (mut engine, _ctx) = crate::testing::setup_test_engine().unwrap();
    engine
        .execute_cypher("CALL db.index.fulltext.createNodeIndex('films', ['Film'], ['title'])")
        .unwrap();
    engine
        .execute_cypher("CREATE (:Documentary {title: 'Earth At Night'})")
        .unwrap();
    let r = engine
        .execute_cypher("CALL db.index.fulltext.queryNodes('films', 'earth')")
        .unwrap();
    assert!(
        r.rows.is_empty(),
        "Documentary must not leak into the Film-scoped index, got {:?}",
        r.rows
    );
}

// phase6_fulltext-wal-integration §4.3 — DELETE evicts the doc.
#[test]
fn fulltext_delete_node_evicts_from_index() {
    let (mut engine, _ctx) = crate::testing::setup_test_engine().unwrap();
    engine
        .execute_cypher("CALL db.index.fulltext.createNodeIndex('posts', ['Post'], ['body'])")
        .unwrap();
    engine
        .execute_cypher("CREATE (n:Post {id: 1, body: 'the quick brown fox'})")
        .unwrap();
    let pre = engine
        .execute_cypher("CALL db.index.fulltext.queryNodes('posts', 'fox')")
        .unwrap();
    assert!(!pre.rows.is_empty(), "auto-populate missing");

    engine
        .execute_cypher("MATCH (n:Post {id: 1}) DELETE n")
        .unwrap();
    let post = engine
        .execute_cypher("CALL db.index.fulltext.queryNodes('posts', 'fox')")
        .unwrap();
    assert!(
        post.rows.is_empty(),
        "DELETE must evict doc from FTS, got {:?}",
        post.rows
    );
}

// phase6_fulltext-wal-integration §4 — SET refreshes the doc.
#[test]
fn fulltext_set_property_refreshes_doc() {
    let (mut engine, _ctx) = crate::testing::setup_test_engine().unwrap();
    engine
        .execute_cypher("CALL db.index.fulltext.createNodeIndex('news', ['News'], ['headline'])")
        .unwrap();
    engine
        .execute_cypher("CREATE (n:News {id: 1, headline: 'First headline'})")
        .unwrap();

    engine
        .execute_cypher("MATCH (n:News {id: 1}) SET n.headline = 'Second breaking story'")
        .unwrap();

    let fresh = engine
        .execute_cypher("CALL db.index.fulltext.queryNodes('news', 'breaking')")
        .unwrap();
    assert!(
        !fresh.rows.is_empty(),
        "new term `breaking` missing after SET, got {:?}",
        fresh.rows
    );

    let stale = engine
        .execute_cypher("CALL db.index.fulltext.queryNodes('news', 'First')")
        .unwrap();
    assert!(
        stale.rows.is_empty(),
        "old term `First` must be purged after SET, got {:?}",
        stale.rows
    );
}

// phase6_fulltext-wal-integration §4.3 — REMOVE drops the doc when
// no indexed property is left.
#[test]
fn fulltext_remove_property_evicts_doc() {
    let (mut engine, _ctx) = crate::testing::setup_test_engine().unwrap();
    engine
        .execute_cypher("CALL db.index.fulltext.createNodeIndex('tags', ['Tag'], ['label'])")
        .unwrap();
    engine
        .execute_cypher("CREATE (n:Tag {id: 1, label: 'urgent ticket'})")
        .unwrap();
    engine
        .execute_cypher("MATCH (n:Tag {id: 1}) REMOVE n.label")
        .unwrap();
    let hits = engine
        .execute_cypher("CALL db.index.fulltext.queryNodes('tags', 'urgent')")
        .unwrap();
    assert!(
        hits.rows.is_empty(),
        "REMOVE of the only indexed property must drop the FTS doc, got {:?}",
        hits.rows
    );
}

// phase6_fulltext-analyzer-catalogue — unknown analyzer is rejected.
#[test]
fn fulltext_unknown_analyzer_is_rejected() {
    let (mut engine, _ctx) = crate::testing::setup_test_engine().unwrap();
    let err = engine
        .execute_cypher(
            "CALL db.index.fulltext.createNodeIndex('bad', ['L'], ['p'], \
             {analyzer: 'klingon'})",
        )
        .expect_err("unknown analyzer must surface ERR_FTS_UNKNOWN_ANALYZER");
    assert!(
        err.to_string().contains("ERR_FTS_UNKNOWN_ANALYZER"),
        "got: {err}"
    );
}

// phase6_clean-graph-rebuild-null-ids — null-key contract: MERGE with null property value is rejected.
#[test]
fn merge_rejects_null_property_value() {
    let (mut engine, _ctx) = crate::testing::setup_test_engine().unwrap();
    let err = engine
        .execute_cypher("MERGE (n:Label {id: null}) RETURN n")
        .expect_err("MERGE with null property value must be rejected");
    assert!(
        err.to_string()
            .contains("Cannot merge node using null property value for id"),
        "got: {err}"
    );
}

// phase6_clean-graph-rebuild-null-ids — null-key contract: MERGE with non-null property succeeds and is idempotent.
#[test]
fn merge_non_null_property_still_works() {
    let (mut engine, _ctx) = crate::testing::setup_test_engine().unwrap();

    // First MERGE creates the node.
    engine
        .execute_cypher("MERGE (n:Person {name: 'Alice'}) RETURN n")
        .expect("first MERGE must succeed");

    // Second identical MERGE must not create a duplicate.
    engine
        .execute_cypher("MERGE (n:Person {name: 'Alice'}) RETURN n")
        .expect("second MERGE must succeed");

    let result = engine
        .execute_cypher("MATCH (n:Person {name: 'Alice'}) RETURN count(n) AS c")
        .expect("count query must succeed");

    let count_val = result
        .rows
        .first()
        .and_then(|row| row.values.first())
        .cloned()
        .unwrap_or(serde_json::Value::Null);

    assert_eq!(
        count_val,
        serde_json::Value::Number(1.into()),
        "expected exactly 1 Alice node, got count={count_val}"
    );
}
