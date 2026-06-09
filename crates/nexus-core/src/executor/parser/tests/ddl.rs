//! Tests for DDL and advanced Cypher 25 syntax: dynamic labels, index
//! and constraint DDL, savepoints, graph scoping, CALL IN TRANSACTIONS,
//! and USING RTREE index alias.

use super::*;

// phase6_opencypher-advanced-types §2 — write-side dynamic labels
#[test]
fn parse_dynamic_label_in_create() {
    let mut parser = CypherParser::new("CREATE (n:$label {k: 1})".to_string());
    let q = parser.parse().expect("CREATE with :$param must parse");
    match &q.clauses[0] {
        Clause::Create(c) => match &c.pattern.elements[0] {
            PatternElement::Node(n) => {
                assert_eq!(n.labels, vec!["$label".to_string()]);
            }
            other => panic!("expected node pattern, got {other:?}"),
        },
        other => panic!("expected CREATE, got {other:?}"),
    }
}

#[test]
fn parse_dynamic_label_mixed_with_static() {
    let mut parser = CypherParser::new("CREATE (n:Base:$role)".to_string());
    let q = parser
        .parse()
        .expect("mixed static+dynamic labels must parse");
    if let Clause::Create(c) = &q.clauses[0] {
        if let PatternElement::Node(n) = &c.pattern.elements[0] {
            assert_eq!(n.labels, vec!["Base".to_string(), "$role".to_string()]);
            return;
        }
    }
    panic!("expected CREATE node pattern");
}

#[test]
fn parse_set_dynamic_label() {
    let mut parser = CypherParser::new("MATCH (n) SET n:$role".to_string());
    let q = parser.parse().expect("SET n:$param must parse");
    let set = q
        .clauses
        .iter()
        .find_map(|c| {
            if let Clause::Set(s) = c {
                Some(s)
            } else {
                None
            }
        })
        .expect("SET clause");
    match &set.items[0] {
        SetItem::Label { label, .. } => assert_eq!(label, "$role"),
        other => panic!("expected SET label, got {other:?}"),
    }
}

#[test]
fn parse_remove_dynamic_label() {
    let mut parser = CypherParser::new("MATCH (n) REMOVE n:$role".to_string());
    let q = parser.parse().expect("REMOVE n:$param must parse");
    let rem = q
        .clauses
        .iter()
        .find_map(|c| {
            if let Clause::Remove(r) = c {
                Some(r)
            } else {
                None
            }
        })
        .expect("REMOVE clause");
    match &rem.items[0] {
        RemoveItem::Label { label, .. } => assert_eq!(label, "$role"),
        other => panic!("expected REMOVE label, got {other:?}"),
    }
}

// phase6_opencypher-advanced-types §3 — composite index DDL
#[test]
fn parse_composite_index_modern_form() {
    let mut parser = CypherParser::new(
        "CREATE INDEX person_id FOR (p:Person) ON (p.tenantId, p.id)".to_string(),
    );
    let q = parser.parse().expect("composite index DDL must parse");
    match &q.clauses[0] {
        Clause::CreateIndex(ix) => {
            assert_eq!(ix.name.as_deref(), Some("person_id"));
            assert_eq!(ix.label, "Person");
            assert_eq!(
                ix.properties,
                vec!["tenantId".to_string(), "id".to_string()]
            );
        }
        other => panic!("expected CREATE INDEX, got {other:?}"),
    }
}

#[test]
fn parse_legacy_single_property_index_still_works() {
    let mut parser = CypherParser::new("CREATE INDEX ON :Person(email)".to_string());
    let q = parser
        .parse()
        .expect("legacy single-property index must still parse");
    match &q.clauses[0] {
        Clause::CreateIndex(ix) => {
            assert_eq!(ix.label, "Person");
            assert_eq!(ix.properties, vec!["email".to_string()]);
            assert_eq!(ix.property, "email");
            assert_eq!(ix.name, None);
        }
        other => panic!("expected CREATE INDEX, got {other:?}"),
    }
}

