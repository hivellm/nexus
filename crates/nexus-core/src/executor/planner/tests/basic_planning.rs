use super::*;

#[test]
fn test_plan_simple_query() {
    let (catalog, _ctx) = create_test_catalog();
    let label_index = LabelIndex::new();
    let knn_index = KnnIndex::new(crate::index::DEFAULT_VECTORIZER_DIMENSION).unwrap();
    let mut planner = QueryPlanner::new(&catalog, &label_index, &knn_index);

    let query = CypherQuery {
        clauses: vec![
            Clause::Match(MatchClause {
                pattern: Pattern {
                    path_variable: None,
                    extra_path_variables: Vec::new(),
                    elements: vec![PatternElement::Node(NodePattern {
                        variable: Some("n".to_string()),
                        labels: vec!["Person".to_string()],
                        properties: None,
                        external_id_expr: None,
                    })],
                },
                where_clause: None,
                optional: false,
                hints: vec![],
            }),
            Clause::Return(ReturnClause {
                items: vec![ReturnItem {
                    expression: Expression::Variable("n".to_string()),
                    alias: None,
                }],
                distinct: false,
            }),
        ],
        params: std::collections::HashMap::new(),
        graph_scope: None,
    };

    let operators = planner.plan_query(&query).unwrap();
    assert_eq!(operators.len(), 2);

    match &operators[0] {
        Operator::NodeByLabel { variable, .. } => {
            assert_eq!(variable, "n");
        }
        _ => panic!("Expected NodeByLabel operator"),
    }

    match &operators[1] {
        Operator::Project { items } => {
            assert_eq!(items.len(), 1);
            assert_eq!(items[0].alias, "n");
        }
        _ => panic!("Expected Project operator"),
    }
}

#[test]
fn test_estimate_cost() {
    let (catalog, _ctx) = create_test_catalog();
    let label_index = LabelIndex::new();
    let knn_index = KnnIndex::new(crate::index::DEFAULT_VECTORIZER_DIMENSION).unwrap();
    let mut planner = QueryPlanner::new(&catalog, &label_index, &knn_index);

    let operators = vec![
        Operator::NodeByLabel {
            label_id: 1,
            variable: "n".to_string(),
        },
        Operator::Filter {
            predicate: "n.age > 18".to_string(),
            predicate_ast: None,
        },
        Operator::Project {
            items: vec![ProjectionItem {
                alias: "n".to_string(),
                expression: Expression::Variable("n".to_string()),
            }],
        },
    ];

    let cost = planner.estimate_cost(&operators).unwrap();
    assert!(cost > 0.0);
}

#[test]
fn test_plan_query_with_where_clause() {
    let (catalog, _ctx) = create_test_catalog();
    let label_index = LabelIndex::new();
    let knn_index = KnnIndex::new(crate::index::DEFAULT_VECTORIZER_DIMENSION).unwrap();
    let mut planner = QueryPlanner::new(&catalog, &label_index, &knn_index);

    let query = CypherQuery {
        clauses: vec![
            Clause::Match(MatchClause {
                pattern: Pattern {
                    path_variable: None,
                    extra_path_variables: Vec::new(),
                    elements: vec![PatternElement::Node(NodePattern {
                        variable: Some("n".to_string()),
                        labels: vec!["Person".to_string()],
                        properties: None,
                        external_id_expr: None,
                    })],
                },
                where_clause: Some(WhereClause {
                    expression: Expression::BinaryOp {
                        left: Box::new(Expression::PropertyAccess {
                            variable: "n".to_string(),
                            property: "age".to_string(),
                        }),
                        op: BinaryOperator::GreaterThan,
                        right: Box::new(Expression::Literal(Literal::Integer(18))),
                    },
                }),
                optional: false,
                hints: vec![],
            }),
            Clause::Return(ReturnClause {
                items: vec![ReturnItem {
                    expression: Expression::Variable("n".to_string()),
                    alias: None,
                }],
                distinct: false,
            }),
        ],
        params: std::collections::HashMap::new(),
        graph_scope: None,
    };

    let operators = planner.plan_query(&query).unwrap();
    assert_eq!(operators.len(), 3); // NodeByLabel, Filter, Project

    match &operators[0] {
        Operator::NodeByLabel { variable, .. } => {
            assert_eq!(variable, "n");
        }
        _ => panic!("Expected NodeByLabel operator"),
    }

    match &operators[1] {
        Operator::Filter { predicate, .. } => {
            assert!(predicate.contains("n.age"));
            assert!(predicate.contains(">"));
            assert!(predicate.contains("18"));
        }
        _ => panic!("Expected Filter operator"),
    }

    match &operators[2] {
        Operator::Project { items } => {
            assert_eq!(items.len(), 1);
            assert_eq!(items[0].alias, "n");
        }
        _ => panic!("Expected Project operator"),
    }
}

