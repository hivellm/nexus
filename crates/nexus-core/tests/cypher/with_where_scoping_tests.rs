//! Regression tests for `WITH ... WHERE ...` scope visibility.
//!
//! openCypher lets the WHERE attached to a WITH clause reference a variable
//! that is in scope but NOT among the WITH's own projected aliases (e.g.
//! `WITH c WHERE r IS NULL` after `OPTIONAL MATCH (a)-[r]->(c)` — the
//! openCypher TCK `triadicSelection` feature and `WithWhere1[2]` scenario;
//! `WithWhere1[3]`/`[4]` use a comma-separated multi-pattern `MATCH` and hit
//! an unrelated, still-open defect where that shape never materializes the
//! second pattern's variable into driving rows before `OPTIONAL MATCH`
//! runs — out of scope here). The planner used to lower a WITH's attached
//! WHERE into a separate `Filter`
//! operator placed AFTER the `With` operator; by the time `Filter` ran,
//! `execute_with` had already cleared `context.variables` down to only the
//! projected aliases, so any WHERE referencing a non-projected variable
//! (like `r`) silently saw it as always-NULL. The fix threads the predicate
//! into `Operator::With` itself so `execute_with` can evaluate it per row,
//! before the scope cut, against a scope merging the pre-projection
//! bindings with the newly projected aliases.
//!
//! A second, independent defect surfaced while reproducing the TCK
//! `triadicSelection` fixture: `execute_expand`'s OPTIONAL MATCH handling
//! only padded a NULL row per source when the source had NO candidate
//! relationships at all (`relationships.is_empty()`). When a source DID
//! have candidates but every one of them was rejected per-row (e.g. a
//! closing pattern `OPTIONAL MATCH (a)-[r:KNOWS]->(c)` where `c` is already
//! bound to a specific node from an earlier clause and `a` has `:KNOWS`
//! relationships, just not to that `c`), the row was silently dropped
//! instead of preserved with `r = NULL` — see `crates/nexus-core/src/executor/operators/expand.rs`.

use nexus_core::Error;
use nexus_core::testing::setup_isolated_test_engine;

/// Upstream openCypher TCK `tck/graphs/binary-tree-1/binary-tree-1.cypher`
/// (mirrors `tck_common::BINARY_TREE_1_CYPHER`, duplicated here because that
/// constant lives in a different test binary's module tree): 13 nodes (1
/// `:A` + 12 `:X`) and 16 relationships (2 `KNOWS`, 2 `FOLLOWS`, 12
/// `FRIEND`).
const BINARY_TREE_1: &str = "CREATE (a:A {name: 'a'}),
       (b1:X {name: 'b1'}),
       (b2:X {name: 'b2'}),
       (b3:X {name: 'b3'}),
       (b4:X {name: 'b4'}),
       (c11:X {name: 'c11'}),
       (c12:X {name: 'c12'}),
       (c21:X {name: 'c21'}),
       (c22:X {name: 'c22'}),
       (c31:X {name: 'c31'}),
       (c32:X {name: 'c32'}),
       (c41:X {name: 'c41'}),
       (c42:X {name: 'c42'})
CREATE (a)-[:KNOWS]->(b1),
       (a)-[:KNOWS]->(b2),
       (a)-[:FOLLOWS]->(b3),
       (a)-[:FOLLOWS]->(b4)
CREATE (b1)-[:FRIEND]->(c11),
       (b1)-[:FRIEND]->(c12),
       (b2)-[:FRIEND]->(c21),
       (b2)-[:FRIEND]->(c22),
       (b3)-[:FRIEND]->(c31),
       (b3)-[:FRIEND]->(c32),
       (b4)-[:FRIEND]->(c41),
       (b4)-[:FRIEND]->(c42)
CREATE (b1)-[:FRIEND]->(b2),
       (b2)-[:FRIEND]->(b3),
       (b3)-[:FRIEND]->(b4),
       (b4)-[:FRIEND]->(b1);";

fn names(engine: &mut nexus_core::Engine, cypher: &str) -> Vec<String> {
    let result = engine
        .execute_cypher(cypher)
        .unwrap_or_else(|e| panic!("query `{cypher}` failed: {e}"));
    let mut values: Vec<String> = result
        .rows
        .iter()
        .map(|r| r.values[0].as_str().unwrap().to_string())
        .collect();
    values.sort();
    values
}