#[test]
fn parse_composite_index_rejects_mismatched_variable() {
    let mut parser =
        CypherParser::new("CREATE INDEX FOR (p:Person) ON (q.tenantId, p.id)".to_string());
    assert!(
        parser.parse().is_err(),
        "prefix variable must match pattern variable"
    );
}

// phase6_opencypher-advanced-types §5 — savepoints
#[test]
fn parse_savepoint_statement() {
    let mut parser = CypherParser::new("SAVEPOINT my_sp".to_string());
    let q = parser.parse().unwrap();
    match &q.clauses[0] {
        Clause::Savepoint(s) => assert_eq!(s.name, "my_sp"),
        other => panic!("expected Savepoint, got {other:?}"),
    }
}

#[test]
fn parse_rollback_to_savepoint() {
    let mut parser = CypherParser::new("ROLLBACK TO SAVEPOINT my_sp".to_string());
    let q = parser.parse().unwrap();
    match &q.clauses[0] {
        Clause::RollbackToSavepoint(s) => assert_eq!(s.name, "my_sp"),
        other => panic!("expected RollbackToSavepoint, got {other:?}"),
    }
}

#[test]
fn parse_release_savepoint() {
    let mut parser = CypherParser::new("RELEASE SAVEPOINT my_sp".to_string());
    let q = parser.parse().unwrap();
    match &q.clauses[0] {
        Clause::ReleaseSavepoint(s) => assert_eq!(s.name, "my_sp"),
        other => panic!("expected ReleaseSavepoint, got {other:?}"),
    }
}

#[test]
fn bare_rollback_still_parses_as_transaction_rollback() {
    let mut parser = CypherParser::new("ROLLBACK".to_string());
    let q = parser.parse().unwrap();
    assert!(matches!(q.clauses[0], Clause::RollbackTransaction));
}

// phase6_opencypher-advanced-types §6 — graph scoping
#[test]
fn parse_graph_scope_preamble() {
    let mut parser = CypherParser::new("GRAPH[analytics] MATCH (n:Person) RETURN n".to_string());
    let q = parser.parse().expect("GRAPH[name] preamble must parse");
    assert_eq!(q.graph_scope.as_deref(), Some("analytics"));
    assert_eq!(q.clauses.len(), 2);
}

#[test]
fn query_without_graph_scope_has_none() {
    let mut parser = CypherParser::new("MATCH (n) RETURN n".to_string());
    let q = parser.parse().unwrap();
    assert_eq!(q.graph_scope, None);
}

// ───────── phase6_opencypher-constraint-enforcement — Cypher 25 DDL ─────────

#[test]
fn parse_cypher25_node_key_constraint() {
    let mut parser = CypherParser::new(
        "CREATE CONSTRAINT person_key FOR (p:Person) REQUIRE (p.tenantId, p.id) IS NODE KEY"
            .to_string(),
    );
    let q = parser.parse().expect("NODE KEY DDL must parse");
    match &q.clauses[0] {
        Clause::CreateConstraint(c) => {
            assert_eq!(c.name.as_deref(), Some("person_key"));
            assert_eq!(c.constraint_type, ConstraintType::NodeKey);
            assert_eq!(c.label, "Person");
            assert_eq!(c.properties, vec!["tenantId".to_string(), "id".to_string()]);
            assert_eq!(c.entity, ConstraintEntity::Node);
        }
        other => panic!("expected CREATE CONSTRAINT, got {other:?}"),
    }
}

#[test]
fn parse_cypher25_not_null_constraint() {
    let mut parser = CypherParser::new(
        "CREATE CONSTRAINT FOR (p:Person) REQUIRE p.email IS NOT NULL".to_string(),
    );
    let q = parser.parse().expect("IS NOT NULL DDL must parse");
    match &q.clauses[0] {
        Clause::CreateConstraint(c) => {
            assert_eq!(c.constraint_type, ConstraintType::Exists);
            assert_eq!(c.label, "Person");
            assert_eq!(c.property, "email");
            assert_eq!(c.entity, ConstraintEntity::Node);
        }
        other => panic!("expected CREATE CONSTRAINT, got {other:?}"),
    }
}

