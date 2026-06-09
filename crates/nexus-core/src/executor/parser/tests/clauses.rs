//! Tests for top-level clause parsing: MATCH, WHERE, RETURN, ORDER BY,
//! SKIP, LIMIT, WITH, OPTIONAL MATCH, UNWIND, and UNION.

use super::*;

#[test]
fn test_parse_simple_match() {
    let mut parser = CypherParser::new("MATCH (n:Person) RETURN n".to_string());
    let query = parser.parse().unwrap();

    assert_eq!(query.clauses.len(), 2);

    match &query.clauses[0] {
        Clause::Match(match_clause) => {
            assert_eq!(match_clause.pattern.elements.len(), 1);
            match &match_clause.pattern.elements[0] {
                PatternElement::Node(node) => {
                    assert_eq!(node.variable, Some("n".to_string()));
                    assert_eq!(node.labels, vec!["Person"]);
                }
                _ => panic!("Expected node pattern"),
            }
        }
        _ => panic!("Expected match clause"),
    }

    match &query.clauses[1] {
        Clause::Return(return_clause) => {
            assert_eq!(return_clause.items.len(), 1);
            assert!(!return_clause.distinct);
        }
        _ => panic!("Expected return clause"),
    }
}

#[test]
fn test_parse_match_with_where() {
    let mut parser = CypherParser::new("MATCH (n:Person) WHERE n.age > 18 RETURN n".to_string());
    let query = parser.parse().unwrap();

    assert_eq!(query.clauses.len(), 3);

    match &query.clauses[0] {
        Clause::Match(match_clause) => {
            assert!(match_clause.where_clause.is_none());
        }
        _ => panic!("Expected match clause"),
    }

    match &query.clauses[1] {
        Clause::Where(where_clause) => {
            // Check that it's a binary operation
            match &where_clause.expression {
                Expression::BinaryOp { op, .. } => {
                    assert_eq!(*op, BinaryOperator::GreaterThan);
                }
                _ => panic!("Expected binary operation"),
            }
        }
        _ => panic!("Expected where clause"),
    }
}

#[test]
fn test_parse_relationship_pattern() {
    let mut parser =
        CypherParser::new("MATCH (a:Person)-[r:KNOWS]->(b:Person) RETURN a, b".to_string());
    let query = parser.parse().unwrap();

    match &query.clauses[0] {
        Clause::Match(match_clause) => {
            assert_eq!(match_clause.pattern.elements.len(), 3); // node, rel, node

            match &match_clause.pattern.elements[1] {
                PatternElement::Relationship(rel) => {
                    assert_eq!(rel.variable, Some("r".to_string()));
                    assert_eq!(rel.types, vec!["KNOWS"]);
                    assert_eq!(rel.direction, RelationshipDirection::Outgoing);
                }
                _ => panic!("Expected relationship pattern"),
            }
        }
        _ => panic!("Expected match clause"),
    }
}

#[test]
fn test_parse_return_with_alias() {
    let mut parser = CypherParser::new("MATCH (n:Person) RETURN n.name AS person_name".to_string());
    let query = parser.parse().unwrap();

    match &query.clauses[1] {
        Clause::Return(return_clause) => {
            assert_eq!(return_clause.items.len(), 1);

            let ReturnItem { expression, alias } = &return_clause.items[0];
            {
                assert_eq!(alias, &Some("person_name".to_string()));

                match expression {
                    Expression::PropertyAccess { variable, property } => {
                        assert_eq!(variable, "n");
                        assert_eq!(property, "name");
                    }
                    _ => panic!("Expected property access"),
                }
            }
        }
        _ => panic!("Expected return clause"),
    }
}

#[test]
fn test_parse_order_by() {
    let mut parser = CypherParser::new("MATCH (n:Person) RETURN n ORDER BY n.age DESC".to_string());
    let query = parser.parse().unwrap();

    assert_eq!(query.clauses.len(), 3);

    match &query.clauses[2] {
        Clause::OrderBy(order_clause) => {
            assert_eq!(order_clause.items.len(), 1);
            assert_eq!(order_clause.items[0].direction, SortDirection::Descending);
        }
        _ => panic!("Expected order by clause"),
    }
}

