//! GH issue #5 Bug 1 — implicit GROUP BY by a function/expression key.
//!
//! Before the fix, `labels(n)[0]` parsed as just `labels(n)` (the postfix
//! `[0]` index after a function call was dropped), so the term was not treated
//! as a grouping key: the query returned one raw row per node with column
//! `labels(n)` and no aggregate. Per openCypher every non-aggregating
//! projection term — including expressions — is an implicit grouping key.

use nexus_core::Engine;
use nexus_core::testing::TestContext;
use std::collections::HashMap;

fn seed() -> (TestContext, Engine) {
    let ctx = TestContext::new();
    let mut engine = Engine::with_data_dir(ctx.path()).unwrap();
    engine
        .execute_cypher(
            "CREATE (:Person {kind:'a'}), (:Person {kind:'a'}), \
             (:Person {kind:'b'}), (:Company {kind:'a'})",
        )
        .unwrap();
    (ctx, engine)
}

/// Collect a two-column `(key, count)` result into a map for order-insensitive
/// comparison, asserting the output columns match the projection aliases.
fn grouped(engine: &mut Engine, query: &str, expected_cols: &[&str]) -> HashMap<String, i64> {
    let r = engine.execute_cypher(query).expect("query must execute");
    assert_eq!(
        r.columns, expected_cols,
        "output columns must be the projection aliases for `{query}`"
    );
    let mut map = HashMap::new();
    for row in &r.rows {
        let key = row.values[0]
            .as_str()
            .unwrap_or_else(|| panic!("group key must be a scalar string, got {:?}", row.values[0]))
            .to_string();
        let count = row.values[1]
            .as_i64()
            .unwrap_or_else(|| panic!("count must be an integer, got {:?}", row.values[1]));
        map.insert(key, count);
    }
    map
}

#[test]
fn group_by_function_index_expression_key() {
    let (_ctx, mut engine) = seed();

    let expected: HashMap<String, i64> =
        [("Person".to_string(), 3), ("Company".to_string(), 1)].into();

    // count(n) form
    assert_eq!(
        grouped(
            &mut engine,
            "MATCH (n) RETURN labels(n)[0] AS label, count(n) AS c",
            &["label", "c"],
        ),
        expected,
    );

    // count(*) form
    assert_eq!(
        grouped(
            &mut engine,
            "MATCH (n) RETURN labels(n)[0] AS label, count(*) AS c",
            &["label", "c"],
        ),
        expected,
    );

    // WITH form
    assert_eq!(
        grouped(
            &mut engine,
            "MATCH (n) WITH labels(n)[0] AS label, count(*) AS c RETURN label, c",
            &["label", "c"],
        ),
        expected,
    );
}

#[test]
fn group_by_property_key_still_works() {
    let (_ctx, mut engine) = seed();
    let expected: HashMap<String, i64> = [("a".to_string(), 3), ("b".to_string(), 1)].into();
    assert_eq!(
        grouped(
            &mut engine,
            "MATCH (n) RETURN n.kind AS k, count(*) AS c",
            &["k", "c"],
        ),
        expected,
    );
}

#[test]
fn function_postfix_index_returns_scalar_element() {
    // The parser fix: `labels(n)[0]` must index into the function result, not
    // drop the `[0]` and return the whole `labels(n)` array.
    let ctx = TestContext::new();
    let mut engine = Engine::with_data_dir(ctx.path()).unwrap();
    engine
        .execute_cypher("CREATE (:Person {name:'Alice'})")
        .unwrap();
    let r = engine
        .execute_cypher("MATCH (n:Person) RETURN labels(n)[0] AS first")
        .unwrap();
    assert_eq!(r.columns, vec!["first".to_string()]);
    assert_eq!(r.rows.len(), 1);
    assert_eq!(r.rows[0].values[0].as_str(), Some("Person"));
}