#[test]
fn parse_cypher25_property_type_constraint() {
    let mut parser = CypherParser::new(
        "CREATE CONSTRAINT FOR (p:Person) REQUIRE p.age IS :: INTEGER".to_string(),
    );
    let q = parser.parse().expect("IS :: TYPE DDL must parse");
    match &q.clauses[0] {
        Clause::CreateConstraint(c) => {
            assert_eq!(c.constraint_type, ConstraintType::PropertyType);
            assert_eq!(c.property, "age");
            assert_eq!(c.property_type.as_deref(), Some("INTEGER"));
            assert_eq!(c.entity, ConstraintEntity::Node);
        }
        other => panic!("expected CREATE CONSTRAINT, got {other:?}"),
    }
}

#[test]
fn parse_cypher25_rel_not_null_constraint() {
    let mut parser = CypherParser::new(
        "CREATE CONSTRAINT FOR ()-[r:CONNECTS]-() REQUIRE r.weight IS NOT NULL".to_string(),
    );
    let q = parser.parse().expect("rel NOT NULL DDL must parse");
    match &q.clauses[0] {
        Clause::CreateConstraint(c) => {
            assert_eq!(c.constraint_type, ConstraintType::Exists);
            assert_eq!(c.label, "CONNECTS");
            assert_eq!(c.property, "weight");
            assert_eq!(c.entity, ConstraintEntity::Relationship);
        }
        other => panic!("expected CREATE CONSTRAINT, got {other:?}"),
    }
}

#[test]
fn parse_cypher25_rel_property_type_directed() {
    let mut parser = CypherParser::new(
        "CREATE CONSTRAINT FOR ()-[r:CONNECTS]->() REQUIRE r.weight IS :: FLOAT".to_string(),
    );
    let q = parser.parse().expect("directed rel DDL must parse");
    match &q.clauses[0] {
        Clause::CreateConstraint(c) => {
            assert_eq!(c.constraint_type, ConstraintType::PropertyType);
            assert_eq!(c.entity, ConstraintEntity::Relationship);
            assert_eq!(c.property_type.as_deref(), Some("FLOAT"));
        }
        other => panic!("expected CREATE CONSTRAINT, got {other:?}"),
    }
}

#[test]
fn parse_legacy_constraint_still_accepted() {
    let mut parser =
        CypherParser::new("CREATE CONSTRAINT ON (p:Person) ASSERT p.email IS UNIQUE".to_string());
    let q = parser.parse().expect("legacy form must still parse");
    match &q.clauses[0] {
        Clause::CreateConstraint(c) => {
            assert_eq!(c.constraint_type, ConstraintType::Unique);
            assert_eq!(c.label, "Person");
            assert_eq!(c.property, "email");
            assert_eq!(c.entity, ConstraintEntity::Node);
        }
        other => panic!("expected CREATE CONSTRAINT, got {other:?}"),
    }
}

// ---------------------------------------------------------------
// CALL { } IN TRANSACTIONS (Cypher 25) — parser-level tests
// phase6_opencypher-subquery-transactions §1 + §2
// ---------------------------------------------------------------

#[test]
fn call_in_transactions_parses_bare() {
    let mut p = CypherParser::new("CALL { CREATE (:Tmp) } IN TRANSACTIONS".to_string());
    let q = p.parse().unwrap();
    let c = call_tx_clause(&q);
    assert!(c.in_transactions);
    assert_eq!(c.batch_size, None);
    assert_eq!(c.concurrency, None);
    assert!(matches!(c.on_error, OnErrorPolicy::Fail));
    assert_eq!(c.status_var, None);
}

#[test]
fn call_in_transactions_parses_batch_size() {
    let mut p = CypherParser::new("CALL { CREATE (:Tmp) } IN TRANSACTIONS OF 500 ROWS".to_string());
    let q = p.parse().unwrap();
    let c = call_tx_clause(&q);
    assert!(c.in_transactions);
    assert_eq!(c.batch_size, Some(500));
}