#[test]
fn test_parse_limit_skip() {
    let mut parser = CypherParser::new("MATCH (n:Person) RETURN n SKIP 10 LIMIT 5".to_string());
    let query = parser.parse().unwrap();

    assert_eq!(query.clauses.len(), 4);

    match &query.clauses[2] {
        Clause::Skip(skip_clause) => match &skip_clause.count {
            Expression::Literal(Literal::Integer(10)) => {}
            _ => panic!("Expected integer literal"),
        },
        _ => panic!("Expected skip clause"),
    }

    match &query.clauses[3] {
        Clause::Limit(limit_clause) => match &limit_clause.count {
            Expression::Literal(Literal::Integer(5)) => {}
            _ => panic!("Expected integer literal"),
        },
        _ => panic!("Expected limit clause"),
    }
}

#[test]
fn test_parse_complex_query() {
    let query_str = r#"
            MATCH (p:Person)-[r:KNOWS]->(f:Person)
            WHERE p.age > $min_age AND f.city = $city
            RETURN p.name AS person_name, f.name AS friend_name, r.since AS friendship_since
            ORDER BY friendship_since DESC
            LIMIT 10
        "#;

    let mut parser = CypherParser::new(query_str.to_string());
    let query = parser.parse().unwrap();

    assert_eq!(query.clauses.len(), 5); // MATCH, WHERE, RETURN, ORDER BY, LIMIT

    // Verify all clause types are present
    let clause_types: Vec<&str> = query
        .clauses
        .iter()
        .map(|c| match c {
            Clause::Match(_) => "MATCH",
            Clause::Where(_) => "WHERE",
            Clause::Return(_) => "RETURN",
            Clause::OrderBy(_) => "ORDER BY",
            Clause::Limit(_) => "LIMIT",
            _ => "OTHER",
        })
        .collect();

    assert_eq!(
        clause_types,
        vec!["MATCH", "WHERE", "RETURN", "ORDER BY", "LIMIT"]
    );
}

#[test]
fn test_parse_return_distinct() {
    let mut parser = CypherParser::new("MATCH (n:Person) RETURN DISTINCT n.name".to_string());
    let query = parser.parse().unwrap();

    match &query.clauses[1] {
        Clause::Return(return_clause) => {
            assert!(return_clause.distinct);
        }
        _ => panic!("Expected return clause"),
    }
}

#[test]
fn test_parse_multiple_return_items() {
    let mut parser = CypherParser::new("MATCH (n:Person) RETURN n.name, n.age, n.city".to_string());
    let query = parser.parse().unwrap();

    match &query.clauses[1] {
        Clause::Return(return_clause) => {
            assert_eq!(return_clause.items.len(), 3);
        }
        _ => panic!("Expected return clause"),
    }
}

#[test]
fn test_parse_order_by_ascending() {
    let mut parser = CypherParser::new("MATCH (n:Person) RETURN n ORDER BY n.age ASC".to_string());
    let query = parser.parse().unwrap();

    match &query.clauses[2] {
        Clause::OrderBy(order_clause) => {
            assert_eq!(order_clause.items[0].direction, SortDirection::Ascending);
        }
        _ => panic!("Expected order by clause"),
    }
}

#[test]
fn test_parse_multiple_order_by() {
    let mut parser =
        CypherParser::new("MATCH (n:Person) RETURN n ORDER BY n.age DESC, n.name ASC".to_string());
    let query = parser.parse().unwrap();

    match &query.clauses[2] {
        Clause::OrderBy(order_clause) => {
            assert_eq!(order_clause.items.len(), 2);
            assert_eq!(order_clause.items[0].direction, SortDirection::Descending);
            assert_eq!(order_clause.items[1].direction, SortDirection::Ascending);
        }
        _ => panic!("Expected order by clause"),
    }
}

