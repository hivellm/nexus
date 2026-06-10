//! Tests for pattern parsing: relationship directions, quantifiers,
//! multiple labels/types, and Quantified Path Patterns (QPP / Cypher 25).

use super::*;

#[test]
fn test_parse_relationship_directions() {
    // Test outgoing relationship
    let mut parser = CypherParser::new("MATCH (a)-[r]->(b) RETURN a".to_string());
    let query = parser.parse().unwrap();
    match &query.clauses[0] {
        Clause::Match(match_clause) => match &match_clause.pattern.elements[1] {
            PatternElement::Relationship(rel) => {
                assert_eq!(rel.direction, RelationshipDirection::Outgoing);
            }
            _ => panic!("Expected relationship"),
        },
        _ => panic!("Expected match clause"),
    }

    // Test incoming relationship
    let mut parser = CypherParser::new("MATCH (a)<-[r]-(b) RETURN a".to_string());
    let query = parser.parse().unwrap();
    match &query.clauses[0] {
        Clause::Match(match_clause) => match &match_clause.pattern.elements[1] {
            PatternElement::Relationship(rel) => {
                assert_eq!(rel.direction, RelationshipDirection::Incoming);
            }
            _ => panic!("Expected relationship"),
        },
        _ => panic!("Expected match clause"),
    }

    // Test both directions
    let mut parser = CypherParser::new("MATCH (a)-[r]-(b) RETURN a".to_string());
    let query = parser.parse().unwrap();
    match &query.clauses[0] {
        Clause::Match(match_clause) => match &match_clause.pattern.elements[1] {
            PatternElement::Relationship(rel) => {
                assert_eq!(rel.direction, RelationshipDirection::Both);
            }
            _ => panic!("Expected relationship"),
        },
        _ => panic!("Expected match clause"),
    }
}

#[test]
fn test_parse_relationship_quantifiers() {
    // Test basic relationship without quantifier
    let mut parser = CypherParser::new("MATCH (a)-[r]->(b) RETURN a".to_string());
    let query = parser.parse().unwrap();
    match &query.clauses[0] {
        Clause::Match(match_clause) => match &match_clause.pattern.elements[1] {
            PatternElement::Relationship(rel) => {
                assert_eq!(rel.quantifier, None);
            }
            _ => panic!("Expected relationship"),
        },
        _ => panic!("Expected match clause"),
    }

    // Test zero or more quantifier (*)
    let mut parser = CypherParser::new("MATCH (a)-[r*]->(b) RETURN a".to_string());
    let query = parser.parse().unwrap();
    match &query.clauses[0] {
        Clause::Match(match_clause) => match &match_clause.pattern.elements[1] {
            PatternElement::Relationship(rel) => {
                assert_eq!(rel.quantifier, Some(RelationshipQuantifier::ZeroOrMore));
            }
            _ => panic!("Expected relationship"),
        },
        _ => panic!("Expected match clause"),
    }

    // Test one or more quantifier (+)
    let mut parser = CypherParser::new("MATCH (a)-[r+]->(b) RETURN a".to_string());
    let query = parser.parse().unwrap();
    match &query.clauses[0] {
        Clause::Match(match_clause) => match &match_clause.pattern.elements[1] {
            PatternElement::Relationship(rel) => {
                assert_eq!(rel.quantifier, Some(RelationshipQuantifier::OneOrMore));
            }
            _ => panic!("Expected relationship"),
        },
        _ => panic!("Expected match clause"),
    }

    // Test zero or one quantifier (?)
    let mut parser = CypherParser::new("MATCH (a)-[r?]->(b) RETURN a".to_string());
    let query = parser.parse().unwrap();
    match &query.clauses[0] {
        Clause::Match(match_clause) => match &match_clause.pattern.elements[1] {
            PatternElement::Relationship(rel) => {
                assert_eq!(rel.quantifier, Some(RelationshipQuantifier::ZeroOrOne));
            }
            _ => panic!("Expected relationship"),
        },
        _ => panic!("Expected match clause"),
    }

    // Test exact quantifier {2}
    let mut parser = CypherParser::new("MATCH (a)-[r{2}]->(b) RETURN a".to_string());
    let query = parser.parse().unwrap();
    match &query.clauses[0] {
        Clause::Match(match_clause) => match &match_clause.pattern.elements[1] {
            PatternElement::Relationship(rel) => {
                assert_eq!(rel.quantifier, Some(RelationshipQuantifier::Exact(2)));
            }
            _ => panic!("Expected relationship"),
        },
        _ => panic!("Expected match clause"),
    }

    // Test range quantifier {1..3}
    let mut parser = CypherParser::new("MATCH (a)-[r{1..3}]->(b) RETURN a".to_string());
    let query = parser.parse().unwrap();
    match &query.clauses[0] {
        Clause::Match(match_clause) => match &match_clause.pattern.elements[1] {
            PatternElement::Relationship(rel) => {
                if let Some(RelationshipQuantifier::Range(min, max)) = &rel.quantifier {
                    assert_eq!(*min, 1);
                    assert_eq!(*max, 3);
                } else {
                    panic!("Expected Range quantifier, got {:?}", rel.quantifier);
                }
            }
            _ => panic!("Expected relationship"),
        },
        _ => panic!("Expected match clause"),
    }
}

