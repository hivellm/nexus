//! End-to-end coverage for WHERE-form equality predicates on an indexed
//! property.
//!
//! Before this fix, `node_index_seek_for`
//! (`crates/nexus-core/src/executor/planner/queries/strategy.rs`) only ever
//! inspected the INLINE property map on a pattern node
//! (`MATCH (n:Person {age: 30})`) when deciding whether to emit
//! `Operator::NodeIndexSeek`. Every WHERE clause — including the
//! semantically identical `MATCH (n:Person) WHERE n.age = 30 RETURN n` —
//! was unconditionally lowered to `Operator::Filter`/`Operator::OptionalFilter`
//! and therefore always fell back to a full `NodeByLabel` scan, even when a
//! covering single-property index existed.
//!
//! The fix lifts a top-level `var.prop = <constant>` conjunct out of the
//! WHERE clause at PLAN TIME, in place of the equivalent inline-property
//! seek, whenever `(label, prop)` has a registered single-property index —
//! mirroring `node_index_seek_for`'s existing inline-property handling.
//! Scope is EQUALITY ONLY: range (`>`, `<`, `>=`, `<=`), `IN`,
//! `STARTS WITH`, and `CONTAINS` predicates remain full scans in this pass
//! (their lack of an index seek is made observable by the
//! `Nexus.Performance.UnindexedPropertyAccess` notification extension
//! covered below).

use nexus_core::Engine;
use nexus_core::executor::types::Operator;
use nexus_core::testing::TestContext;

const UNINDEXED_CODE: &str = "Nexus.Performance.UnindexedPropertyAccess";

/// PLAN-SHAPE (fails pre-fix): a WHERE-form equality predicate on an
/// indexed property must produce a `NodeIndexSeek`, not a `NodeByLabel`
/// scan.
#[test]
fn where_form_equality_on_indexed_property_produces_index_seek() {
    let ctx = TestContext::new();
    let mut engine = Engine::with_isolated_catalog(ctx.path()).expect("engine init");

    engine
        .execute_cypher("CREATE (:Person {name: 'Alice', age: 30})")
        .expect("seed");
    engine
        .execute_cypher("CREATE INDEX FOR (n:Person) ON (n.age)")
        .expect("create index");

    let plan = engine
        .executor
        .parse_and_plan("MATCH (n:Person) WHERE n.age = 30 RETURN n")
        .expect("plan must succeed");

    assert!(
        plan.iter()
            .any(|op| matches!(op, Operator::NodeIndexSeek { .. })),
        "WHERE-form equality on an indexed property must plan a \
         NodeIndexSeek; plan = {plan:?}"
    );
    assert!(
        !plan
            .iter()
            .any(|op| matches!(op, Operator::NodeByLabel { .. })),
        "WHERE-form equality on an indexed property must not fall back to \
         a NodeByLabel scan; plan = {plan:?}"
    );
    assert!(
        !plan
            .iter()
            .any(|op| matches!(op, Operator::AllNodesScan { .. })),
        "WHERE-form equality on an indexed property must not fall back to \
         an AllNodesScan; plan = {plan:?}"
    );
}

/// PLAN-SHAPE baseline (passes today): the inline-pattern-form sibling
/// query already produces a `NodeIndexSeek`. Confirms the asymmetry this
/// fix closes.
#[test]
fn inline_form_equality_on_indexed_property_produces_index_seek() {
    let ctx = TestContext::new();
    let mut engine = Engine::with_isolated_catalog(ctx.path()).expect("engine init");

    engine
        .execute_cypher("CREATE (:Person {name: 'Alice', age: 30})")
        .expect("seed");
    engine
        .execute_cypher("CREATE INDEX FOR (n:Person) ON (n.age)")
        .expect("create index");

    let plan = engine
        .executor
        .parse_and_plan("MATCH (n:Person {age: 30}) RETURN n")
        .expect("plan must succeed");

    assert!(
        plan.iter()
            .any(|op| matches!(op, Operator::NodeIndexSeek { .. })),
        "inline-form equality on an indexed property must plan a \
         NodeIndexSeek; plan = {plan:?}"
    );
}