#[test]
fn test_parse_skip_clause() {
    let mut parser = CypherParser::new("MATCH (n:Person) RETURN n SKIP 5".to_string());
    let query = parser.parse().unwrap();

    match &query.clauses[2] {
        Clause::Skip(skip_clause) => match &skip_clause.count {
            Expression::Literal(Literal::Integer(5)) => {}
            _ => panic!("Expected integer literal"),
        },
        _ => panic!("Expected skip clause"),
    }
}

#[test]
fn test_parse_with_clause() {
    let mut parser = CypherParser::new("WITH n, m.age AS age RETURN age".to_string());
    let query = parser.parse().unwrap();

    assert_eq!(query.clauses.len(), 2);

    match &query.clauses[0] {
        Clause::With(with_clause) => {
            assert_eq!(with_clause.items.len(), 2);
            assert!(!with_clause.distinct);
            assert!(with_clause.where_clause.is_none());

            match &with_clause.items[0].expression {
                Expression::Variable(name) => assert_eq!(name, "n"),
                _ => panic!("Expected variable expression"),
            }

            match &with_clause.items[1].expression {
                Expression::PropertyAccess { variable, property } => {
                    assert_eq!(variable, "m");
                    assert_eq!(property, "age");
                }
                _ => panic!("Expected property access expression"),
            }

            assert_eq!(with_clause.items[1].alias, Some("age".to_string()));
        }
        _ => panic!("Expected WITH clause"),
    }
}

#[test]
fn test_parse_with_distinct() {
    let mut parser = CypherParser::new("WITH DISTINCT n, m RETURN n".to_string());
    let query = parser.parse().unwrap();

    match &query.clauses[0] {
        Clause::With(with_clause) => {
            assert!(with_clause.distinct);
            assert_eq!(with_clause.items.len(), 2);
        }
        _ => panic!("Expected WITH clause"),
    }
}

#[test]
fn test_parse_with_where() {
    let mut parser = CypherParser::new("WITH n WHERE n.age > 30 RETURN n".to_string());
    let query = parser.parse().unwrap();

    assert_eq!(query.clauses.len(), 2);

    match &query.clauses[0] {
        Clause::With(with_clause) => {
            assert!(with_clause.where_clause.is_some());
        }
        _ => panic!("Expected WITH clause"),
    }
}

#[test]
fn test_with_clause_boundary() {
    let parser = CypherParser::new("WITH n".to_string());
    assert!(parser.is_clause_boundary());

    let parser = CypherParser::new("  WITH n".to_string());
    assert!(parser.is_clause_boundary());
}

#[test]
fn test_parse_optional_match() {
    let mut parser =
        CypherParser::new("OPTIONAL MATCH (p:Person)-[r:KNOWS]->(f:Person) RETURN p".to_string());
    let query = parser.parse().unwrap();

    assert!(!query.clauses.is_empty(), "Expected at least one clause");

    match &query.clauses[0] {
        Clause::Match(match_clause) => {
            assert!(match_clause.optional);
        }
        _ => panic!("Expected MATCH clause, got: {:?}", query.clauses[0]),
    }
}

#[test]
fn test_parse_optional_match_with_where() {
    let mut parser = CypherParser::new(
        "MATCH (p:Person) OPTIONAL MATCH (p)-[r:KNOWS]->(f:Person) RETURN p, f".to_string(),
    );
    let query = parser.parse().unwrap();

    assert_eq!(query.clauses.len(), 3); // MATCH, OPTIONAL MATCH, RETURN

    match &query.clauses[0] {
        Clause::Match(match_clause) => {
            assert!(!match_clause.optional);
        }
        _ => panic!("Expected regular MATCH clause"),
    }

    match &query.clauses[1] {
        Clause::Match(match_clause) => {
            assert!(match_clause.optional);
        }
        _ => panic!("Expected OPTIONAL MATCH clause"),
    }
}

