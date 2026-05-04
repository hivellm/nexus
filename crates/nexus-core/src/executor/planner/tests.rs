//! Planner test suite. Attached via `#[cfg(test)] mod tests;` in the
//! parent module; super::* pulls in every planner type.

#![allow(unused_imports)]
use super::*;
use crate::catalog::Catalog;
use crate::executor::JoinType;
use crate::executor::parser::{
    BinaryOperator, Clause, CypherParser, CypherQuery, Expression, LimitClause, Literal,
    MatchClause, NodePattern, Pattern, PatternElement, QuantifiedGroup, RelationshipDirection,
    RelationshipPattern, RelationshipQuantifier, ReturnClause, ReturnItem, WhereClause,
};
use crate::executor::planner::queries::{
    qpp_legacy_rewrite_enabled, set_qpp_legacy_rewrite_enabled,
};
use crate::index::{KnnIndex, LabelIndex};
use crate::testing::TestContext;

/// Helper to create a test catalog with guaranteed directory existence
fn create_test_catalog() -> (Catalog, TestContext) {
    let ctx = TestContext::new();
    let catalog = Catalog::with_isolated_path(
        ctx.path().join("catalog.mdb"),
        crate::catalog::CATALOG_MMAP_INITIAL_SIZE,
    )
    .expect("Failed to create catalog");
    (catalog, ctx)
}

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
        Operator::Filter { predicate } => {
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

// ---------------------------------------------------------------
// QPP planner-shape tests — phase6_opencypher-quantified-path-patterns §4.4
// ---------------------------------------------------------------

/// Shared catalog for the QPP planner-shape tests. Spinning up a
/// fresh LMDB env per QPP test was tripping `MDB_TLS_FULL` when the
/// full lib suite ran in parallel — too many envs across too many
/// threads exhausted the LMDB TLS slot pool. Sharing one read-only
/// catalog across all `parse_and_plan` callers stays safe because
/// the planner only reads label/type ids; nothing under test
/// mutates catalog state.
fn shared_qpp_test_catalog() -> std::sync::MutexGuard<'static, Catalog> {
    use std::sync::{Mutex, OnceLock};
    static SHARED: OnceLock<Mutex<Catalog>> = OnceLock::new();
    let mutex = SHARED.get_or_init(|| {
        let ctx = TestContext::new();
        let path = ctx.path().join("qpp_planner_catalog.mdb");
        // Leak the temp-dir guard for the process lifetime so the
        // backing files survive every test run.
        let _leaked = Box::leak(Box::new(ctx));
        let catalog = Catalog::with_isolated_path(path, crate::catalog::CATALOG_MMAP_INITIAL_SIZE)
            .expect("Failed to create shared QPP catalog");
        Mutex::new(catalog)
    });
    mutex.lock().expect("shared QPP catalog poisoned")
}

/// Helper: parse the cypher source through the public parser so the
/// planner sees the same AST a real query would. The hand-built
/// `CypherQuery` literals above are too verbose for QPP shapes.
fn parse_and_plan(cypher: &str) -> Vec<Operator> {
    let catalog = shared_qpp_test_catalog();
    let label_index = LabelIndex::new();
    let knn_index = KnnIndex::new(crate::index::DEFAULT_VECTORIZER_DIMENSION).unwrap();
    let mut planner = QueryPlanner::new(&catalog, &label_index, &knn_index);
    let mut parser = CypherParser::new(cypher.to_string());
    let query = parser
        .parse()
        .unwrap_or_else(|e| panic!("parse `{cypher}`: {e}"));
    planner
        .plan_query(&query)
        .unwrap_or_else(|e| panic!("plan `{cypher}`: {e}"))
}

#[test]
#[serial_test::serial(qpp_legacy_rewrite_flag)]
fn test_plan_qpp_anonymous_body_lowers_to_variable_length_path() {
    // Slice-1: `( ()-[:T]->() ){m,n}` collapses at parse time to
    // a `RelationshipPattern` with quantifier — the planner sees
    // the legacy shape and emits `VariableLengthPath`, never
    // `QuantifiedExpand`.
    let operators = parse_and_plan("MATCH (a)( ()-[:KNOWS]->() ){1,5}(b) RETURN a, b");
    assert!(
        operators
            .iter()
            .any(|op| matches!(op, Operator::VariableLengthPath { .. })),
        "anonymous-body QPP must lower to VariableLengthPath: {operators:?}",
    );
    assert!(
        !operators
            .iter()
            .any(|op| matches!(op, Operator::QuantifiedExpand { .. })),
        "anonymous-body QPP must NOT reach the slice-2 operator: {operators:?}",
    );
}