/// openCypher TCK `triadicSelection/TriadicSelection1[2]`: `WITH c WHERE r
/// IS NULL` after an `OPTIONAL MATCH` must keep only the rows whose
/// optional relationship did NOT bind — `r` is not one of the WITH's
/// projected aliases (only `c` is) but must still be visible to the
/// attached WHERE.
#[test]
fn with_where_is_null_sees_optional_match_relationship() -> Result<(), Error> {
    let (mut engine, _ctx) = setup_isolated_test_engine()?;
    engine.execute_cypher(BINARY_TREE_1)?;

    let result_names = names(
        &mut engine,
        "MATCH (a:A)-[:KNOWS]->(b)-->(c) \
         OPTIONAL MATCH (a)-[r:KNOWS]->(c) \
         WITH c WHERE r IS NULL \
         RETURN c.name AS name ORDER BY name",
    );
    assert_eq!(
        result_names,
        vec!["b3", "c11", "c12", "c21", "c22"],
        "TriadicSelection1[2]: everything except the direct a-KNOWS->b2 friend-of-friend \
         must survive `r IS NULL`"
    );

    Ok(())
}

/// openCypher TCK `triadicSelection/TriadicSelection1[11]`: complement of
/// the above — `WITH c WHERE r IS NOT NULL` must keep only the rows whose
/// optional relationship DID bind.
#[test]
fn with_where_is_not_null_sees_optional_match_relationship() -> Result<(), Error> {
    let (mut engine, _ctx) = setup_isolated_test_engine()?;
    engine.execute_cypher(BINARY_TREE_1)?;

    let result_names = names(
        &mut engine,
        "MATCH (a:A)-[:KNOWS]->(b)-->(c) \
         OPTIONAL MATCH (a)-[r:KNOWS]->(c) \
         WITH c WHERE r IS NOT NULL \
         RETURN c.name AS name ORDER BY name",
    );
    assert_eq!(
        result_names,
        vec!["b2"],
        "TriadicSelection1[11]: only b2 has a direct a-KNOWS->b2 edge, so only b2 should \
         survive `r IS NOT NULL`"
    );

    Ok(())
}

/// `execute_expand`'s OPTIONAL padding must never null out a `target_var`
/// that was already bound by an earlier clause (a closing pattern) — only
/// `rel_var` becomes `NULL`. This is the `relationships.is_empty()` branch
/// specifically: `a` has a `:LINK` edge to `other` but ZERO `:MISSING`
/// relationships of any kind, so `OPTIONAL MATCH (a)-[r:MISSING]->(other)`
/// must still preserve `other`'s already-known value instead of nulling it
/// out alongside `r`. Uses a connected `MATCH (a)-[:LINK]->(other)` (not a
/// comma-separated `MATCH (a), (other)`) to keep this test clear of the
/// unrelated `WithWhere1[3]`/`[4]` comma-pattern materialization defect
/// noted in this module's doc comment.
#[test]
fn optional_match_with_no_matching_relationship_type_preserves_already_bound_target()
-> Result<(), Error> {
    let (mut engine, _ctx) = setup_isolated_test_engine()?;
    engine.execute_cypher("CREATE (a:A)-[:LINK]->(other:B {id: 1})")?;

    let result = engine.execute_cypher(
        "MATCH (a:A)-[:LINK]->(other:B) \
         OPTIONAL MATCH (a)-[r:MISSING]->(other) \
         RETURN other.id AS id, r IS NULL AS is_null_r",
    )?;
    assert_eq!(
        result.rows.len(),
        1,
        "the driving (a, other) pair must survive"
    );
    assert_eq!(
        result.rows[0].values,
        vec![serde_json::json!(1), serde_json::json!(true)],
        "`other.id` must stay 1 (its already-bound value), not be nulled out alongside `r`"
    );

    Ok(())
}

/// An aggregating WITH's WHERE is unaffected by this change — it is still
/// planned as a post-aggregation predicate (`with_aggregation_where` in the
/// planner) and only sees the aggregate's own projected aliases, not the
/// pre-aggregation row scope (aggregation consumes/collapses those rows).
#[test]
fn aggregate_with_where_still_filters_on_aggregate_alias() -> Result<(), Error> {
    let (mut engine, _ctx) = setup_isolated_test_engine()?;
    engine.execute_cypher(
        "CREATE (:M {grp: 'x'}), (:M {grp: 'x'}), (:M {grp: 'x'}), (:M {grp: 'y'})",
    )?;

    let result = engine.execute_cypher(
        "MATCH (n:M) \
         WITH n.grp AS grp, count(*) AS cnt \
         WHERE cnt > 1 \
         RETURN grp, cnt ORDER BY grp",
    )?;

    let pairs: Vec<(String, i64)> = result
        .rows
        .iter()
        .map(|r| {
            (
                r.values[0].as_str().unwrap().to_string(),
                r.values[1].as_i64().unwrap(),
            )
        })
        .collect();
    assert_eq!(
        pairs,
        vec![("x".to_string(), 3)],
        "only the `x` group has more than one member, so only it should survive `cnt > 1`"
    );

    Ok(())
}

