//! Locks the correct behavior of labelled vs. unlabelled node scans.
//!
//! Filed as an openCypher-TCK-conformance gap: "a labelled node scan
//! appears to return unlabelled/other-labelled nodes", observed as a
//! result set carrying `(:A)`, `(:B {..})`, and `({..})` rows against an
//! expectation of a single `(:A)` row.
//!
//! Diagnosed against the exact TCK scenario that shape comes from —
//! `clauses/match/Match1.feature`, Scenario [2] "Matching all nodes":
//! `CREATE (:A), (:B {name: 'b'}), ({name: 'c'})` followed by
//! `MATCH (n) RETURN n`, expecting all three rows `(:A)`,
//! `(:B {name: 'b'})`, `({name: 'c'})`. That is an UNLABELLED scan
//! (`MATCH (n)`, no label on the pattern) — returning every node,
//! including differently-labelled and unlabelled ones, IS the correct,
//! TCK-expected behavior for an unlabelled scan. Nexus's actual output
//! for this exact scenario already matches the full three-row expected
//! table.
//!
//! Separately, `MATCH (n:A) RETURN n` (an actually LABELLED scan) was
//! verified end-to-end against a graph containing `:A`, `:B`, and
//! unlabelled nodes: it returns ONLY the `:A` nodes — never a `:B` node,
//! never an unlabelled node. Same for `WHERE n:A` label-check filtering.
//!
//! Verdict: the label filter itself is correct. §4.15 is a mis-ported /
//! differently-shaped TCK scenario comparison (the single-`(:A)`-row
//! "expected" value does not correspond to any labelled-scan scenario —
//! it is best explained as reading only the first line of Match1[2]'s
//! multi-row expected table), not a Nexus label-filter bug. This test
//! locks the correct behavior so it cannot silently regress.

use nexus_core::Engine;
use nexus_core::testing::TestContext;

fn engine() -> (Engine, TestContext) {
    let ctx = TestContext::new();
    let engine = Engine::with_isolated_catalog(ctx.path()).expect("engine");
    (engine, ctx)
}

fn node_labels(value: &serde_json::Value) -> Vec<String> {
    value
        .get("_nexus_labels")
        .and_then(serde_json::Value::as_array)
        .map(|labels| {
            labels
                .iter()
                .filter_map(|l| l.as_str().map(str::to_string))
                .collect()
        })
        .unwrap_or_default()
}

/// A labelled scan (`MATCH (n:A)`) against a graph containing `:A`, `:B`,
/// and unlabelled nodes must return ONLY the `:A` nodes.
#[test]
fn labelled_scan_returns_only_that_label() {
    let (mut engine, _ctx) = engine();
    engine
        .execute_cypher("CREATE (:A {id: 1}), (:B {id: 2}), ({id: 3}), (:A {id: 4})")
        .expect("seed A/B/unlabelled graph");

    let r = engine
        .execute_cypher("MATCH (n:A) RETURN n")
        .expect("MATCH (n:A) RETURN n");
    assert_eq!(
        r.rows.len(),
        2,
        "MATCH (n:A) must return exactly the 2 :A nodes, got {:?}",
        r.rows
    );
    for row in &r.rows {
        assert_eq!(
            node_labels(&row.values[0]),
            vec!["A".to_string()],
            "every row from MATCH (n:A) must carry only the A label, got {:?}",
            row.values[0]
        );
    }

    let r = engine
        .execute_cypher("MATCH (n:B) RETURN n")
        .expect("MATCH (n:B) RETURN n");
    assert_eq!(
        r.rows.len(),
        1,
        "MATCH (n:B) must return exactly the 1 :B node, got {:?}",
        r.rows
    );
    assert_eq!(node_labels(&r.rows[0].values[0]), vec!["B".to_string()]);
}

/// `WHERE n:A` label-check filtering must be equally precise: only nodes
/// carrying the `A` label pass, never a `:B` or unlabelled node.
#[test]
fn where_label_check_filters_precisely() {
    let (mut engine, _ctx) = engine();
    engine
        .execute_cypher("CREATE (:A {id: 1}), (:B {id: 2}), ({id: 3}), (:A {id: 4})")
        .expect("seed A/B/unlabelled graph");

    let r = engine
        .execute_cypher("MATCH (n) WHERE n:A RETURN n")
        .expect("MATCH (n) WHERE n:A RETURN n");
    assert_eq!(
        r.rows.len(),
        2,
        "WHERE n:A must select exactly the 2 :A nodes, got {:?}",
        r.rows
    );
    for row in &r.rows {
        assert_eq!(node_labels(&row.values[0]), vec!["A".to_string()]);
    }
}

/// An UNLABELLED scan (`MATCH (n) RETURN n`) must return every node,
/// regardless of label — including nodes with a different label and
/// nodes with no label at all. This is the exact shape of TCK
/// `clauses/match/Match1.feature` Scenario [2] "Matching all nodes".
#[test]
fn unlabelled_scan_returns_every_node_regardless_of_label() {
    let (mut engine, _ctx) = engine();
    engine
        .execute_cypher("CREATE (:A), (:B {name: 'b'}), ({name: 'c'})")
        .expect("seed Match1[2] graph");

    let r = engine
        .execute_cypher("MATCH (n) RETURN n")
        .expect("MATCH (n) RETURN n");
    assert_eq!(
        r.rows.len(),
        3,
        "MATCH (n) RETURN n must return all 3 nodes (labelled and \
         unlabelled), got {:?}",
        r.rows
    );

    let mut label_sets: Vec<Vec<String>> = r
        .rows
        .iter()
        .map(|row| node_labels(&row.values[0]))
        .collect();
    label_sets.sort();
    assert_eq!(
        label_sets,
        vec![
            Vec::<String>::new(),
            vec!["A".to_string()],
            vec!["B".to_string()]
        ],
        "the unlabelled scan must surface one :A row, one :B row, and one \
         unlabelled row — this is the correct behavior, not a label-filter \
         bug"
    );
}