#[test]
#[serial_test::serial(qpp_legacy_rewrite_flag)]
fn test_plan_qpp_named_inner_node_emits_quantified_expand_with_one_hop() {
    // Slice-2: a named or labelled inner boundary node forces
    // list-promotion semantics, so the lowering bows out and the
    // planner emits `QuantifiedExpand` instead. `hops.len() == 1`
    // because the body is single-relationship.
    let operators =
        parse_and_plan("MATCH (a:Person)( (x:Person)-[:KNOWS]->() ){1,3}(b:Person) RETURN x");
    let qpp = operators
        .iter()
        .find_map(|op| match op {
            Operator::QuantifiedExpand {
                hops, inner_nodes, ..
            } => Some((hops, inner_nodes)),
            _ => None,
        })
        .expect("named-inner QPP must emit QuantifiedExpand");
    assert_eq!(qpp.0.len(), 1, "single-rel body has hops.len() == 1");
    assert_eq!(
        qpp.1.len(),
        2,
        "single-rel body has inner_nodes.len() == hops.len() + 1"
    );
    assert!(
        !operators
            .iter()
            .any(|op| matches!(op, Operator::VariableLengthPath { .. })),
        "named-inner QPP must NOT lower to VariableLengthPath: {operators:?}",
    );
}

#[test]
#[serial_test::serial(qpp_legacy_rewrite_flag)]
fn test_plan_qpp_multi_hop_body_emits_quantified_expand_with_n_hops() {
    // Slice-3a: multi-hop bodies plan as a single
    // `QuantifiedExpand` with `hops.len() == n`. The planner walks
    // every Node-Relationship-Node alternation and bundles them
    // into the operator's `hops` / `inner_nodes` vectors.
    let operators = parse_and_plan(
        "MATCH (a)( (x:Person)-[:KNOWS]->(y:Person)-[:KNOWS]->(z:Person) ){1,3}(b) \
         RETURN x, y, z",
    );
    let qpp = operators
        .iter()
        .find_map(|op| match op {
            Operator::QuantifiedExpand {
                hops, inner_nodes, ..
            } => Some((hops, inner_nodes)),
            _ => None,
        })
        .expect("multi-hop QPP must emit QuantifiedExpand");
    assert_eq!(qpp.0.len(), 2, "two-rel body has hops.len() == 2");
    assert_eq!(
        qpp.1.len(),
        3,
        "two-rel body has inner_nodes.len() == hops.len() + 1"
    );
}

#[test]
#[serial_test::serial(qpp_legacy_rewrite_flag)]
fn test_plan_legacy_var_length_rewrite_to_qpp_is_opt_in() {
    // Slice-3b §6.5 — the legacy `*m..n` planner branch can be
    // rewritten to emit `QuantifiedExpand` instead of
    // `VariableLengthPath`. The flag is process-wide
    // (`set_qpp_legacy_rewrite_enabled` / `qpp_legacy_rewrite_enabled`)
    // and starts disabled unless `NEXUS_QPP_REWRITE_LEGACY=1` was
    // set at startup. This test pins both halves of the contract.
    //
    // The shared QPP catalog mutex (`shared_qpp_test_catalog`)
    // serialises every `parse_and_plan` call across the planner
    // test module, so flipping the static flag here is safe even
    // under `cargo test` parallel execution: the lock is held for
    // the entire `parse_and_plan` body, and nobody else reads the
    // flag outside that body. We restore the flag on exit so a
    // failure mid-test doesn't leak state.
    let previous = qpp_legacy_rewrite_enabled();

    // Default branch: legacy operator stays on.
    set_qpp_legacy_rewrite_enabled(false);
    let default_ops = parse_and_plan("MATCH (a:Person)-[:KNOWS*1..3]->(b:Person) RETURN b");
    assert!(
        default_ops
            .iter()
            .any(|op| matches!(op, Operator::VariableLengthPath { .. })),
        "default planner must keep emitting VariableLengthPath: {default_ops:?}"
    );
    assert!(
        !default_ops
            .iter()
            .any(|op| matches!(op, Operator::QuantifiedExpand { .. })),
        "default planner must NOT emit QuantifiedExpand for legacy *m..n: {default_ops:?}"
    );

    // Opt-in branch: flag flipped on.
    set_qpp_legacy_rewrite_enabled(true);
    let rewrite_ops = parse_and_plan("MATCH (a:Person)-[:KNOWS*1..3]->(b:Person) RETURN b");
    assert!(
        rewrite_ops
            .iter()
            .any(|op| matches!(op, Operator::QuantifiedExpand { .. })),
        "with the rewrite flag set, the planner must emit QuantifiedExpand: {rewrite_ops:?}"
    );
    assert!(
        !rewrite_ops
            .iter()
            .any(|op| matches!(op, Operator::VariableLengthPath { .. })),
        "with rewrite on, VariableLengthPath must not appear: {rewrite_ops:?}"
    );

    set_qpp_legacy_rewrite_enabled(previous);
}

