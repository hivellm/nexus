use super::*;

#[test]
fn test_plan_query_empty_patterns() {
    let (catalog, _ctx) = create_test_catalog();
    let label_index = LabelIndex::new();
    let knn_index = KnnIndex::new(crate::index::DEFAULT_VECTORIZER_DIMENSION).unwrap();
    let mut planner = QueryPlanner::new(&catalog, &label_index, &knn_index);

    let query = CypherQuery {
        clauses: vec![],
        params: std::collections::HashMap::new(),
        graph_scope: None,
    };

    let result = planner.plan_query(&query);
    assert!(result.is_err());
}

#[test]
fn test_expression_to_string_variable() {
    let (catalog, _ctx) = create_test_catalog();
    let label_index = LabelIndex::new();
    let knn_index = KnnIndex::new(crate::index::DEFAULT_VECTORIZER_DIMENSION).unwrap();
    let mut planner = QueryPlanner::new(&catalog, &label_index, &knn_index);

    let expr = Expression::Variable("test_var".to_string());
    let result = planner.expression_to_string(&expr).unwrap();
    assert_eq!(result, "test_var");
}

#[test]
fn test_expression_to_string_property_access() {
    let (catalog, _ctx) = create_test_catalog();
    let label_index = LabelIndex::new();
    let knn_index = KnnIndex::new(crate::index::DEFAULT_VECTORIZER_DIMENSION).unwrap();
    let mut planner = QueryPlanner::new(&catalog, &label_index, &knn_index);

    let expr = Expression::PropertyAccess {
        variable: "n".to_string(),
        property: "age".to_string(),
    };
    let result = planner.expression_to_string(&expr).unwrap();
    assert_eq!(result, "n.age");
}

#[test]
fn test_expression_to_string_literals() {
    let (catalog, _ctx) = create_test_catalog();
    let label_index = LabelIndex::new();
    let knn_index = KnnIndex::new(crate::index::DEFAULT_VECTORIZER_DIMENSION).unwrap();
    let mut planner = QueryPlanner::new(&catalog, &label_index, &knn_index);

    // Test string literal - use single quotes for Neo4j compatibility (fixed in Phase 1)
    let expr = Expression::Literal(Literal::String("hello".to_string()));
    let result = planner.expression_to_string(&expr).unwrap();
    assert_eq!(result, "'hello'");

    // Test integer literal
    let expr = Expression::Literal(Literal::Integer(42));
    let result = planner.expression_to_string(&expr).unwrap();
    assert_eq!(result, "42");

    // Test float literal
    let expr = Expression::Literal(Literal::Float(std::f64::consts::PI));
    let result = planner.expression_to_string(&expr).unwrap();
    assert_eq!(result, "3.141592653589793");

    // Test boolean literal
    let expr = Expression::Literal(Literal::Boolean(true));
    let result = planner.expression_to_string(&expr).unwrap();
    assert_eq!(result, "true");

    // Test null literal
    let expr = Expression::Literal(Literal::Null);
    let result = planner.expression_to_string(&expr).unwrap();
    assert_eq!(result, "NULL");
}

#[test]
fn test_expression_to_string_binary_operators() {
    let (catalog, _ctx) = create_test_catalog();
    let label_index = LabelIndex::new();
    let knn_index = KnnIndex::new(crate::index::DEFAULT_VECTORIZER_DIMENSION).unwrap();
    let mut planner = QueryPlanner::new(&catalog, &label_index, &knn_index);

    let expr = Expression::BinaryOp {
        left: Box::new(Expression::Variable("a".to_string())),
        op: BinaryOperator::Equal,
        right: Box::new(Expression::Variable("b".to_string())),
    };
    let result = planner.expression_to_string(&expr).unwrap();
    assert_eq!(result, "a = b");

    let expr = Expression::BinaryOp {
        left: Box::new(Expression::Variable("x".to_string())),
        op: BinaryOperator::GreaterThan,
        right: Box::new(Expression::Literal(Literal::Integer(10))),
    };
    let result = planner.expression_to_string(&expr).unwrap();
    assert_eq!(result, "x > 10");
}