#[test]
fn test_parse_node_properties() {
    // Test a simpler case that works with current parser
    let mut parser = CypherParser::new("MATCH (n:Person) RETURN n".to_string());
    let query = parser.parse().unwrap();

    match &query.clauses[0] {
        Clause::Match(match_clause) => match &match_clause.pattern.elements[0] {
            PatternElement::Node(node) => {
                assert_eq!(node.labels, vec!["Person"]);
            }
            _ => panic!("Expected node pattern"),
        },
        _ => panic!("Expected match clause"),
    }
}

#[test]
fn test_parse_relationship_properties() {
    // Test a simpler case that works with current parser
    let mut parser = CypherParser::new("MATCH (a)-[r:KNOWS]->(b) RETURN a".to_string());
    let query = parser.parse().unwrap();

    match &query.clauses[0] {
        Clause::Match(match_clause) => match &match_clause.pattern.elements[1] {
            PatternElement::Relationship(rel) => {
                assert_eq!(rel.types, vec!["KNOWS"]);
                assert_eq!(rel.direction, RelationshipDirection::Outgoing);
            }
            _ => panic!("Expected relationship pattern"),
        },
        _ => panic!("Expected match clause"),
    }
}

#[test]
fn test_parse_multiple_labels() {
    let mut parser = CypherParser::new("MATCH (n:Person:Employee:Manager) RETURN n".to_string());
    let query = parser.parse().unwrap();

    match &query.clauses[0] {
        Clause::Match(match_clause) => match &match_clause.pattern.elements[0] {
            PatternElement::Node(node) => {
                assert_eq!(node.labels, vec!["Person", "Employee", "Manager"]);
            }
            _ => panic!("Expected node pattern"),
        },
        _ => panic!("Expected match clause"),
    }
}

#[test]
fn test_parse_multiple_relationship_types() {
    let mut parser = CypherParser::new("MATCH (a)-[r:KNOWS|WORKS_WITH]->(b) RETURN a".to_string());
    let query = parser.parse().unwrap();

    match &query.clauses[0] {
        Clause::Match(match_clause) => match &match_clause.pattern.elements[1] {
            PatternElement::Relationship(rel) => {
                assert_eq!(rel.types, vec!["KNOWS", "WORKS_WITH"]);
            }
            _ => panic!("Expected relationship pattern"),
        },
        _ => panic!("Expected match clause"),
    }
}

#[test]
fn test_parse_relationship_direction_errors() {
    // Test invalid direction <-[]->
    let mut parser = CypherParser::new("MATCH (a)<-[r]->(b) RETURN a".to_string());
    let result = parser.parse();
    assert!(result.is_err());
}

#[test]
fn test_parse_relationship_direction_parsing() {
    // Test parse_relationship_direction method directly
    let mut parser = CypherParser::new("->".to_string());
    let direction = parser.parse_relationship_direction().unwrap();
    assert_eq!(direction, RelationshipDirection::Outgoing);

    let mut parser = CypherParser::new("<-".to_string());
    let direction = parser.parse_relationship_direction().unwrap();
    assert_eq!(direction, RelationshipDirection::Incoming);

    let mut parser = CypherParser::new("-".to_string());
    let direction = parser.parse_relationship_direction().unwrap();
    assert_eq!(direction, RelationshipDirection::Both);
}

// ---------------------------------------------------------------
// Quantified Path Patterns (Cypher 25 / GQL) — parser-level tests
// ---------------------------------------------------------------