#[test]
fn test_plan_query_with_limit() {
    let (catalog, _ctx) = create_test_catalog();
    let label_index = LabelIndex::new();
    let knn_index = KnnIndex::new(crate::index::DEFAULT_VECTORIZER_DIMENSION).unwrap();
    let mut planner = QueryPlanner::new(&catalog, &label_index, &knn_index);

    let query = CypherQuery {
        clauses: vec![
            Clause::Match(MatchClause {
                pattern: Pattern {
                    path_variable: None,
                    extra_path_variables: Vec::new(),
                    elements: vec![PatternElement::Node(NodePattern {
                        variable: Some("n".to_string()),
                        labels: vec!["Person".to_string()],
                        properties: None,
                        external_id_expr: None,
                    })],
                },
                where_clause: None,
                optional: false,
                hints: vec![],
            }),
            Clause::Return(ReturnClause {
                items: vec![ReturnItem {
                    expression: Expression::Variable("n".to_string()),
                    alias: None,
                }],
                distinct: false,
            }),
            Clause::Limit(LimitClause {
                count: Expression::Literal(Literal::Integer(10)),
            }),
        ],
        params: std::collections::HashMap::new(),
        graph_scope: None,
    };

    let operators = planner.plan_query(&query).unwrap();
    assert_eq!(operators.len(), 3); // NodeByLabel, Project, Limit

    match &operators[2] {
        Operator::Limit { count } => {
            assert_eq!(*count, 10);
        }
        _ => panic!("Expected Limit operator"),
    }
}

#[test]
fn test_plan_query_with_relationship() {
    let (catalog, _ctx) = create_test_catalog();
    let label_index = LabelIndex::new();
    let knn_index = KnnIndex::new(crate::index::DEFAULT_VECTORIZER_DIMENSION).unwrap();
    let mut planner = QueryPlanner::new(&catalog, &label_index, &knn_index);

    let query = CypherQuery {
        clauses: vec![
            Clause::Match(MatchClause {
                pattern: Pattern {
                    path_variable: None,
                    extra_path_variables: Vec::new(),
                    elements: vec![
                        PatternElement::Node(NodePattern {
                            variable: Some("a".to_string()),
                            labels: vec!["Person".to_string()],
                            properties: None,
                            external_id_expr: None,
                        }),
                        PatternElement::Relationship(RelationshipPattern {
                            variable: Some("r".to_string()),
                            types: vec!["KNOWS".to_string()],
                            direction: RelationshipDirection::Outgoing,
                            properties: None,
                            quantifier: None,
                        }),
                        PatternElement::Node(NodePattern {
                            variable: Some("b".to_string()),
                            labels: vec!["Person".to_string()],
                            properties: None,
                            external_id_expr: None,
                        }),
                    ],
                },
                where_clause: None,
                optional: false,
                hints: vec![],
            }),
            Clause::Return(ReturnClause {
                items: vec![ReturnItem {
                    expression: Expression::Variable("a".to_string()),
                    alias: None,
                }],
                distinct: false,
            }),
        ],
        params: std::collections::HashMap::new(),
        graph_scope: None,
    };

    let operators = planner.plan_query(&query).unwrap();
    assert!(operators.len() >= 2); // At least NodeByLabel and Project

    // Check for Expand operator
    let has_expand = operators
        .iter()
        .any(|op| matches!(op, Operator::Expand { .. }));
    assert!(has_expand, "Expected Expand operator for relationship");
}