#[test]
#[serial_test::serial(qpp_legacy_rewrite_flag)]
fn test_plan_qpp_starting_node_uses_label_scan_upstream() {
    // Slice-3b §4.2: the QPP operator does not pick its own
    // source — it consumes whatever upstream operator the
    // surrounding pattern emits. When the source pattern carries
    // a label, the planner must emit `NodeByLabel` (or
    // `IndexScan`, gated on index availability) *before*
    // `QuantifiedExpand` so the source-side rows are already
    // narrowed by the time the expansion runs.
    let operators =
        parse_and_plan("MATCH (a:Person)( (x:Person)-[:KNOWS]->() ){1,3}(b:Person) RETURN x");
    let label_scan_idx = operators
        .iter()
        .position(|op| matches!(op, Operator::NodeByLabel { .. }))
        .expect("source-side label scan must be planned");
    let qpp_idx = operators
        .iter()
        .position(|op| matches!(op, Operator::QuantifiedExpand { .. }))
        .expect("QPP must emit QuantifiedExpand");
    assert!(
        label_scan_idx < qpp_idx,
        "source-side NodeByLabel must precede QuantifiedExpand: {operators:?}",
    );
}

#[test]
#[serial_test::serial(qpp_legacy_rewrite_flag)]
fn test_plan_qpp_named_body_target_var_chains_to_following_node() {
    // The QPP planner threads `prev_node_var` so a follow-up
    // pattern element after the QPP gets the right source. When
    // `(b)` follows the QPP without the operator binding the
    // target to `b`, downstream Expands break. Pin the contract:
    // a named trailing boundary node must end up as the operator's
    // `target_var`.
    let operators =
        parse_and_plan("MATCH (a:Person)( (x:Person)-[:KNOWS]->() ){1,3}(b:Person) RETURN b");
    let target_var = operators
        .iter()
        .find_map(|op| match op {
            Operator::QuantifiedExpand { target_var, .. } => Some(target_var),
            _ => None,
        })
        .expect("named-inner QPP must emit QuantifiedExpand");
    assert_eq!(
        target_var, "b",
        "trailing named boundary node must wire to the operator's target_var"
    );
}

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

/// Check if aggregation can be optimized with streaming
pub fn can_use_streaming_aggregation(operators: &[Operator]) -> bool {
    // Check if we have aggregation operations that can benefit from streaming
    for operator in operators {
        if let Operator::Aggregate {
            group_by,
            aggregations,
            ..
        } = operator
        {
            // Streaming is beneficial when:
            // 1. We have aggregations that can be computed incrementally
            // 2. Group-by keys are not too numerous (to avoid memory explosion)
            // 3. We don't have complex expressions in aggregations

            if aggregations.len() > 10 {
                return false; // Too many aggregations, stick with in-memory
            }

            // Check aggregation types - streaming works best with COUNT, SUM, AVG
            for agg in aggregations {
                match agg {
                    Aggregation::Count { .. }
                    | Aggregation::Sum { .. }
                    | Aggregation::Avg { .. } => {
                        // These can be streamed
                    }
                    Aggregation::Min { .. } | Aggregation::Max { .. } => {
                        // These can also be streamed
                    }
                    Aggregation::Collect { .. } => {
                        // Collect requires storing all values, not suitable for streaming
                        return false;
                    }
                    Aggregation::CountStarOptimized { .. } => {
                        // Optimized count is already efficient
                    }
                    _ => {
                        // Other aggregations may not be suitable for streaming
                        return false;
                    }
                }
            }

            // Check group-by complexity
            if group_by.len() > 3 {
                return false; // Too many group-by keys for streaming
            }

            return true;
        }
    }
    false
}