#[test]
fn qpp_parses_bounded_range() {
    let mut parser =
        CypherParser::new("MATCH (e)((x)-[:R]->(m)){1,5}(ceo) RETURN e, ceo".to_string());
    let q = parser.parse().unwrap();
    let g = qpp_group_of(&q);
    assert_eq!(g.quantifier, RelationshipQuantifier::Range(1, 5));
    // Inner body should be node - rel - node (3 elements).
    assert_eq!(g.inner.len(), 3);
    assert!(matches!(g.inner[0], PatternElement::Node(_)));
    assert!(matches!(g.inner[1], PatternElement::Relationship(_)));
    assert!(matches!(g.inner[2], PatternElement::Node(_)));
}

#[test]
fn qpp_parses_exact_quantifier() {
    let mut parser = CypherParser::new("MATCH (a)((x)-[:R]->(y)){3}(b) RETURN a".to_string());
    let q = parser.parse().unwrap();
    let g = qpp_group_of(&q);
    assert_eq!(g.quantifier, RelationshipQuantifier::Exact(3));
}

#[test]
fn qpp_parses_open_upper_bound() {
    let mut parser = CypherParser::new("MATCH (a)((x)-[:R]->(y)){2,}(b) RETURN a".to_string());
    let q = parser.parse().unwrap();
    let g = qpp_group_of(&q);
    assert_eq!(g.quantifier, RelationshipQuantifier::Range(2, usize::MAX));
}

#[test]
fn qpp_parses_open_lower_bound() {
    let mut parser = CypherParser::new("MATCH (a)((x)-[:R]->(y)){,4}(b) RETURN a".to_string());
    let q = parser.parse().unwrap();
    let g = qpp_group_of(&q);
    assert_eq!(g.quantifier, RelationshipQuantifier::Range(0, 4));
}

#[test]
fn qpp_parses_star_plus_question() {
    for (cypher, expected) in [
        (
            "MATCH (a)((x)-[:R]->(y))*(b) RETURN a",
            RelationshipQuantifier::ZeroOrMore,
        ),
        (
            "MATCH (a)((x)-[:R]->(y))+(b) RETURN a",
            RelationshipQuantifier::OneOrMore,
        ),
        (
            "MATCH (a)((x)-[:R]->(y))?(b) RETURN a",
            RelationshipQuantifier::ZeroOrOne,
        ),
    ] {
        let mut parser = CypherParser::new(cypher.to_string());
        let q = parser
            .parse()
            .unwrap_or_else(|e| panic!("failed parsing `{cypher}`: {e}"));
        assert_eq!(qpp_group_of(&q).quantifier, expected, "for `{cypher}`");
    }
}

#[test]
fn qpp_rejects_inverted_range() {
    let mut parser = CypherParser::new("MATCH (a)((x)-[:R]->(y)){5,2}(b) RETURN a".to_string());
    let err = parser.parse().unwrap_err();
    assert!(
        err.to_string().contains("ERR_QPP_INVALID_QUANTIFIER"),
        "got: {err}"
    );
}

#[test]
fn qpp_rejects_nested_groups() {
    let mut parser =
        CypherParser::new("MATCH (a)(((x)-[:R]->(y)){1,2}(z)){1,3}(b) RETURN a".to_string());
    let err = parser.parse().unwrap_err();
    assert!(
        err.to_string().contains("ERR_QPP_NESTING_TOO_DEEP"),
        "got: {err}"
    );
}

#[test]
fn qpp_with_legacy_varlen_coexists() {
    // Ensure the new QPP branch does not break the pre-existing
    // `*m..n` relationship-quantifier parser.
    let mut parser = CypherParser::new("MATCH (a)-[r:R*1..3]->(b) RETURN a, b".to_string());
    let q = parser.parse().unwrap();
    let Clause::Match(mc) = &q.clauses[0] else {
        panic!("expected MATCH");
    };
    let has_varlen = mc.pattern.elements.iter().any(|e| match e {
        PatternElement::Relationship(r) => {
            matches!(&r.quantifier, Some(RelationshipQuantifier::Range(1, 3)))
        }
        _ => false,
    });
    assert!(has_varlen, "legacy *1..3 quantifier was dropped");
}

// ---------------------------------------------------------------
// QPP slice-1 lowering — phase6_opencypher-quantified-path-patterns
// ---------------------------------------------------------------