#[test]
fn call_in_transactions_accepts_singular_row() {
    let mut p = CypherParser::new("CALL { CREATE (:Tmp) } IN TRANSACTIONS OF 1 ROW".to_string());
    let q = p.parse().unwrap();
    assert_eq!(call_tx_clause(&q).batch_size, Some(1));
}

#[test]
fn call_in_transactions_parses_concurrent() {
    let mut p = CypherParser::new(
        "CALL { CREATE (:Tmp) } IN CONCURRENT TRANSACTIONS OF 100 ROWS".to_string(),
    );
    let q = p.parse().unwrap();
    let c = call_tx_clause(&q);
    assert!(c.in_transactions);
    // `Some(0)` is the parser sentinel meaning "concurrent flag set,
    // executor resolves the worker count against
    // `ExecutorConfig::cypher_concurrency`" (default 4). The serial
    // variant remains `None`. Phase6 §6.1 + §6.2.
    assert_eq!(c.concurrency, Some(0));
    assert_eq!(c.batch_size, Some(100));
}

#[test]
fn call_in_transactions_parses_every_on_error_form() {
    for (cypher, expected) in [
        (
            "CALL { CREATE (:T) } IN TRANSACTIONS ON ERROR CONTINUE",
            OnErrorPolicy::Continue,
        ),
        (
            "CALL { CREATE (:T) } IN TRANSACTIONS ON ERROR BREAK",
            OnErrorPolicy::Break,
        ),
        (
            "CALL { CREATE (:T) } IN TRANSACTIONS ON ERROR FAIL",
            OnErrorPolicy::Fail,
        ),
        (
            "CALL { CREATE (:T) } IN TRANSACTIONS ON ERROR RETRY 3",
            OnErrorPolicy::Retry { max_attempts: 3 },
        ),
    ] {
        let mut p = CypherParser::new(cypher.to_string());
        let q = p
            .parse()
            .unwrap_or_else(|e| panic!("failed parsing `{cypher}`: {e}"));
        assert_eq!(call_tx_clause(&q).on_error, expected, "for `{cypher}`");
    }
}

#[test]
fn call_in_transactions_parses_report_status() {
    let mut p = CypherParser::new(
        "CALL { CREATE (:T) } IN TRANSACTIONS OF 100 ROWS REPORT STATUS AS status".to_string(),
    );
    let q = p.parse().unwrap();
    let c = call_tx_clause(&q);
    assert_eq!(c.batch_size, Some(100));
    assert_eq!(c.status_var, Some("status".to_string()));
}

#[test]
fn call_in_transactions_suffix_clauses_are_order_agnostic() {
    // REPORT STATUS first, then OF, then ON ERROR — all parse.
    let mut p = CypherParser::new(
        "CALL { CREATE (:T) } IN TRANSACTIONS REPORT STATUS AS s OF 50 ROWS \
         ON ERROR RETRY 5"
            .to_string(),
    );
    let q = p.parse().unwrap();
    let c = call_tx_clause(&q);
    assert_eq!(c.batch_size, Some(50));
    assert_eq!(c.status_var, Some("s".to_string()));
    assert_eq!(c.on_error, OnErrorPolicy::Retry { max_attempts: 5 });
}

#[test]
fn call_in_transactions_rejects_zero_batch() {
    let mut p = CypherParser::new("CALL { CREATE (:T) } IN TRANSACTIONS OF 0 ROWS".to_string());
    let err = p.parse().unwrap_err();
    assert!(
        err.to_string().contains("ERR_CALL_IN_TX_INVALID_BATCH"),
        "got: {err}"
    );
}

#[test]
fn call_in_transactions_rejects_bad_retry() {
    let mut p =
        CypherParser::new("CALL { CREATE (:T) } IN TRANSACTIONS ON ERROR RETRY 0".to_string());
    let err = p.parse().unwrap_err();
    assert!(
        err.to_string().contains("ERR_CALL_IN_TX_INVALID_RETRY"),
        "got: {err}"
    );
}

#[test]
fn call_in_transactions_rejects_unknown_on_error() {
    let mut p =
        CypherParser::new("CALL { CREATE (:T) } IN TRANSACTIONS ON ERROR IGNORE".to_string());
    let err = p.parse().unwrap_err();
    assert!(
        err.to_string().contains("ERR_CALL_IN_TX_UNKNOWN_ON_ERROR"),
        "got: {err}"
    );
}