#[test]
fn test_parse_multiple_optional_matches() {
    let mut parser = CypherParser::new(
            "MATCH (p:Person) OPTIONAL MATCH (p)-[r1]->(friend) OPTIONAL MATCH (p)-[r2]->(colleague) RETURN p, friend, colleague".to_string(),
        );
    let query = parser.parse().unwrap();

    assert_eq!(query.clauses.len(), 4); // MATCH, OPTIONAL MATCH, OPTIONAL MATCH, RETURN

    match &query.clauses[1] {
        Clause::Match(match_clause) => assert!(match_clause.optional),
        _ => panic!("Expected first OPTIONAL MATCH"),
    }

    match &query.clauses[2] {
        Clause::Match(match_clause) => assert!(match_clause.optional),
        _ => panic!("Expected second OPTIONAL MATCH"),
    }
}

#[test]
fn test_parse_unwind_clause() {
    let mut parser = CypherParser::new("UNWIND [1, 2, 3] AS x RETURN x".to_string());
    let query = parser.parse().unwrap();

    assert!(!query.clauses.is_empty());

    match &query.clauses[0] {
        Clause::Unwind(unwind_clause) => {
            // Check that expression is parsed correctly
            assert!(matches!(&unwind_clause.expression, Expression::List(_)));
            assert_eq!(unwind_clause.variable, "x");
        }
        _ => panic!("Expected UNWIND clause"),
    }
}

#[test]
fn test_unwind_clause_boundary() {
    let parser = CypherParser::new("UNWIND [1, 2, 3] AS x".to_string());
    assert!(parser.is_clause_boundary());

    let parser = CypherParser::new("  UNWIND [1, 2, 3] AS x".to_string());
    assert!(parser.is_clause_boundary());
}

#[test]
fn test_parse_union_clause() {
    let mut parser =
        CypherParser::new("MATCH (n:Person) RETURN n UNION MATCH (m:Company) RETURN m".to_string());
    let query = parser.parse().unwrap();

    // UNION splits into two separate queries
    assert_eq!(query.clauses.len(), 5); // MATCH, RETURN, UNION, MATCH, RETURN

    match &query.clauses[2] {
        Clause::Union(union_clause) => {
            assert_eq!(union_clause.union_type, UnionType::Distinct);
        }
        _ => panic!("Expected UNION clause"),
    }
}

#[test]
fn test_parse_union_all_clause() {
    let mut parser = CypherParser::new(
        "MATCH (n:Person) RETURN n UNION ALL MATCH (m:Company) RETURN m".to_string(),
    );
    let query = parser.parse().unwrap();

    // UNION ALL splits into two separate queries
    assert_eq!(query.clauses.len(), 5); // MATCH, RETURN, UNION ALL, MATCH, RETURN

    // Check that UNION ALL clause is parsed
    let has_union = query.clauses.iter().any(|c| matches!(c, Clause::Union(_)));
    assert!(has_union, "Expected UNION ALL clause in query");

    // Find the UNION clause and check its type
    for clause in &query.clauses {
        if let Clause::Union(union_clause) = clause {
            assert_eq!(union_clause.union_type, UnionType::All);
            return;
        }
    }
    panic!("Expected UNION ALL clause");
}

#[test]
fn test_union_clause_boundary() {
    let parser = CypherParser::new("UNION MATCH (n) RETURN n".to_string());
    assert!(parser.is_clause_boundary());

    let parser = CypherParser::new("  UNION ALL MATCH (n) RETURN n".to_string());
    assert!(parser.is_clause_boundary());
}

// ── Standalone-WHERE rejection (phase3_unwind-where-neo4j-parity §1) ──
//
// Neo4j 2025.09.0 rejects `WHERE` that isn't attached to a `MATCH` /
// `OPTIONAL MATCH` / `WITH` with a syntax error listing the valid
// follow-up clauses. Nexus now emits a message of the same shape so
// queries that worked against the old permissive parser fail with an
// actionable error pointing callers at the `WITH <vars>` migration.