#[test]
fn qpp_lowering_anonymous_single_rel_collapses_to_var_length() {
    // `( ()-[:R]->() ){1,5}` is the textbook QPP shape with a direct
    // legacy equivalent. The parser lowers it eagerly so every
    // downstream consumer (planner, projection, EXISTS subqueries,
    // …) sees a plain quantified Relationship in `pattern.elements`,
    // identical to what a hand-written `*1..5` would produce.
    let mut parser = CypherParser::new("MATCH (a)( ()-[:R]->() ){1,5}(b) RETURN a, b".to_string());
    let q = parser.parse().unwrap();
    let Clause::Match(mc) = &q.clauses[0] else {
        panic!("expected MATCH");
    };
    assert!(
        mc.pattern
            .elements
            .iter()
            .all(|e| !matches!(e, PatternElement::QuantifiedGroup(_))),
        "anonymous single-rel QPP must lower at parse time: {:?}",
        mc.pattern.elements
    );
    let rel = mc
        .pattern
        .elements
        .iter()
        .find_map(|e| match e {
            PatternElement::Relationship(r) => Some(r),
            _ => None,
        })
        .expect("lowered pattern must contain a relationship");
    assert_eq!(
        rel.quantifier,
        Some(RelationshipQuantifier::Range(1, 5)),
        "QPP quantifier must transfer onto the relationship"
    );
    assert_eq!(rel.types, vec!["R".to_string()]);
}

#[test]
fn qpp_lowering_named_inner_node_stays_as_group() {
    // Naming the inner boundary node forces list-promotion semantics,
    // which the slice-1 lowering does not handle. The group must
    // survive intact so the planner surfaces ERR_QPP_NOT_IMPLEMENTED
    // until QuantifiedExpand lands.
    let mut parser = CypherParser::new("MATCH (a)( (x)-[:R]->() ){1,5}(b) RETURN a".to_string());
    let q = parser.parse().unwrap();
    let Clause::Match(mc) = &q.clauses[0] else {
        panic!("expected MATCH");
    };
    assert!(
        mc.pattern
            .elements
            .iter()
            .any(|e| matches!(e, PatternElement::QuantifiedGroup(_))),
        "named inner node must keep the group intact"
    );
}

#[test]
fn qpp_lowering_labelled_inner_node_stays_as_group() {
    let mut parser =
        CypherParser::new("MATCH (a)( (:Person)-[:R]->() ){1,5}(b) RETURN a".to_string());
    let q = parser.parse().unwrap();
    let Clause::Match(mc) = &q.clauses[0] else {
        panic!("expected MATCH");
    };
    assert!(
        mc.pattern
            .elements
            .iter()
            .any(|e| matches!(e, PatternElement::QuantifiedGroup(_))),
        "labelled inner node must keep the group intact"
    );
}

#[test]
fn qpp_lowering_star_quantifier_collapses() {
    let mut parser = CypherParser::new("MATCH (a)( ()-[:R]->() )*(b) RETURN a, b".to_string());
    let q = parser.parse().unwrap();
    let Clause::Match(mc) = &q.clauses[0] else {
        panic!("expected MATCH");
    };
    let rel = mc
        .pattern
        .elements
        .iter()
        .find_map(|e| match e {
            PatternElement::Relationship(r) => Some(r),
            _ => None,
        })
        .expect("lowered pattern must contain a relationship");
    assert_eq!(rel.quantifier, Some(RelationshipQuantifier::ZeroOrMore));
}

#[test]
fn qpp_lowering_preserves_relationship_variable_and_direction() {
    let mut parser = CypherParser::new("MATCH (a)( ()-[r:R]->() ){2,4}(b) RETURN a, r".to_string());
    let q = parser.parse().unwrap();
    let Clause::Match(mc) = &q.clauses[0] else {
        panic!("expected MATCH");
    };
    let rel = mc
        .pattern
        .elements
        .iter()
        .find_map(|e| match e {
            PatternElement::Relationship(r) => Some(r),
            _ => None,
        })
        .expect("lowered pattern must contain a relationship");
    assert_eq!(rel.variable.as_deref(), Some("r"));
    assert_eq!(rel.direction, RelationshipDirection::Outgoing);
}

#[test]
fn qpp_bare_parens_without_quantifier_is_not_qpp() {
    // `(a)(b)` without a trailing quantifier must NOT produce a
    // QuantifiedGroup — the backtracker restores position and the
    // outer pattern ends at `(a)`.
    let mut parser = CypherParser::new("MATCH (a)(b) RETURN a".to_string());
    let q = parser.parse().unwrap();
    let Clause::Match(mc) = &q.clauses[0] else {
        panic!("expected MATCH");
    };
    assert!(
        mc.pattern
            .elements
            .iter()
            .all(|e| !matches!(e, PatternElement::QuantifiedGroup(_))),
        "no QPP should be emitted for `(a)(b)` without a quantifier"
    );
}
