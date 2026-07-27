//! Tests for phase4_cypher-parity-quick-wins: closes the P1/P2 function tail
//! from `docs/nexus/01-compatibility-gaps.md` — `randomUUID()`, `ascii()`,
//! `chr()`, `lpad()`/`rpad()`, `normalize()`, two-arg `log()`, `isNaN()`,
//! `shuffle()`, the `elementId()` opaque-string format, percentile/stDev
//! aggregate verification, and multi-pattern `CREATE`.

use nexus_core::testing::setup_isolated_test_engine;
use nexus_core::{Engine, executor::ResultSet};

fn execute_query(engine: &mut Engine, query: &str) -> ResultSet {
    engine.execute_cypher(query).expect("Query should succeed")
}

fn get_single_value(result: &ResultSet) -> &serde_json::Value {
    assert!(!result.rows.is_empty(), "Result has no rows!");
    assert!(
        !result.rows[0].values.is_empty(),
        "First row has no values!"
    );
    &result.rows[0].values[0]
}

// ============================================================================
// randomUUID()
// ============================================================================

#[test]
fn test_random_uuid_is_valid_v4() {
    let (mut engine, _ctx) = setup_isolated_test_engine().unwrap();
    let result = execute_query(&mut engine, "RETURN randomUUID() AS u");
    let v = get_single_value(&result);
    let s = v.as_str().expect("randomUUID() must return a string");
    let parsed = uuid::Uuid::parse_str(s).expect("randomUUID() must return a parsable UUID");
    assert_eq!(
        parsed.get_version_num(),
        4,
        "randomUUID() must return a v4 UUID"
    );
}

#[test]
fn test_random_uuid_is_unique_per_call() {
    let (mut engine, _ctx) = setup_isolated_test_engine().unwrap();
    let a = get_single_value(&execute_query(&mut engine, "RETURN randomUUID() AS u"))
        .as_str()
        .unwrap()
        .to_string();
    let b = get_single_value(&execute_query(&mut engine, "RETURN randomUUID() AS u"))
        .as_str()
        .unwrap()
        .to_string();
    assert_ne!(a, b, "two randomUUID() calls must not collide");
}

// ============================================================================
// ascii() / chr()
// ============================================================================

#[test]
fn test_ascii_function() {
    let (mut engine, _ctx) = setup_isolated_test_engine().unwrap();
    let result = execute_query(&mut engine, "RETURN ascii('A') AS a");
    assert_eq!(get_single_value(&result).as_i64().unwrap(), 65);

    // Only the first character counts.
    let result = execute_query(&mut engine, "RETURN ascii('hello') AS a");
    assert_eq!(get_single_value(&result).as_i64().unwrap(), 104);
}

#[test]
fn test_ascii_function_null_and_empty() {
    let (mut engine, _ctx) = setup_isolated_test_engine().unwrap();
    let result = execute_query(&mut engine, "RETURN ascii(null) AS a");
    assert!(get_single_value(&result).is_null());

    let result = execute_query(&mut engine, "RETURN ascii('') AS a");
    assert!(get_single_value(&result).is_null());
}

#[test]
fn test_ascii_function_type_error_returns_null() {
    let (mut engine, _ctx) = setup_isolated_test_engine().unwrap();
    let result = execute_query(&mut engine, "RETURN ascii(42) AS a");
    assert!(get_single_value(&result).is_null());
}

#[test]
fn test_chr_function() {
    let (mut engine, _ctx) = setup_isolated_test_engine().unwrap();
    let result = execute_query(&mut engine, "RETURN chr(65) AS c");
    assert_eq!(get_single_value(&result).as_str().unwrap(), "A");
}

#[test]
fn test_chr_function_null_and_invalid() {
    let (mut engine, _ctx) = setup_isolated_test_engine().unwrap();
    let result = execute_query(&mut engine, "RETURN chr(null) AS c");
    assert!(get_single_value(&result).is_null());

    // 0xD800 is a UTF-16 surrogate half — not a valid Unicode scalar value.
    let result = execute_query(&mut engine, "RETURN chr(55296) AS c");
    assert!(get_single_value(&result).is_null());
}

// ============================================================================
// lpad() / rpad()
// ============================================================================