#[test]
fn call_in_transactions_rejects_return_with_report_status() {
    let mut p = CypherParser::new(
        "CALL { MATCH (n) RETURN n } IN TRANSACTIONS REPORT STATUS AS s".to_string(),
    );
    let err = p.parse().unwrap_err();
    assert!(
        err.to_string()
            .contains("ERR_CALL_IN_TX_RETURN_WITH_STATUS"),
        "got: {err}"
    );
}

#[test]
fn call_subquery_without_in_transactions_still_parses() {
    // Back-compat — plain CALL { } subqueries must keep working
    // with the extended grammar.
    let mut p = CypherParser::new("CALL { CREATE (:Tmp) }".to_string());
    let q = p.parse().unwrap();
    let c = call_tx_clause(&q);
    assert!(!c.in_transactions);
    assert_eq!(c.batch_size, None);
    assert_eq!(c.concurrency, None);
    assert_eq!(c.status_var, None);
    assert_eq!(c.on_error, OnErrorPolicy::Fail);
    assert_eq!(c.import_list, None);
}

#[test]
fn call_subquery_parses_cypher25_import_list() {
    // phase6 §8 — `CALL (a, b) { … }` Cypher 25 scoped subquery.
    let mut p = CypherParser::new(
        "MATCH (a:Person) WITH a, 1 AS b CALL (a, b) { RETURN 42 AS x } RETURN x".to_string(),
    );
    let q = p.parse().unwrap();
    let c = call_tx_clause(&q);
    assert_eq!(c.import_list, Some(vec!["a".to_string(), "b".to_string()]));
}

#[test]
fn call_subquery_parses_empty_import_list() {
    // `CALL () { … }` — empty list (declares an isolated inner scope).
    let mut p =
        CypherParser::new("CALL () { MATCH (n) RETURN count(n) AS c } RETURN c".to_string());
    let q = p.parse().unwrap();
    let c = call_tx_clause(&q);
    assert_eq!(c.import_list, Some(vec![]));
}

// phase6_rtree-index-core §7.5 — `USING RTREE` parser alias

#[test]
fn create_index_using_rtree_marks_index_type_as_spatial() {
    let mut p = CypherParser::new(
        "CREATE INDEX place_loc FOR (p:Place) ON (p.loc) USING RTREE".to_string(),
    );
    let q = p.parse().unwrap();
    let ix = first_create_index(&q);
    assert_eq!(ix.index_type.as_deref(), Some("spatial"));
    assert_eq!(ix.label, "Place");
    assert_eq!(ix.properties, vec!["loc".to_string()]);
    assert_eq!(ix.name.as_deref(), Some("place_loc"));
}

#[test]
fn create_index_using_rtree_lowercase_also_parses() {
    // Cypher keywords are case-insensitive; the alias should
    // accept lower-case `using rtree` too.
    let mut p = CypherParser::new("CREATE INDEX FOR (p:Place) ON (p.loc) using rtree".to_string());
    let q = p.parse().unwrap();
    let ix = first_create_index(&q);
    assert_eq!(ix.index_type.as_deref(), Some("spatial"));
}

#[test]
fn create_index_using_unknown_type_errors() {
    let mut p = CypherParser::new("CREATE INDEX FOR (p:Place) ON (p.loc) USING BTREE".to_string());
    assert!(p.parse().is_err(), "USING BTREE is not yet wired up");
}

#[test]
fn create_spatial_index_legacy_form_still_parses() {
    // The pre-§7.5 grammar `CREATE SPATIAL INDEX ON :Label(prop)`
    // keeps its semantics — both shapes register the same
    // index_type so existing scripts don't break.
    let mut p = CypherParser::new("CREATE SPATIAL INDEX ON :Place(loc)".to_string());
    let q = p.parse().unwrap();
    let ix = first_create_index(&q);
    assert_eq!(ix.index_type.as_deref(), Some("spatial"));
}