/// A plain (non-WHERE) `WITH` must still cut scope exactly like before this
/// change: a variable bound before the `WITH` but not re-projected by it is
/// gone afterwards. `r` was never carried by `WITH b`, so `RETURN r` reads
/// it back as `NULL` here — this pins the CURRENT behavior (unchanged by
/// this fix) and confirms the merged-scope WHERE evaluation added here does
/// not leak into the no-WHERE path. It is NOT spec-correct: openCypher
/// requires referencing an out-of-scope variable to be a compile-time
/// `Variable 'r' not defined` error, but `semantic_validation.rs`'s binder
/// pass does not yet track per-clause WITH scope cuts (it collects binders
/// for the whole query). Revisit/replace this assertion once that pass
/// learns WITH scope cuts and starts rejecting the query outright.
#[test]
fn plain_with_still_scopes_out_prior_variables_from_later_return() -> Result<(), Error> {
    let (mut engine, _ctx) = setup_isolated_test_engine()?;
    engine.execute_cypher("CREATE (a:P1 {name: 'a'})-[:KNOWS]->(b:P2 {name: 'b'})")?;

    let result = engine.execute_cypher("MATCH (a:P1)-[r:KNOWS]->(b:P2) WITH b RETURN r AS r")?;
    assert_eq!(
        result.rows.len(),
        1,
        "the b row itself must still come through"
    );
    assert!(
        result.rows[0].values[0].is_null(),
        "r was not carried forward by `WITH b`, so it must read back as NULL, not the \
         relationship value: got {:?}",
        result.rows[0].values[0]
    );

    Ok(())
}

/// User-visible behavior change: moving a WITH's attached WHERE off the
/// trailing `Filter` operator and into `Operator::With` also removed that
/// `Filter`'s incidental row-level dedup (`execute_filter` collapses rows
/// that are byte-for-byte identical composite keys — a `Filter`-specific
/// implementation detail, not `DISTINCT`). A non-`DISTINCT` `WITH a WHERE
/// ...` over genuinely duplicate driving rows (the same `a` reached twice
/// via two distinct relationships) now preserves both rows instead of
/// silently collapsing to one — the openCypher-correct multiset semantics
/// for a plain (non-`DISTINCT`) `WITH`, and consistent with how a
/// `WITH`-without-`WHERE` already behaved pre-fix.
#[test]
fn with_where_preserves_duplicate_rows_without_distinct() -> Result<(), Error> {
    let (mut engine, _ctx) = setup_isolated_test_engine()?;
    engine.execute_cypher("CREATE (a:N {n: 'a'})-[:R1]->(:M), (a)-[:R2]->(:M)")?;

    let result = engine.execute_cypher(
        "MATCH (a:N)-->() \
         WITH a WHERE a.n = 'a' \
         RETURN a.n AS n",
    )?;
    let names: Vec<String> = result
        .rows
        .iter()
        .map(|r| r.values[0].as_str().unwrap().to_string())
        .collect();
    assert_eq!(
        names,
        vec!["a".to_string(), "a".to_string()],
        "`a` is reached by two distinct relationships, so a non-DISTINCT `WITH ... WHERE` \
         must preserve both driving rows instead of deduping them"
    );

    Ok(())
}

/// openCypher TCK `WithWhere1[2]`: `WITH DISTINCT a.name2 AS name WHERE
/// a.name2 = 'B'` — the WHERE references `a`, which is in scope but not one
/// of the WITH's own projected aliases (only `name` is), and must be
/// evaluated BEFORE the DISTINCT dedup collapses the surviving rows.
#[test]
fn with_distinct_where_filters_before_dedup() -> Result<(), Error> {
    let (mut engine, _ctx) = setup_isolated_test_engine()?;
    engine.execute_cypher("CREATE ({name2: 'A'}), ({name2: 'A'}), ({name2: 'B'})")?;

    let result = engine.execute_cypher(
        "MATCH (a) \
         WITH DISTINCT a.name2 AS name \
         WHERE a.name2 = 'B' \
         RETURN name",
    )?;
    let result_names: Vec<String> = result
        .rows
        .iter()
        .map(|r| r.values[0].as_str().unwrap().to_string())
        .collect();
    assert_eq!(
        result_names,
        vec!["B".to_string()],
        "only the `B` row must survive `a.name2 = 'B'`, and DISTINCT must not \
         reintroduce the deduplicated `A` rows"
    );

    Ok(())
}