#[test]
fn test_lpad_default_space_padding() {
    let (mut engine, _ctx) = setup_isolated_test_engine().unwrap();
    let result = execute_query(&mut engine, "RETURN lpad('abc', 6) AS s");
    assert_eq!(get_single_value(&result).as_str().unwrap(), "   abc");
}

#[test]
fn test_lpad_custom_padding() {
    let (mut engine, _ctx) = setup_isolated_test_engine().unwrap();
    let result = execute_query(&mut engine, "RETURN lpad('abc', 6, 'x') AS s");
    assert_eq!(get_single_value(&result).as_str().unwrap(), "xxxabc");
}

#[test]
fn test_lpad_truncates_when_length_shorter() {
    let (mut engine, _ctx) = setup_isolated_test_engine().unwrap();
    let result = execute_query(&mut engine, "RETURN lpad('abcdef', 3, 'x') AS s");
    assert_eq!(get_single_value(&result).as_str().unwrap(), "abc");
}

#[test]
fn test_lpad_null_propagation() {
    let (mut engine, _ctx) = setup_isolated_test_engine().unwrap();
    let result = execute_query(&mut engine, "RETURN lpad(null, 6, 'x') AS s");
    assert!(get_single_value(&result).is_null());
}

#[test]
fn test_rpad_default_space_padding() {
    let (mut engine, _ctx) = setup_isolated_test_engine().unwrap();
    let result = execute_query(&mut engine, "RETURN rpad('abc', 6) AS s");
    assert_eq!(get_single_value(&result).as_str().unwrap(), "abc   ");
}

#[test]
fn test_rpad_custom_padding() {
    let (mut engine, _ctx) = setup_isolated_test_engine().unwrap();
    let result = execute_query(&mut engine, "RETURN rpad('abc', 6, 'x') AS s");
    assert_eq!(get_single_value(&result).as_str().unwrap(), "abcxxx");
}

#[test]
fn test_rpad_truncates_when_length_shorter() {
    let (mut engine, _ctx) = setup_isolated_test_engine().unwrap();
    let result = execute_query(&mut engine, "RETURN rpad('abcdef', 3, 'x') AS s");
    assert_eq!(get_single_value(&result).as_str().unwrap(), "abc");
}

// ============================================================================
// normalize()
// ============================================================================

#[test]
fn test_normalize_default_nfc() {
    let (mut engine, _ctx) = setup_isolated_test_engine().unwrap();
    // "e" + combining acute accent (U+0065 U+0301) normalizes to U+00E9 (é) under NFC.
    let result = execute_query(&mut engine, "RETURN normalize('e\u{0301}') AS s");
    let s = get_single_value(&result).as_str().unwrap().to_string();
    assert_eq!(s, "\u{00e9}");
}

#[test]
fn test_normalize_nfd_decomposes() {
    let (mut engine, _ctx) = setup_isolated_test_engine().unwrap();
    let result = execute_query(&mut engine, "RETURN normalize('\u{00e9}', 'NFD') AS s");
    let s = get_single_value(&result).as_str().unwrap().to_string();
    assert_eq!(s, "e\u{0301}");
}

#[test]
fn test_normalize_nfkc_and_nfkd() {
    let (mut engine, _ctx) = setup_isolated_test_engine().unwrap();
    // U+FB01 (ﬁ ligature) -> "fi" under compatibility decomposition (NFKC/NFKD).
    let result = execute_query(&mut engine, "RETURN normalize('\u{fb01}', 'NFKC') AS s");
    assert_eq!(get_single_value(&result).as_str().unwrap(), "fi");

    let result = execute_query(&mut engine, "RETURN normalize('\u{fb01}', 'NFKD') AS s");
    assert_eq!(get_single_value(&result).as_str().unwrap(), "fi");
}

#[test]
fn test_normalize_null_propagation() {
    let (mut engine, _ctx) = setup_isolated_test_engine().unwrap();
    let result = execute_query(&mut engine, "RETURN normalize(null) AS s");
    assert!(get_single_value(&result).is_null());
}

#[test]
fn test_normalize_invalid_form_errors() {
    let (mut engine, _ctx) = setup_isolated_test_engine().unwrap();
    let err = engine.execute_cypher("RETURN normalize('abc', 'BOGUS') AS s");
    assert!(err.is_err(), "an invalid normal form must be a query error");
}