#[test]
fn test_plan_query_with_variable_length_path() {
    let (catalog, _ctx) = create_test_catalog();
    let label_index = LabelIndex::new();
    let knn_index = KnnIndex::new(crate::index::DEFAULT_VECTORIZER_DIMENSION).unwrap();
    let mut planner = QueryPlanner::new(&catalog, &label_index, &knn_index);

    let query = CypherQuery {
        clauses: vec![
            Clause::Match(MatchClause {
                pattern: Pattern {
                    path_variable: None,
                    extra_path_variables: Vec::new(),
                    elements: vec![
                        PatternElement::Node(NodePattern {
                            variable: Some("a".to_string()),
                            labels: vec!["Person".to_string()],
                            properties: None,
                            external_id_expr: None,
                        }),
                        PatternElement::Relationship(RelationshipPattern {
                            variable: Some("r".to_string()),
                            types: vec!["KNOWS".to_string()],
                            direction: RelationshipDirection::Outgoing,
                            properties: None,
                            quantifier: Some(RelationshipQuantifier::ZeroOrMore),
                        }),
                        PatternElement::Node(NodePattern {
                            variable: Some("b".to_string()),
                            labels: vec!["Person".to_string()],
                            properties: None,
                            external_id_expr: None,
                        }),
                    ],
                },
                where_clause: None,
                optional: false,
                hints: vec![],
            }),
            Clause::Return(ReturnClause {
                items: vec![ReturnItem {
                    expression: Expression::Variable("a".to_string()),
                    alias: None,
                }],
                distinct: false,
            }),
        ],
        params: std::collections::HashMap::new(),
        graph_scope: None,
    };

    let operators = planner.plan_query(&query).unwrap();

    // Check for VariableLengthPath operator
    let has_variable_length_path = operators
        .iter()
        .any(|op| matches!(op, Operator::VariableLengthPath { .. }));
    assert!(
        has_variable_length_path,
        "Expected VariableLengthPath operator for variable-length relationship"
    );

    // Should NOT have regular Expand operator
    let has_expand = operators
        .iter()
        .any(|op| matches!(op, Operator::Expand { .. }));
    assert!(
        !has_expand,
        "Should not have Expand operator when quantifier is present"
    );
}

#[test]
fn test_plan_query_with_range_quantifier() {
    let (catalog, _ctx) = create_test_catalog();
    let label_index = LabelIndex::new();
    let knn_index = KnnIndex::new(crate::index::DEFAULT_VECTORIZER_DIMENSION).unwrap();
    let mut planner = QueryPlanner::new(&catalog, &label_index, &knn_index);

    let query = CypherQuery {
        clauses: vec![
            Clause::Match(MatchClause {
                pattern: Pattern {
                    path_variable: None,
                    extra_path_variables: Vec::new(),
                    elements: vec![
                        PatternElement::Node(NodePattern {
                            variable: Some("a".to_string()),
                            labels: vec!["Person".to_string()],
                            properties: None,
                            external_id_expr: None,
                        }),
                        PatternElement::Relationship(RelationshipPattern {
                            variable: Some("r".to_string()),
                            types: vec!["KNOWS".to_string()],
                            direction: RelationshipDirection::Outgoing,
                            properties: None,
                            quantifier: Some(RelationshipQuantifier::Range(1, 3)),
                        }),
                        PatternElement::Node(NodePattern {
                            variable: Some("b".to_string()),
                            labels: vec!["Person".to_string()],
                            properties: None,
                            external_id_expr: None,
                        }),
                    ],
                },
                where_clause: None,
                optional: false,
                hints: vec![],
            }),
            Clause::Return(ReturnClause {
                items: vec![ReturnItem {
                    expression: Expression::Variable("a".to_string()),
                    alias: None,
                }],
                distinct: false,
            }),
        ],
        params: std::collections::HashMap::new(),
        graph_scope: None,
    };

    let operators = planner.plan_query(&query).unwrap();

    // Check for VariableLengthPath operator with Range quantifier
    let has_variable_length_path = operators.iter().any(|op| {
        if let Operator::VariableLengthPath { quantifier, .. } = op {
            matches!(quantifier, RelationshipQuantifier::Range(1, 3))
        } else {
            false
        }
    });
    assert!(
        has_variable_length_path,
        "Expected VariableLengthPath operator with Range quantifier"
    );
}
