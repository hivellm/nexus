//! Unit tests for the shared TCK cell parser + graph-element comparison
//! (`tck_common`). This is a normal libtest (`harness = true`) target so the
//! `#[test]` functions actually run — the `tck_common` module itself is only
//! otherwise included by the `harness = false` corpus runners, which cannot
//! collect unit tests.

#[allow(dead_code)]
#[path = "tck_common/mod.rs"]
mod tck_common;

use serde_json::json;
use tck_common::{tck_cell_to_json, values_equal};

// ── Parser: IEEE special floats + temporal/duration strings ──

#[test]
fn parses_nan_as_float_marker() {
    assert_eq!(tck_cell_to_json("NaN"), json!({"@tck_float": "NaN"}));
}

#[test]
fn parses_positive_infinity_as_float_marker() {
    assert_eq!(
        tck_cell_to_json("Infinity"),
        json!({"@tck_float": "Infinity"})
    );
}

#[test]
fn parses_negative_infinity_as_float_marker() {
    assert_eq!(
        tck_cell_to_json("-Infinity"),
        json!({"@tck_float": "-Infinity"})
    );
}

#[test]
fn nan_marker_nested_in_a_list_parses() {
    assert_eq!(
        tck_cell_to_json("[1, NaN]"),
        json!([1, {"@tck_float": "NaN"}])
    );
}

#[test]
fn temporal_and_duration_values_parse_as_strings() {
    // The corpus renders temporal/duration results as quoted strings, so no
    // dedicated temporal literal is needed — they must parse as plain strings.
    assert_eq!(tck_cell_to_json("'2015-07-21'"), json!("2015-07-21"));
    assert_eq!(
        tck_cell_to_json("'1816-12-23T00:00'"),
        json!("1816-12-23T00:00")
    );
    assert_eq!(tck_cell_to_json("'P14DT16H12M'"), json!("P14DT16H12M"));
    assert_eq!(
        tck_cell_to_json("'PT-23H-59M-59.9S'"),
        json!("PT-23H-59M-59.9S")
    );
}

// ── Comparison: IEEE special floats ──

#[test]
fn nan_marker_does_not_match_a_finite_number() {
    // Nexus errors on every non-finite arithmetic result, so a NaN expectation
    // can never match a Nexus value — the scenario stays an attributable fail,
    // not a harness panic.
    let expected = tck_cell_to_json("NaN");
    assert!(!values_equal(&expected, &json!(1.5)));
    assert!(!values_equal(&expected, &json!(0.0)));
}

#[test]
fn nan_marker_does_not_match_null_or_string() {
    let expected = tck_cell_to_json("NaN");
    assert!(!values_equal(&expected, &json!(null)));
    assert!(!values_equal(&expected, &json!("NaN")));
}

// ── Parser: graph-element literals ──

#[test]
fn parses_labelled_node_with_properties() {
    assert_eq!(
        tck_cell_to_json("(:A {name: 'A'})"),
        json!({"@tck_node": true, "@labels": ["A"], "name": "A"})
    );
}

#[test]
fn parses_empty_node() {
    assert_eq!(
        tck_cell_to_json("()"),
        json!({"@tck_node": true, "@labels": []})
    );
}

#[test]
fn parses_multi_label_node() {
    assert_eq!(
        tck_cell_to_json("(:A:B {k: 1})"),
        json!({"@tck_node": true, "@labels": ["A", "B"], "k": 1})
    );
}

#[test]
fn parses_relationship_with_properties() {
    assert_eq!(
        tck_cell_to_json("[:KNOWS {num: 1}]"),
        json!({"@tck_rel": true, "@type": "KNOWS", "num": 1})
    );
}

#[test]
fn parses_path_with_two_nodes_one_rel() {
    let path = tck_cell_to_json("<(:A)-[:T]->(:B)>");
    assert_eq!(path["@tck_path"], json!(true));
    assert_eq!(path["nodes"].as_array().unwrap().len(), 2);
    assert_eq!(path["relationships"].as_array().unwrap().len(), 1);
    assert_eq!(path["relationships"][0]["@type"], json!("T"));
}

// ── Comparison: parsed literal (expected) vs Nexus value (actual) ──

#[test]
fn unlabelled_node_matches_nexus_node_on_properties() {
    let expected = tck_cell_to_json("({name: 'c'})");
    let actual = json!({"name": "c", "_nexus_id": 5, "_nexus_labels": []});
    assert!(values_equal(&expected, &actual));
}

#[test]
fn labelled_node_matches_when_labels_present() {
    // Nexus emits labels under `_labels`, so a labelled literal matches
    // when the label sets agree.
    let expected = tck_cell_to_json("(:A {name: 'c'})");
    let actual = json!({"name": "c", "_nexus_id": 5, "_nexus_labels": ["A"]});
    assert!(values_equal(&expected, &actual));
}

#[test]
fn labelled_node_does_not_match_on_label_mismatch() {
    let expected = tck_cell_to_json("(:A {name: 'c'})");
    let actual = json!({"name": "c", "_nexus_id": 5, "_nexus_labels": ["B"]});
    assert!(!values_equal(&expected, &actual));
}

#[test]
fn multi_label_node_matches_regardless_of_order() {
    let expected = tck_cell_to_json("(:A:B)");
    let actual = json!({"_nexus_id": 5, "_nexus_labels": ["B", "A"]});
    assert!(values_equal(&expected, &actual));
}

#[test]
fn node_with_extra_property_does_not_match() {
    let expected = tck_cell_to_json("({name: 'c', age: 3})");
    let actual = json!({"name": "c", "_nexus_id": 5, "_nexus_labels": []});
    assert!(!values_equal(&expected, &actual));
}

#[test]
fn relationship_matches_nexus_rel() {
    let expected = tck_cell_to_json("[:KNOWS {num: 1}]");
    let actual = json!({
        "num": 1, "_nexus_id": 9, "_nexus_rel_type": "KNOWS", "type": "KNOWS"
    });
    assert!(values_equal(&expected, &actual));
}

#[test]
fn relationship_with_differing_property_does_not_match() {
    let expected = tck_cell_to_json("[:KNOWS {num: 1}]");
    let actual = json!({
        "num": 2, "_nexus_id": 9, "_nexus_rel_type": "KNOWS", "type": "KNOWS"
    });
    assert!(!values_equal(&expected, &actual));
}

#[test]
fn relationship_type_mismatch_does_not_match() {
    let expected = tck_cell_to_json("[:KNOWS]");
    let actual = json!({
        "_nexus_id": 9, "_nexus_rel_type": "FOLLOWS", "type": "FOLLOWS"
    });
    assert!(!values_equal(&expected, &actual));
}