/// Optimize aggregation operations by pushing them down in the query plan
pub fn optimize_aggregations(operators: Vec<Operator>) -> Result<Vec<Operator>> {
    let mut result = Vec::new();

    for operator in operators {
        match operator {
            Operator::Aggregate {
                ref aggregations,
                ref group_by,
                ref source,
                ..
            } => {
                // Check if we can push aggregation down to reduce data volume earlier
                if let Some(source_op) = source.as_ref() {
                    // Convert group_by from Vec<String> to Vec<Expression> for the check
                    // For now, we'll just check if we can push down (simplified)
                    let can_push = match source_op.as_ref() {
                        Operator::Filter { .. } | Operator::Project { .. } => true,
                        _ => false,
                    };
                    if can_push {
                        // Create a new aggregation operator with push-down optimization
                        let optimized_agg = Operator::Aggregate {
                            aggregations: aggregations.clone(),
                            group_by: group_by.clone(),
                            projection_items: None,
                            source: source.clone(),
                            streaming_optimized: false,
                            push_down_optimized: true,
                        };
                        result.push(optimized_agg);
                        continue;
                    }
                }

                // Use streaming aggregation if beneficial
                if can_use_streaming_aggregation(&[operator.clone()]) {
                    let streaming_agg = Operator::Aggregate {
                        aggregations: aggregations.clone(),
                        group_by: group_by.clone(),
                        projection_items: None,
                        source: source.clone(),
                        streaming_optimized: true,
                        push_down_optimized: false,
                    };
                    result.push(streaming_agg);
                    continue;
                }

                // Default aggregation
                result.push(operator);
            }
            _ => result.push(operator),
        }
    }

    Ok(result)
}

/// Check if aggregation can be pushed down to reduce data processing
fn can_push_aggregation_down(
    source_op: &Operator,
    aggregations: &[Aggregation],
    group_by: &[Expression],
) -> bool {
    match source_op {
        Operator::Filter { .. } => {
            // We can push aggregation past filters
            // Filter doesn't have a source field, so we can push down
            return true;
        }
        Operator::Project { .. } => {
            // Check if projection includes all needed columns for aggregation
            // Project doesn't have a source field, so we can push down
            return true;
        }
        Operator::Expand { .. } => {
            // Relationship expansions can sometimes be optimized with aggregation
            // For now, be conservative and don't push down
            return false;
        }
        _ => {
            // Other operators - check if they produce data we need for aggregation
            return source_supports_aggregation(source_op, aggregations, group_by);
        }
    }
}

/// Check if a source operator supports aggregation optimization
fn source_supports_aggregation(
    source_op: &Operator,
    _aggregations: &[Aggregation],
    _group_by: &[Expression],
) -> bool {
    match source_op {
        Operator::NodeByLabel { .. }
        | Operator::AllNodesScan { .. }
        | Operator::IndexScan { .. } => {
            // These are good sources for aggregation - they produce nodes we can aggregate
            true
        }
        Operator::Expand { .. } => {
            // Relationship traversal results can be aggregated
            true
        }
        _ => false,
    }
}

/// Create optimized COUNT operations
pub fn optimize_count_operations(operators: Vec<Operator>) -> Result<Vec<Operator>> {
    let mut result = Vec::new();

    for operator in operators {
        match operator {
            Operator::Aggregate {
                aggregations,
                group_by,
                source,
                ..
            } => {
                let mut optimized_aggregations = Vec::new();

                for agg in aggregations {
                    match agg {
                        Aggregation::Count { column: None, .. } => {
                            // Optimize COUNT(*) operations
                            if can_optimize_count_star(&source) {
                                optimized_aggregations.push(Aggregation::CountStarOptimized {
                                    alias: "count".to_string(), // Default alias
                                });
                            } else {
                                optimized_aggregations.push(agg);
                            }
                        }
                        _ => optimized_aggregations.push(agg),
                    }
                }

                result.push(Operator::Aggregate {
                    aggregations: optimized_aggregations,
                    group_by,
                    projection_items: None,
                    source,
                    streaming_optimized: false,
                    push_down_optimized: false,
                });
            }
            _ => result.push(operator),
        }
    }

    Ok(result)
}