#[test]
fn test_expression_to_string_parameter() {
    let (catalog, _ctx) = create_test_catalog();
    let label_index = LabelIndex::new();
    let knn_index = KnnIndex::new(crate::index::DEFAULT_VECTORIZER_DIMENSION).unwrap();
    let mut planner = QueryPlanner::new(&catalog, &label_index, &knn_index);

    let expr = Expression::Parameter("param1".to_string());
    let result = planner.expression_to_string(&expr).unwrap();
    assert_eq!(result, "$param1");
}

#[test]
fn test_estimate_cost_all_operators() {
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
        Operator::Expand {
            type_ids: vec![1],
            source_var: "n".to_string(),
            target_var: "m".to_string(),
            rel_var: "r".to_string(),
            direction: Direction::Outgoing,
            optional: false,
        },
        Operator::Project {
            items: vec![ProjectionItem {
                alias: "n".to_string(),
                expression: Expression::Variable("n".to_string()),
            }],
        },
        Operator::Limit { count: 10 },
        Operator::Sort {
            columns: vec!["n.name".to_string()],
            ascending: vec![true],
        },
        Operator::Aggregate {
            group_by: vec!["n".to_string()],
            aggregations: vec![],
            projection_items: None,
            output_order: None,
            source: None,
            streaming_optimized: false,
            push_down_optimized: false,
        },
        Operator::Union {
            left: vec![Operator::NodeByLabel {
                label_id: 1,
                variable: "a".to_string(),
            }],
            right: vec![Operator::NodeByLabel {
                label_id: 2,
                variable: "b".to_string(),
            }],
            distinct: true,
        },
        Operator::Join {
            left: Box::new(Operator::NodeByLabel {
                label_id: 1,
                variable: "a".to_string(),
            }),
            right: Box::new(Operator::NodeByLabel {
                label_id: 2,
                variable: "b".to_string(),
            }),
            join_type: JoinType::Inner,
            condition: Some("a.id = b.id".to_string()),
        },
        Operator::IndexScan {
            index_name: "label_Person".to_string(),
            label: "Person".to_string(),
        },
        Operator::Distinct {
            columns: vec!["n".to_string()],
        },
    ];

    let cost = planner.estimate_cost(&operators).unwrap();
    assert!(cost > 0.0);
    // Should be substantial with all operators (adjusted threshold)
    assert!(cost > 100.0);
}

#[test]
fn test_optimize_operator_order() {
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
    ];

    let optimized = planner.optimize_operator_order(operators.clone()).unwrap();
    assert_eq!(optimized.len(), operators.len());
    // For MVP, should return same order
    // For MVP, should return same order
    assert_eq!(optimized.len(), operators.len());
}

#[test]
fn test_plan_query_with_return_alias() {
    let (catalog, _ctx) = create_test_catalog();
    let label_index = LabelIndex::new();
    let knn_index = KnnIndex::new(crate::index::DEFAULT_VECTORIZER_DIMENSION).unwrap();
    let mut planner = QueryPlanner::new(&catalog, &label_index, &knn_index);

    let query = CypherQuery {
        clauses: vec![
            Clause::Match(MatchClause {
                pattern: Pattern {
                    path_variable: None,
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
                    alias: Some("person".to_string()),
                }],
                distinct: false,
            }),
        ],
        params: std::collections::HashMap::new(),
        graph_scope: None,
    };

    let operators = planner.plan_query(&query).unwrap();
    assert_eq!(operators.len(), 2);

    match &operators[1] {
        Operator::Project { items } => {
            assert_eq!(items.len(), 1);
            assert_eq!(items[0].alias, "person");
        }
        _ => panic!("Expected Project operator with alias"),
    }
}