/// BEHAVIORAL: on a populated dataset with several `:Person` nodes of
/// differing ages, the WHERE-form and inline-form queries must return
/// exactly the same rows — the seek is a plan-selection fix, not a
/// semantics fix.
#[test]
fn where_form_and_inline_form_return_identical_rows() {
    let ctx = TestContext::new();
    let mut engine = Engine::with_isolated_catalog(ctx.path()).expect("engine init");

    engine
        .execute_cypher(
            "CREATE (:Person {name: 'Alice', age: 30}), \
                    (:Person {name: 'Bob', age: 25}), \
                    (:Person {name: 'Carol', age: 30}), \
                    (:Person {name: 'Dave', age: 40})",
        )
        .expect("seed");
    engine
        .execute_cypher("CREATE INDEX FOR (n:Person) ON (n.age)")
        .expect("create index");

    let where_form = engine
        .execute_cypher("MATCH (n:Person) WHERE n.age = 30 RETURN n.name ORDER BY n.name")
        .expect("where-form query must succeed");
    let inline_form = engine
        .execute_cypher("MATCH (n:Person {age: 30}) RETURN n.name ORDER BY n.name")
        .expect("inline-form query must succeed");

    let where_names: Vec<String> = where_form
        .rows
        .iter()
        .map(|row| {
            row.values[0]
                .as_str()
                .expect("name must be a string")
                .to_string()
        })
        .collect();
    let inline_names: Vec<String> = inline_form
        .rows
        .iter()
        .map(|row| {
            row.values[0]
                .as_str()
                .expect("name must be a string")
                .to_string()
        })
        .collect();

    assert_eq!(
        where_names,
        vec!["Alice".to_string(), "Carol".to_string()],
        "WHERE-form must return exactly the age=30 nodes; got {where_names:?}"
    );
    assert_eq!(
        where_names, inline_names,
        "WHERE-form and inline-form must return identical rows"
    );
}

/// PLAN-SHAPE + BEHAVIORAL: when the WHERE clause has an extra conjunct
/// beyond the indexed equality, the seeked property is lifted out of the
/// Filter but the OTHER conjunct must remain as a residual `Filter` — and
/// must still be enforced correctly.
#[test]
fn where_form_equality_with_extra_conjunct_lifts_seek_and_keeps_residual_filter() {
    let ctx = TestContext::new();
    let mut engine = Engine::with_isolated_catalog(ctx.path()).expect("engine init");

    engine
        .execute_cypher(
            "CREATE (:Person {name: 'Alice', age: 30}), (:Person {name: 'Bob', age: 30})",
        )
        .expect("seed");
    engine
        .execute_cypher("CREATE INDEX FOR (n:Person) ON (n.age)")
        .expect("create index");

    let plan = engine
        .executor
        .parse_and_plan("MATCH (n:Person) WHERE n.age = 30 AND n.name = 'Bob' RETURN n")
        .expect("plan must succeed");

    assert!(
        plan.iter()
            .any(|op| matches!(op, Operator::NodeIndexSeek { .. })),
        "the age=30 conjunct must still lift to a NodeIndexSeek; plan = {plan:?}"
    );
    assert!(
        plan.iter().any(|op| matches!(op, Operator::Filter { .. })),
        "the name='Bob' conjunct must remain as a residual Filter; plan = {plan:?}"
    );

    let result = engine
        .execute_cypher("MATCH (n:Person) WHERE n.age = 30 AND n.name = 'Bob' RETURN n.name")
        .expect("query must succeed");

    let names: Vec<String> = result
        .rows
        .iter()
        .map(|row| {
            row.values[0]
                .as_str()
                .expect("name must be a string")
                .to_string()
        })
        .collect();
    assert_eq!(
        names,
        vec!["Bob".to_string()],
        "only the Bob node satisfies BOTH conjuncts; got {names:?}"
    );
}

/// `Nexus.Performance.UnindexedPropertyAccess` baseline: a range predicate
/// on an indexed property full-scans (no seek exists for range predicates
/// in this pass) and must surface the unindexed-access notification, since
/// the scan gets none of the index's benefit.
#[test]
fn range_predicate_on_indexed_property_emits_unindexed_notification() {
    let ctx = TestContext::new();
    let mut engine = Engine::with_isolated_catalog(ctx.path()).expect("engine init");

    engine
        .execute_cypher("CREATE (:Person {name: 'Alice', age: 30})")
        .expect("seed");
    engine
        .execute_cypher("CREATE INDEX FOR (n:Person) ON (n.age)")
        .expect("create index");

    let result = engine
        .execute_cypher("MATCH (n:Person) WHERE n.age > 30 RETURN n")
        .expect("query must succeed");

    assert!(
        result
            .notifications
            .iter()
            .any(|n| n.code == UNINDEXED_CODE),
        "a range predicate must full-scan and notify, even though the \
         property is indexed; got {:?}",
        result.notifications
    );
}