/// Check if COUNT(*) can be optimized (e.g., using index statistics)
fn can_optimize_count_star(source: &Option<Box<Operator>>) -> bool {
    if let Some(source_op) = source {
        match source_op.as_ref() {
            Operator::NodeByLabel { label_id, .. } => {
                // We can potentially use label index statistics for COUNT(*)
                // This would require label index to track counts per label
                let _ = label_id; // We'll use this in the future
                false // For now, not implemented
            }
            Operator::AllNodesScan { .. } => {
                // For all nodes, we could potentially use total node count
                false // For now, not implemented
            }
            _ => false,
        }
    } else {
        false
    }
}

// ───────────────────────────────────────────────────────────────────
// phase7_planner-using-index-hints — USING INDEX validation tests
// ───────────────────────────────────────────────────────────────────

/// Helper: parse + plan with an explicit `PropertyIndex` handle.
fn plan_with_property_index(
    cypher: &str,
    catalog: &Catalog,
    prop_idx: &crate::index::PropertyIndex,
) -> std::result::Result<Vec<Operator>, crate::Error> {
    let label_index = LabelIndex::new();
    let knn_index = KnnIndex::new(crate::index::DEFAULT_VECTORIZER_DIMENSION).unwrap();
    let mut planner =
        QueryPlanner::new(catalog, &label_index, &knn_index).with_property_index(prop_idx);
    let mut parser = CypherParser::new(cypher.to_string());
    let query = parser.parse()?;
    planner.plan_query(&query)
}

#[test]
fn using_index_hint_accepted_silently_without_property_index_handle() {
    // No `with_property_index` call → planner returns Ok(...) without
    // validating the hint, matching legacy behaviour for unit-test
    // callers that don't carry an `IndexManager` handle.
    let (catalog, _ctx) = create_test_catalog();
    let label_index = LabelIndex::new();
    let knn_index = KnnIndex::new(crate::index::DEFAULT_VECTORIZER_DIMENSION).unwrap();
    let mut planner = QueryPlanner::new(&catalog, &label_index, &knn_index);
    let mut parser = CypherParser::new(
        "MATCH (n:Person) USING INDEX n:Person(email) WHERE n.email = 'a@b' RETURN n".to_string(),
    );
    let query = parser.parse().expect("parse");
    let result = planner.plan_query(&query);
    assert!(
        result.is_ok(),
        "USING INDEX hint should be accepted silently when no PropertyIndex handle is installed; got {result:?}"
    );
}

#[test]
fn using_index_hint_validated_when_property_index_handle_installed_and_index_exists() {
    // With a `PropertyIndex` handle and a registered index for
    // (Person, email), the hint passes validation.
    let (catalog, _ctx) = create_test_catalog();
    let label_id = catalog.get_or_create_label("Person").expect("label");
    let key_id = catalog.get_or_create_key("email").expect("key");

    let prop_idx = crate::index::PropertyIndex::new();
    prop_idx
        .create_index(label_id, key_id)
        .expect("create index");

    let result = plan_with_property_index(
        "MATCH (n:Person) USING INDEX n:Person(email) WHERE n.email = 'a@b' RETURN n",
        &catalog,
        &prop_idx,
    );
    assert!(
        result.is_ok(),
        "USING INDEX hint should pass validation when a matching property index exists; got {result:?}"
    );
}

#[test]
fn using_index_hint_errors_when_index_missing() {
    // Property index handle installed but no matching index for
    // (Person, email) → planner emits ERR_USING_INDEX_NOT_FOUND.
    let (catalog, _ctx) = create_test_catalog();
    catalog.get_or_create_label("Person").expect("label");
    catalog.get_or_create_key("email").expect("key");

    let prop_idx = crate::index::PropertyIndex::new();
    // Deliberately no `create_index` call — the registry is empty.

    let result = plan_with_property_index(
        "MATCH (n:Person) USING INDEX n:Person(email) WHERE n.email = 'a@b' RETURN n",
        &catalog,
        &prop_idx,
    );
    let err = result.expect_err("missing-index hint must error");
    let msg = err.to_string();
    assert!(
        msg.contains("ERR_USING_INDEX_NOT_FOUND"),
        "expected ERR_USING_INDEX_NOT_FOUND, got: {msg}"
    );
    assert!(
        msg.contains(":Person(email)"),
        "error message should name the (label, property) pair: {msg}"
    );
}

