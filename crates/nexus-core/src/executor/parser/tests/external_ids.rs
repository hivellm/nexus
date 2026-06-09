//! Tests for the external node-id (`_id`) parser: extraction from CREATE and
//! MERGE patterns, ON CONFLICT policy parsing, and rejection of invalid forms.
//! (phase9_external-node-ids §4.2 / §4.3 / §4.5)

use super::*;

#[test]
fn parser_extracts_underscore_id_string_literal() {
    let mut p = CypherParser::new("CREATE (n:File {_id: 'sha256:abc', name: 'a.txt'})".to_string());
    let q = p.parse().unwrap();
    let c = first_create_clause(&q);
    match &c.external_id_expr {
        Some(Expression::Literal(Literal::String(s))) => assert_eq!(s, "sha256:abc"),
        other => panic!("expected string literal external_id_expr, got {:?}", other),
    }
    if let PatternElement::Node(np) = &c.pattern.elements[0] {
        let props = np.properties.as_ref().expect("node has properties");
        assert!(!props.properties.contains_key("_id"));
        assert!(props.properties.contains_key("name"));
    } else {
        panic!("expected node pattern");
    }
}

#[test]
fn parser_extracts_underscore_id_parameter() {
    let mut p = CypherParser::new("CREATE (n:File {_id: $ext_id})".to_string());
    let q = p.parse().unwrap();
    let c = first_create_clause(&q);
    match &c.external_id_expr {
        Some(Expression::Parameter(name)) => assert_eq!(name, "ext_id"),
        other => panic!("expected parameter external_id_expr, got {:?}", other),
    }
}

#[test]
fn parser_rejects_non_string_underscore_id() {
    let mut p = CypherParser::new("CREATE (n:File {_id: 42})".to_string());
    assert!(p.parse().is_err(), "_id with integer must error");
}

#[test]
fn parser_default_conflict_policy_is_error() {
    let mut p = CypherParser::new("CREATE (n:File {_id: 'sha256:abc'})".to_string());
    let q = p.parse().unwrap();
    let c = first_create_clause(&q);
    assert_eq!(c.conflict_policy, AstConflictPolicy::Error);
}

#[test]
fn parser_on_conflict_match() {
    let mut p =
        CypherParser::new("CREATE (n:File {_id: 'sha256:abc'}) ON CONFLICT MATCH".to_string());
    let q = p.parse().unwrap();
    let c = first_create_clause(&q);
    assert_eq!(c.conflict_policy, AstConflictPolicy::Match);
}

#[test]
fn parser_on_conflict_replace() {
    let mut p =
        CypherParser::new("CREATE (n:File {_id: 'sha256:abc'}) ON CONFLICT REPLACE".to_string());
    let q = p.parse().unwrap();
    let c = first_create_clause(&q);
    assert_eq!(c.conflict_policy, AstConflictPolicy::Replace);
}

#[test]
fn parser_on_conflict_error_explicit() {
    let mut p =
        CypherParser::new("CREATE (n:File {_id: 'sha256:abc'}) ON CONFLICT ERROR".to_string());
    let q = p.parse().unwrap();
    let c = first_create_clause(&q);
    assert_eq!(c.conflict_policy, AstConflictPolicy::Error);
}

#[test]
fn parser_on_conflict_unknown_policy_errors() {
    let mut p =
        CypherParser::new("CREATE (n:File {_id: 'sha256:abc'}) ON CONFLICT IGNORE".to_string());
    assert!(p.parse().is_err());
}

#[test]
fn parser_create_without_id_has_no_external_id_expr() {
    let mut p = CypherParser::new("CREATE (n:File {name: 'a.txt'})".to_string());
    let q = p.parse().unwrap();
    let c = first_create_clause(&q);
    assert!(c.external_id_expr.is_none());
    assert_eq!(c.conflict_policy, AstConflictPolicy::Error);
}

// phase9_external-node-ids §4.5 — MERGE clause MUST also extract `_id`.

#[test]
fn parser_merge_extracts_underscore_id_string_literal() {
    let mut p = CypherParser::new("MERGE (n:File {_id: 'sha256:abc'})".to_string());
    let q = p.parse().unwrap();
    let merge = match &q.clauses[0] {
        Clause::Merge(m) => m,
        other => panic!("expected MERGE clause, got {:?}", other),
    };
    match &merge.external_id_expr {
        Some(Expression::Literal(Literal::String(s))) => assert_eq!(s, "sha256:abc"),
        other => panic!(
            "expected string literal external_id_expr on MERGE, got {:?}",
            other
        ),
    }
    if let PatternElement::Node(np) = &merge.pattern.elements[0] {
        let props = np.properties.as_ref().expect("node has properties");
        assert!(!props.properties.contains_key("_id"));
    } else {
        panic!("expected node pattern in MERGE");
    }
}

#[test]
fn parser_merge_without_underscore_id() {
    let mut p = CypherParser::new("MERGE (n:File {name: 'a.txt'})".to_string());
    let q = p.parse().unwrap();
    let merge = match &q.clauses[0] {
        Clause::Merge(m) => m,
        other => panic!("expected MERGE, got {:?}", other),
    };
    assert!(merge.external_id_expr.is_none());
}