// ============================================================================
// log(x, base) two-arg form + isNaN()
// ============================================================================

#[test]
fn test_log_one_arg_is_natural_log() {
    let (mut engine, _ctx) = setup_isolated_test_engine().unwrap();
    let result = execute_query(&mut engine, "RETURN log(2.718281828459045) AS l");
    assert!((get_single_value(&result).as_f64().unwrap() - 1.0).abs() < 0.0001);
}

#[test]
fn test_log_two_arg_base() {
    let (mut engine, _ctx) = setup_isolated_test_engine().unwrap();
    // log base 2 of 8 = 3
    let result = execute_query(&mut engine, "RETURN log(8, 2) AS l");
    assert!((get_single_value(&result).as_f64().unwrap() - 3.0).abs() < 0.0001);

    // log base 10 of 1000 = 3
    let result = execute_query(&mut engine, "RETURN log(1000, 10) AS l");
    assert!((get_single_value(&result).as_f64().unwrap() - 3.0).abs() < 0.0001);
}

#[test]
fn test_log_two_arg_null_propagation() {
    let (mut engine, _ctx) = setup_isolated_test_engine().unwrap();
    let result = execute_query(&mut engine, "RETURN log(null, 2) AS l");
    assert!(get_single_value(&result).is_null());
    let result = execute_query(&mut engine, "RETURN log(8, null) AS l");
    assert!(get_single_value(&result).is_null());
}

#[test]
fn test_is_nan_function() {
    let (mut engine, _ctx) = setup_isolated_test_engine().unwrap();
    // serde_json's Number cannot represent a live NaN (from_f64 rejects
    // non-finite values), so arithmetic like `0.0 / 0.0` errors out as
    // "division by zero" before a NaN value could ever reach isNaN().
    // The one reachable path to a real f64::NAN is value_to_number's
    // STRING fallback ("NaN".parse::<f64>() succeeds in Rust), which
    // never round-trips through JSON — this is the same coercion path
    // isNaN() is documented to use for non-FLOAT/INTEGER input.
    let result = execute_query(&mut engine, "RETURN isNaN('NaN') AS n");
    assert!(get_single_value(&result).as_bool().unwrap());

    let result = execute_query(&mut engine, "RETURN isNaN(1.5) AS n");
    assert!(!get_single_value(&result).as_bool().unwrap());
}

#[test]
fn test_is_nan_null_propagation() {
    let (mut engine, _ctx) = setup_isolated_test_engine().unwrap();
    let result = execute_query(&mut engine, "RETURN isNaN(null) AS n");
    assert!(get_single_value(&result).is_null());
}

// ============================================================================
// shuffle()
// ============================================================================

#[test]
fn test_shuffle_preserves_multiset() {
    let (mut engine, _ctx) = setup_isolated_test_engine().unwrap();
    let result = execute_query(&mut engine, "RETURN shuffle([1, 2, 3, 4, 5]) AS s");
    let arr = get_single_value(&result)
        .as_array()
        .expect("shuffle() must return a list")
        .clone();
    let mut sorted: Vec<i64> = arr.iter().map(|v| v.as_i64().unwrap()).collect();
    sorted.sort();
    assert_eq!(sorted, vec![1, 2, 3, 4, 5]);
}

#[test]
fn test_shuffle_null_propagation() {
    let (mut engine, _ctx) = setup_isolated_test_engine().unwrap();
    let result = execute_query(&mut engine, "RETURN shuffle(null) AS s");
    assert!(get_single_value(&result).is_null());
}

#[test]
fn test_shuffle_empty_list() {
    let (mut engine, _ctx) = setup_isolated_test_engine().unwrap();
    let result = execute_query(&mut engine, "RETURN shuffle([]) AS s");
    assert_eq!(get_single_value(&result).as_array().unwrap().len(), 0);
}

// ============================================================================
// elementId() opaque stable string; id() unchanged
// ============================================================================

#[test]
fn test_element_id_node_format() {
    let (mut engine, _ctx) = setup_isolated_test_engine().unwrap();
    engine
        .execute_cypher("CREATE (n:EidNode {name:'x'})")
        .unwrap();
    let result = execute_query(
        &mut engine,
        "MATCH (n:EidNode) RETURN elementId(n) AS eid, id(n) AS nid",
    );
    let eid = result.rows[0].values[0].as_str().unwrap().to_string();
    let nid = result.rows[0].values[1].as_i64().unwrap();
    assert_eq!(eid, format!("n:{nid}"));
}