#[test]
fn using_index_hint_errors_when_label_missing_in_catalog() {
    // Hint references a label that was never registered in the
    // catalog. Planner short-circuits before consulting the index.
    let (catalog, _ctx) = create_test_catalog();
    let prop_idx = crate::index::PropertyIndex::new();

    let result = plan_with_property_index(
        "MATCH (n:Ghost) USING INDEX n:Ghost(id) WHERE n.id = 1 RETURN n",
        &catalog,
        &prop_idx,
    );
    let err = result.expect_err("hint on unknown label must error");
    let msg = err.to_string();
    assert!(
        msg.contains("ERR_USING_INDEX_NOT_FOUND"),
        "expected ERR_USING_INDEX_NOT_FOUND, got: {msg}"
    );
    assert!(
        msg.contains("Ghost"),
        "error message should name the unknown label: {msg}"
    );
}

// ───────────────────────────────────────────────────────────────────
// phase6_merge-unindexed-property-warning — `Nexus.Performance.
// UnindexedPropertyAccess` notification emission.
// ───────────────────────────────────────────────────────────────────

/// Helper: parse + plan with an explicit `PropertyIndex` handle and
/// return both the operators and the planner's drained notifications.
/// Mirrors `plan_with_property_index` but exposes the diagnostic side
/// channel so tests can assert on hint emission.
fn plan_with_notifications(
    cypher: &str,
    catalog: &Catalog,
    prop_idx: &crate::index::PropertyIndex,
) -> (Vec<Operator>, Vec<crate::executor::types::Notification>) {
    let label_index = LabelIndex::new();
    let knn_index = KnnIndex::new(crate::index::DEFAULT_VECTORIZER_DIMENSION).unwrap();
    let mut planner =
        QueryPlanner::new(catalog, &label_index, &knn_index).with_property_index(prop_idx);
    let mut parser = CypherParser::new(cypher.to_string());
    let query = parser.parse().expect("parse");
    let ops = planner.plan_query(&query).expect("plan");
    let notes = planner.take_notifications();
    (ops, notes)
}

#[test]
fn unindexed_property_notification_emitted_for_merge_inline_selector() {
    // `MERGE (n:Artifact { natural_key: $v })` with no index on
    // (Artifact, natural_key) → planner emits one
    // `Nexus.Performance.UnindexedPropertyAccess` notification.
    let (catalog, _ctx) = create_test_catalog();
    catalog.get_or_create_label("Artifact").expect("label");
    catalog.get_or_create_key("natural_key").expect("key");
    let prop_idx = crate::index::PropertyIndex::new();

    let (_ops, notes) = plan_with_notifications(
        "MERGE (n:Artifact { natural_key: 'sha256:abc' }) RETURN count(n) AS c",
        &catalog,
        &prop_idx,
    );

    assert_eq!(
        notes.len(),
        1,
        "expected exactly one notification, got: {notes:?}"
    );
    let n = &notes[0];
    assert_eq!(n.code, "Nexus.Performance.UnindexedPropertyAccess");
    assert!(
        n.title.contains("Artifact"),
        "title should name the label: {}",
        n.title
    );
    assert!(
        n.title.contains("natural_key"),
        "title should name the property: {}",
        n.title
    );
    assert!(
        n.description.contains("MERGE"),
        "description should name the offending clause: {}",
        n.description
    );
    assert!(
        n.description
            .contains("CREATE INDEX FOR (n:Artifact) ON (n.natural_key)"),
        "description should include the suggested DDL verbatim: {}",
        n.description
    );
}