/// `Nexus.Performance.UnindexedPropertyAccess`: `IN` predicates on an
/// indexed property remain full scans and must notify.
#[test]
fn in_predicate_on_indexed_property_emits_unindexed_notification() {
    let ctx = TestContext::new();
    let mut engine = Engine::with_isolated_catalog(ctx.path()).expect("engine init");

    engine
        .execute_cypher("CREATE (:Person {name: 'Alice', age: 30})")
        .expect("seed");
    engine
        .execute_cypher("CREATE INDEX FOR (n:Person) ON (n.age)")
        .expect("create index");

    let result = engine
        .execute_cypher("MATCH (n:Person) WHERE n.age IN [30, 40] RETURN n")
        .expect("query must succeed");

    assert!(
        result
            .notifications
            .iter()
            .any(|n| n.code == UNINDEXED_CODE),
        "an IN predicate must full-scan and notify, even though the \
         property is indexed; got {:?}",
        result.notifications
    );
}

/// `Nexus.Performance.UnindexedPropertyAccess`: `STARTS WITH` predicates on
/// an indexed property remain full scans and must notify.
#[test]
fn starts_with_predicate_on_indexed_property_emits_unindexed_notification() {
    let ctx = TestContext::new();
    let mut engine = Engine::with_isolated_catalog(ctx.path()).expect("engine init");

    engine
        .execute_cypher("CREATE (:Person {name: 'Alice', age: 30})")
        .expect("seed");
    engine
        .execute_cypher("CREATE INDEX FOR (n:Person) ON (n.name)")
        .expect("create index");

    let result = engine
        .execute_cypher("MATCH (n:Person) WHERE n.name STARTS WITH 'A' RETURN n")
        .expect("query must succeed");

    assert!(
        result
            .notifications
            .iter()
            .any(|n| n.code == UNINDEXED_CODE),
        "a STARTS WITH predicate must full-scan and notify, even though \
         the property is indexed; got {:?}",
        result.notifications
    );
}

/// `Nexus.Performance.UnindexedPropertyAccess`: `CONTAINS` predicates on an
/// indexed property remain full scans and must notify.
#[test]
fn contains_predicate_on_indexed_property_emits_unindexed_notification() {
    let ctx = TestContext::new();
    let mut engine = Engine::with_isolated_catalog(ctx.path()).expect("engine init");

    engine
        .execute_cypher("CREATE (:Person {name: 'Alice', age: 30})")
        .expect("seed");
    engine
        .execute_cypher("CREATE INDEX FOR (n:Person) ON (n.name)")
        .expect("create index");

    let result = engine
        .execute_cypher("MATCH (n:Person) WHERE n.name CONTAINS 'lic' RETURN n")
        .expect("query must succeed");

    assert!(
        result
            .notifications
            .iter()
            .any(|n| n.code == UNINDEXED_CODE),
        "a CONTAINS predicate must full-scan and notify, even though the \
         property is indexed; got {:?}",
        result.notifications
    );
}

/// CONTRACT: equality on an indexed property is now SEEKED (§ above), so
/// it must stay SILENT — the notification tracks reality (full scan
/// despite an index existing), not just "any WHERE predicate on an
/// indexed property".
#[test]
fn equality_predicate_on_indexed_property_stays_silent() {
    let ctx = TestContext::new();
    let mut engine = Engine::with_isolated_catalog(ctx.path()).expect("engine init");

    engine
        .execute_cypher("CREATE (:Person {name: 'Alice', age: 30})")
        .expect("seed");
    engine
        .execute_cypher("CREATE INDEX FOR (n:Person) ON (n.age)")
        .expect("create index");

    let result = engine
        .execute_cypher("MATCH (n:Person) WHERE n.age = 30 RETURN n")
        .expect("query must succeed");

    assert!(
        result
            .notifications
            .iter()
            .all(|n| n.code != UNINDEXED_CODE),
        "equality on an indexed property is now seeked, so no \
         UnindexedPropertyAccess notification should fire; got {:?}",
        result.notifications
    );
}
