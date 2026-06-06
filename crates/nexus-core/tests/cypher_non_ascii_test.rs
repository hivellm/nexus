//! GH issue #6 — non-ASCII text in Cypher must be handled, not panic.
//!
//! The lexer advanced `pos` by 1 byte per consumed char; a multi-byte UTF-8
//! char (any non-ASCII text in a string literal / property value) left `pos`
//! mid-sequence and the next slice panicked on a non-char boundary, which in
//! the HTTP server surfaced as a dropped connection. The fix advances by
//! `len_utf8()`. These tests assert lossless round-trips.

use nexus_core::Engine;
use nexus_core::testing::TestContext;

fn engine() -> (TestContext, Engine) {
    let ctx = TestContext::new();
    let e = Engine::with_data_dir(ctx.path()).unwrap();
    (ctx, e)
}

#[test]
fn non_ascii_string_literals_round_trip() {
    let (_ctx, mut engine) = engine();
    for v in ["versão", "日本語", "Привет", "emoji 😀", "café — déjà vu"] {
        let create = format!("CREATE (:T {{v:'{v}'}})");
        engine
            .execute_cypher(&create)
            .unwrap_or_else(|e| panic!("CREATE with {v:?} must succeed, got {e:?}"));
    }
    let r = engine
        .execute_cypher("MATCH (n:T) RETURN n.v AS v")
        .expect("MATCH must succeed");
    let mut got: Vec<String> = r
        .rows
        .iter()
        .filter_map(|row| row.values[0].as_str().map(|s| s.to_string()))
        .collect();
    got.sort();
    let mut want = vec!["versão", "日本語", "Привет", "emoji 😀", "café — déjà vu"]
        .into_iter()
        .map(String::from)
        .collect::<Vec<_>>();
    want.sort();
    assert_eq!(got, want, "non-ASCII values must round-trip losslessly");
}

#[test]
fn non_ascii_in_where_predicate() {
    let (_ctx, mut engine) = engine();
    engine
        .execute_cypher("CREATE (:P {name:'versão'}), (:P {name:'verso'})")
        .unwrap();
    let r = engine
        .execute_cypher("MATCH (n:P) WHERE n.name = 'versão' RETURN n.name AS name")
        .expect("WHERE with non-ASCII literal must succeed");
    assert_eq!(r.rows.len(), 1, "exactly the accented row matches");
    assert_eq!(r.rows[0].values[0].as_str(), Some("versão"));
}

#[test]
fn non_ascii_via_parameter() {
    let (_ctx, mut engine) = engine();
    engine.execute_cypher("CREATE (:P {name:'naïve'})").unwrap();
    let mut params = std::collections::HashMap::new();
    params.insert("n".to_string(), serde_json::json!("naïve"));
    let r = engine
        .execute_cypher_with_params(
            "MATCH (p:P) WHERE p.name = $n RETURN p.name AS name",
            params,
        )
        .expect("parametrized non-ASCII match must succeed");
    assert_eq!(r.rows.len(), 1);
    assert_eq!(r.rows[0].values[0].as_str(), Some("naïve"));
}