#[test]
fn unindexed_property_notification_emitted_for_match_inline_selector() {
    // Same as the MERGE case but the offending clause is MATCH; the
    // emitter must label the clause correctly.
    let (catalog, _ctx) = create_test_catalog();
    catalog.get_or_create_label("Artifact").expect("label");
    catalog.get_or_create_key("path").expect("key");
    let prop_idx = crate::index::PropertyIndex::new();

    let (_ops, notes) = plan_with_notifications(
        "MATCH (a:Artifact { path: '/etc/x' }) RETURN a",
        &catalog,
        &prop_idx,
    );

    assert_eq!(notes.len(), 1, "expected one notification, got: {notes:?}");
    assert!(
        notes[0].description.contains("MATCH"),
        "description should name MATCH: {}",
        notes[0].description
    );
}

#[test]
fn unindexed_property_notification_emitted_for_where_equality() {
    // `MATCH (n:Label) WHERE n.prop = $v` form — the WHERE walker
    // resolves `n` to its label via the variable-binding map and
    // emits the notification when no index covers (Label, prop).
    let (catalog, _ctx) = create_test_catalog();
    catalog.get_or_create_label("Artifact").expect("label");
    catalog.get_or_create_key("natural_key").expect("key");
    let prop_idx = crate::index::PropertyIndex::new();

    let (_ops, notes) = plan_with_notifications(
        "MATCH (n:Artifact) WHERE n.natural_key = 'x' RETURN n",
        &catalog,
        &prop_idx,
    );

    assert!(
        !notes.is_empty(),
        "WHERE equality on unindexed property should emit a notification"
    );
    assert_eq!(notes[0].code, "Nexus.Performance.UnindexedPropertyAccess");
    assert!(
        notes[0].title.contains("natural_key"),
        "title should name the property"
    );
}

#[test]
fn unindexed_property_notification_suppressed_when_index_exists() {
    // With a registered index for (Artifact, natural_key), the
    // planner must NOT emit the notification — false positives would
    // teach operators to ignore the hint.
    let (catalog, _ctx) = create_test_catalog();
    let label_id = catalog.get_or_create_label("Artifact").expect("label");
    let key_id = catalog.get_or_create_key("natural_key").expect("key");
    let prop_idx = crate::index::PropertyIndex::new();
    prop_idx
        .create_index(label_id, key_id)
        .expect("create index");

    let (_ops, notes) = plan_with_notifications(
        "MERGE (n:Artifact { natural_key: 'x' }) RETURN n",
        &catalog,
        &prop_idx,
    );

    assert!(
        notes.is_empty(),
        "no notification expected when the index exists, got: {notes:?}"
    );
}

#[test]
fn unindexed_property_notification_deduplicated_within_single_plan() {
    // Two clauses referencing the same (label, prop) pair — the
    // planner emits ONE notification, not two, so a query with
    // both MATCH and MERGE on the same selector is not noisy.
    let (catalog, _ctx) = create_test_catalog();
    catalog.get_or_create_label("Artifact").expect("label");
    catalog.get_or_create_key("natural_key").expect("key");
    let prop_idx = crate::index::PropertyIndex::new();

    let (_ops, notes) = plan_with_notifications(
        "MATCH (a:Artifact { natural_key: 'x' }) WITH a \
         MERGE (b:Artifact { natural_key: 'x' }) RETURN a, b",
        &catalog,
        &prop_idx,
    );

    assert_eq!(
        notes.len(),
        1,
        "duplicate (label, prop) selector should emit one notification, got: {notes:?}"
    );
}

#[test]
fn unindexed_property_notification_no_op_without_property_index_handle() {
    // Planners constructed without `with_property_index(...)` — the
    // standalone `Executor::parse_and_plan` path and existing planner
    // unit tests — must not emit notifications. Removing the catalog
    // handle entirely avoids surprises in callers that haven't opted
    // into diagnostics.
    let (catalog, _ctx) = create_test_catalog();
    catalog.get_or_create_label("Artifact").expect("label");
    catalog.get_or_create_key("natural_key").expect("key");
    let label_index = LabelIndex::new();
    let knn_index = KnnIndex::new(crate::index::DEFAULT_VECTORIZER_DIMENSION).unwrap();
    let mut planner = QueryPlanner::new(&catalog, &label_index, &knn_index);
    let mut parser =
        CypherParser::new("MERGE (n:Artifact { natural_key: 'x' }) RETURN n".to_string());
    let query = parser.parse().expect("parse");
    let _ops = planner.plan_query(&query).expect("plan");
    let notes = planner.take_notifications();
    assert!(
        notes.is_empty(),
        "no notifications expected, got: {notes:?}"
    );
}