#[test]
fn test_element_id_relationship_format() {
    let (mut engine, _ctx) = setup_isolated_test_engine().unwrap();
    engine
        .execute_cypher("CREATE (a:EidA)-[r:EIDREL]->(b:EidB)")
        .unwrap();
    let result = execute_query(
        &mut engine,
        "MATCH (:EidA)-[r:EIDREL]->(:EidB) RETURN elementId(r) AS eid, id(r) AS rid",
    );
    let eid = result.rows[0].values[0].as_str().unwrap().to_string();
    let rid = result.rows[0].values[1].as_i64().unwrap();
    assert_eq!(eid, format!("r:{rid}"));
}

#[test]
fn test_element_id_null_propagation() {
    let (mut engine, _ctx) = setup_isolated_test_engine().unwrap();
    let result = execute_query(&mut engine, "RETURN elementId(null) AS eid");
    assert!(get_single_value(&result).is_null());
}

// ============================================================================
// percentileDisc / percentileCont / stDev / stDevP — verified against
// hand-computed reference values (phase4 §2.2).
// ============================================================================

#[test]
fn test_stdev_and_stdevp_hand_computed() {
    let (mut engine, _ctx) = setup_isolated_test_engine().unwrap();
    engine
        .execute_cypher("CREATE (:StatN {v: 1}), (:StatN {v: 2}), (:StatN {v: 3}), (:StatN {v: 4})")
        .unwrap();

    let result = execute_query(&mut engine, "MATCH (n:StatN) RETURN stdev(n.v) AS x");
    assert!((get_single_value(&result).as_f64().unwrap() - 1.290_994_4).abs() < 0.000_01);

    let result = execute_query(&mut engine, "MATCH (n:StatN) RETURN stdevp(n.v) AS x");
    assert!((get_single_value(&result).as_f64().unwrap() - 1.118_033_9).abs() < 0.000_01);
}

#[test]
fn test_percentile_disc_and_cont_hand_computed() {
    let (mut engine, _ctx) = setup_isolated_test_engine().unwrap();
    engine
        .execute_cypher(
            "CREATE (:StatM {v: 1}), (:StatM {v: 2}), (:StatM {v: 3}), (:StatM {v: 4}), (:StatM {v: 5})",
        )
        .unwrap();

    let result = execute_query(
        &mut engine,
        "MATCH (n:StatM) RETURN percentileDisc(n.v, 0.5) AS x",
    );
    assert!((get_single_value(&result).as_f64().unwrap() - 3.0).abs() < 0.000_01);

    let result = execute_query(
        &mut engine,
        "MATCH (n:StatM) WHERE n.v <= 4 RETURN percentileCont(n.v, 0.5) AS x",
    );
    assert!((get_single_value(&result).as_f64().unwrap() - 2.5).abs() < 0.000_01);
}

// ============================================================================
// phase0_fix-cypher-eval-panics — percentileCont must validate its
// percentile argument instead of indexing out of bounds or silently
// saturating.
// ============================================================================

#[test]
fn test_percentile_cont_out_of_range_high_errors() {
    let (mut engine, _ctx) = setup_isolated_test_engine().unwrap();
    engine
        .execute_cypher("CREATE (:StatP {v: 1.0}), (:StatP {v: 2.0}), (:StatP {v: 3.0})")
        .unwrap();
    let result = engine.execute_cypher("MATCH (n:StatP) RETURN percentileCont(n.v, 1.5) AS y");
    assert!(
        result.is_err(),
        "percentileCont with percentile > 1.0 must error, not panic OOB; got: {:?}",
        result
    );
}

// Note: a negative percentile literal (e.g. `-0.5`) parses as a `UnaryOp`,
// not a `Literal`, so the planner's `percentileCont(...)` extraction (a
// separate, out-of-scope code path in `executor/planner/queries/{planner_core,strategy}.rs`)
// silently drops the whole aggregation before it ever reaches the executor —
// there is currently no Cypher query text that reaches
// `Aggregation::PercentileCont` with a negative `percentile` field. The
// executor-level [0,1] guard added here is covered directly instead, in
// `crates/nexus-core/src/executor/operators/aggregate/tests.rs::percentile_cont_rejects_negative_percentile`.