#[test]
fn standalone_where_after_unwind_rejects() {
    let mut parser = CypherParser::new("UNWIND [1, 2, 3] AS x WHERE x > 1 RETURN x".to_string());
    let err = parser
        .parse()
        .expect_err("standalone WHERE after UNWIND must reject");
    let msg = format!("{}", err);
    assert!(
        msg.contains("Invalid input 'WHERE'"),
        "expected Neo4j-style syntax error, got: {msg}"
    );
    assert!(
        msg.contains("'WITH'"),
        "error must point callers at the WITH migration, got: {msg}"
    );
}

#[test]
fn standalone_where_after_create_rejects() {
    let mut parser = CypherParser::new("CREATE (n:X) WHERE n.x = 1 RETURN n".to_string());
    let err = parser
        .parse()
        .expect_err("standalone WHERE after CREATE must reject");
    let msg = format!("{}", err);
    assert!(
        msg.contains("Invalid input 'WHERE'"),
        "expected Neo4j-style syntax error, got: {msg}"
    );
}

#[test]
fn standalone_where_after_delete_rejects() {
    let mut parser = CypherParser::new("MATCH (n) DELETE n WHERE n.x = 1 RETURN n".to_string());
    let err = parser
        .parse()
        .expect_err("standalone WHERE after DELETE must reject");
    let msg = format!("{}", err);
    assert!(
        msg.contains("Invalid input 'WHERE'"),
        "expected Neo4j-style syntax error, got: {msg}"
    );
}

#[test]
fn match_where_still_parses() {
    // In Nexus, WHERE after MATCH is emitted as a standalone
    // `Clause::Where` that the executor attaches to the preceding
    // MATCH at run time (see `executor::mod`'s dispatch loop).
    // That path must keep working after the reject arm — it's the
    // single most common Cypher shape.
    let mut parser = CypherParser::new("MATCH (n:Person) WHERE n.age > 30 RETURN n".to_string());
    let query = parser.parse().expect("MATCH … WHERE must parse");
    assert_eq!(
        query.clauses.len(),
        3,
        "expected MATCH + WHERE + RETURN clauses"
    );
    assert!(
        matches!(query.clauses[0], Clause::Match(_)),
        "first clause must be MATCH"
    );
    assert!(
        matches!(query.clauses[1], Clause::Where(_)),
        "second clause must be the standalone WHERE that MATCH accepts"
    );
}

#[test]
fn with_where_still_parses() {
    // WITH absorbs its attached WHERE into its own struct via
    // `parse_with_clause`, so the clause stream is UNWIND + WITH +
    // RETURN (three clauses, not four). This is the canonical
    // migration target for the previously-accepted shorthand.
    let mut parser =
        CypherParser::new("UNWIND [1, 2, 3, 4, 5] AS x WITH x WHERE x > 2 RETURN x".to_string());
    let query = parser.parse().expect("UNWIND … WITH x WHERE must parse");
    assert_eq!(
        query.clauses.len(),
        3,
        "expected UNWIND + WITH + RETURN clauses"
    );
    match &query.clauses[1] {
        Clause::With(w) => assert!(
            w.where_clause.is_some(),
            "WITH clause must own its attached WHERE"
        ),
        other => panic!("expected Clause::With, got {:?}", other),
    }
}

#[test]
fn optional_match_where_still_parses() {
    // OPTIONAL MATCH produces a `Clause::Match` with `optional=true`
    // (not a distinct variant), so the follow-up WHERE is accepted
    // by the same context check MATCH uses. Result shape: MATCH +
    // MATCH(optional) + WHERE + RETURN = four clauses.
    let mut parser =
        CypherParser::new("MATCH (a) OPTIONAL MATCH (b) WHERE b.x > 0 RETURN a, b".to_string());
    let query = parser.parse().expect("OPTIONAL MATCH … WHERE must parse");
    assert_eq!(
        query.clauses.len(),
        4,
        "expected MATCH + OPTIONAL MATCH + WHERE + RETURN"
    );
    match &query.clauses[1] {
        Clause::Match(m) => assert!(m.optional, "second clause is OPTIONAL MATCH"),
        other => panic!("expected Clause::Match, got {:?}", other),
    }
    assert!(matches!(query.clauses[2], Clause::Where(_)));
}