#[test]
fn test_percentile_cont_boundary_values_succeed() {
    let (mut engine, _ctx) = setup_isolated_test_engine().unwrap();
    engine
        .execute_cypher("CREATE (:StatP {v: 1.0}), (:StatP {v: 2.0}), (:StatP {v: 3.0})")
        .unwrap();

    let result = engine
        .execute_cypher("MATCH (n:StatP) RETURN percentileCont(n.v, 0.0) AS y")
        .expect("percentileCont(n.v, 0.0) is a valid boundary and must succeed");
    assert!((get_single_value(&result).as_f64().unwrap() - 1.0).abs() < 0.000_01);

    let result = engine
        .execute_cypher("MATCH (n:StatP) RETURN percentileCont(n.v, 1.0) AS y")
        .expect("percentileCont(n.v, 1.0) is a valid boundary and must succeed");
    assert!((get_single_value(&result).as_f64().unwrap() - 3.0).abs() < 0.000_01);
}

#[test]
fn test_percentile_cont_just_past_boundary_errors() {
    let (mut engine, _ctx) = setup_isolated_test_engine().unwrap();
    engine
        .execute_cypher("CREATE (:StatP {v: 1.0}), (:StatP {v: 2.0}), (:StatP {v: 3.0})")
        .unwrap();
    let result = engine.execute_cypher("MATCH (n:StatP) RETURN percentileCont(n.v, 1.0001) AS y");
    assert!(
        result.is_err(),
        "percentileCont just past the [0,1] boundary must error; got: {:?}",
        result
    );
}

// ============================================================================
// Multiple comma-separated patterns in one CREATE
// ============================================================================

#[test]
fn test_create_multiple_disconnected_patterns() {
    let (mut engine, _ctx) = setup_isolated_test_engine().unwrap();
    engine
        .execute_cypher("CREATE (a:MpA {name:'A'}), (b:MpB {name:'B'})")
        .unwrap();

    let result = execute_query(&mut engine, "MATCH (n:MpA) RETURN count(n) AS c");
    assert_eq!(get_single_value(&result).as_i64().unwrap(), 1);

    let result = execute_query(&mut engine, "MATCH (n:MpB) RETURN count(n) AS c");
    assert_eq!(get_single_value(&result).as_i64().unwrap(), 1);
}

#[test]
fn test_create_multiple_patterns_with_relationship_between_them() {
    let (mut engine, _ctx) = setup_isolated_test_engine().unwrap();
    engine
        .execute_cypher("CREATE (a:MpC {name:'A'}), (b:MpD {name:'B'}), (a)-[r:MPREL]->(b)")
        .unwrap();

    let result = execute_query(&mut engine, "MATCH (n:MpC) RETURN count(n) AS c");
    assert_eq!(get_single_value(&result).as_i64().unwrap(), 1);
    let result = execute_query(&mut engine, "MATCH (n:MpD) RETURN count(n) AS c");
    assert_eq!(get_single_value(&result).as_i64().unwrap(), 1);
    let result = execute_query(
        &mut engine,
        "MATCH (:MpC)-[r:MPREL]->(:MpD) RETURN count(r) AS c",
    );
    assert_eq!(get_single_value(&result).as_i64().unwrap(), 1);
}

#[test]
fn test_create_via_execute_cypher_with_params_multi_pattern() {
    use std::collections::HashMap;
    let (mut engine, _ctx) = setup_isolated_test_engine().unwrap();
    let params: HashMap<String, serde_json::Value> = HashMap::new();
    engine
        .execute_cypher_with_params("CREATE (a:MpE {name:'A'}), (b:MpF {name:'B'})", params)
        .unwrap();

    let result = execute_query(&mut engine, "MATCH (n:MpE) RETURN count(n) AS c");
    assert_eq!(get_single_value(&result).as_i64().unwrap(), 1);
    let result = execute_query(&mut engine, "MATCH (n:MpF) RETURN count(n) AS c");
    assert_eq!(get_single_value(&result).as_i64().unwrap(), 1);
}
