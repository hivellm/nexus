//! Regression tests for phase3_engine-dispatch-consolidation: unifying
//! `execute_cypher_dispatch` and `execute_cypher_ast` into one private
//! `Engine::dispatch`, and retiring the params-dropping
//! `Engine::execute_cypher(&str)` footgun.
//!
//! Each test below locks one concrete divergence found while diffing the
//! two (formerly separate) dispatch functions — see `Engine::dispatch`'s
//! doc comment in `engine/query_pipeline.rs` for the full list.

use super::*;

/// Neo4j semantics: `PROFILE` executes the query, it does not just plan
/// it. Before the consolidation this was already true for MERGE (both
/// forks routed identically through `execute_write_query`), but the two
/// dispatch functions diverged elsewhere in ways that could have let a
/// future MERGE-adjacent fix land in only one fork. This test locks the
/// end-to-end contract explicitly, as requested for this task.
#[test]
fn profile_merge_creates_relationship_same_as_unprofiled() {
    let ctx = crate::testing::TestContext::new();
    let mut engine = Engine::with_isolated_catalog(ctx.path()).unwrap();

    let r = engine
        .execute_cypher("PROFILE MERGE (a:PC {id:1})-[r:PR]->(b:PC {id:2})")
        .expect("PROFILE MERGE must execute, not just plan");
    assert_eq!(r.columns, vec!["profile".to_string()]);

    let edge = engine
        .execute_cypher("MATCH (a:PC {id:1})-[r:PR]->(b:PC {id:2}) RETURN count(r) AS c")
        .expect("re-read after PROFILE MERGE");
    assert_eq!(
        edge.rows[0].values[0].as_i64(),
        Some(1),
        "PROFILE MERGE must perform the same write as the unprofiled query"
    );
}

/// Divergence: the AST-only fork (`execute_cypher_ast`) never checked
/// `has_show_constraints` — the query-text fork did. `SHOW CONSTRAINTS`
/// reached through PROFILE (or any other internal AST-holding caller)
/// fell through to the generic executor, which has no `ShowConstraints`
/// operator and silently drops unmatched clauses (`_ => {}` in
/// `planner_core.rs`), yielding an empty/wrong result instead of the
/// constraint list.
///
/// `PROFILE SHOW CONSTRAINTS` is not parseable Cypher (the PROFILE inner
/// grammar has no `SHOW` arm), so this calls the internal, AST-holding
/// entry point directly — `execute_cypher_ast` is `pub(super)`,
/// reachable from this descendant test module — mirroring how PROFILE
/// and `CALL { ... }` recursion reach [`Engine::dispatch`] with
/// `DispatchSource::Internal`.
#[test]
fn internal_ast_dispatch_routes_show_constraints() {
    let ctx = crate::testing::TestContext::new();
    let mut engine = Engine::with_isolated_catalog(ctx.path()).unwrap();
    engine
        .execute_cypher("CREATE CONSTRAINT ON (n:PSC) ASSERT n.id IS UNIQUE")
        .expect("create constraint");

    let direct = engine
        .execute_cypher("SHOW CONSTRAINTS")
        .expect("direct SHOW CONSTRAINTS");
    assert!(
        !direct.rows.is_empty(),
        "constraint must be listed directly"
    );

    let mut parser = crate::executor::parser::CypherParser::new("SHOW CONSTRAINTS".to_string());
    let ast = parser.parse().expect("parse SHOW CONSTRAINTS");
    let via_internal_dispatch = engine
        .execute_cypher_ast(&ast)
        .expect("internal AST dispatch must route SHOW CONSTRAINTS to the constraint listing");
    assert_eq!(
        via_internal_dispatch.rows.len(),
        direct.rows.len(),
        "internal AST dispatch must return the same row count as the unprofiled listing"
    );
    assert_eq!(via_internal_dispatch.columns, direct.columns);
}

/// Divergence: the AST-only fork's standalone CREATE routed through the
/// legacy `execute_create_query` (a manual pattern walk in
/// `match_exec.rs` that predates the executor's CREATE operator) instead
/// of the query-text fork's executor-backed path, so it never got the
/// `0a46cadf` typed-property-index fix
/// (`index_typed_properties_for_new_nodes`). A node created through
/// PROFILE was seek-invisible to a follow-up `MATCH {prop}`.
#[test]
fn profile_create_indexes_property_same_as_unprofiled() {
    let ctx = crate::testing::TestContext::new();
    let mut engine = Engine::with_isolated_catalog(ctx.path()).unwrap();

    engine
        .execute_cypher("PROFILE CREATE (n:PCreate {x: 1})")
        .expect("PROFILE CREATE must execute");

    // Depends on the typed property B-tree seeing the node — the exact
    // path the 0a46cadf fix targeted.
    engine
        .execute_cypher("MATCH (n:PCreate {x: 1}) SET n.y = 2")
        .expect("SET must find the PROFILE-created node via the typed property index");

    let read = engine
        .execute_cypher("MATCH (n:PCreate {x: 1}) RETURN n.y AS y")
        .expect("read back");
    assert_eq!(
        read.rows[0].values[0].as_i64(),
        Some(2),
        "PROFILE-created node must be indexed just like a non-profiled CREATE"
    );
}

/// Divergence: on `DELETE ... RETURN <expr>` (non-count), the query-text
/// fork replayed a RETURN-only tail AST (`phase6 §8.2`), while the
/// AST-only fork replayed the FULL original ast — including the CREATE /
/// DELETE clauses — a second time (a double CREATE + double DELETE via
/// `self.executor.execute`). The unified `dispatch` shares ONE DELETE
/// branch with no `match source` at this point at all — the RETURN-only
/// tail_ast is now structurally the only code path for both callers, so
/// a second full-AST execution is no longer reachable. This test locks
/// the externally observable contract: PROFILE returns exactly the one
/// row the RETURN clause produces, and the created-then-deleted node
/// does not survive — both of which a double execution would be liable
/// to disturb (an extra row, an error re-deleting an already-deleted
/// node, or a leftover survivor).
#[test]
fn profile_create_delete_return_executes_exactly_once() {
    let ctx = crate::testing::TestContext::new();
    let mut engine = Engine::with_isolated_catalog(ctx.path()).unwrap();

    let r = engine
        .execute_cypher("PROFILE CREATE (n:PDel {x: 1}) WITH n DELETE n RETURN 'done' AS status")
        .expect("PROFILE CREATE + WITH + DELETE + RETURN must execute");
    assert_eq!(r.columns, vec!["profile".to_string()]);
    let profile = &r.rows[0].values[0];
    assert_eq!(
        profile.get("rows_returned").and_then(|v| v.as_u64()),
        Some(1),
        "profiled execution must return exactly the one 'done' row, got {profile:?}"
    );

    let surviving = engine
        .execute_cypher("MATCH (n:PDel) RETURN count(n) AS c")
        .unwrap();
    assert_eq!(surviving.rows[0].values[0].as_i64(), Some(0));
}

/// Divergence: both the RETURN-tail and final-fallback branches on the
/// AST-only fork threaded `ast.params` (always `{}` — the parser never
/// populates it, see `executor/parser/clauses/mod.rs`) into the executor
/// instead of `self.current_params` (the actually-supplied params). Any
/// `$param` reference in a query reached through PROFILE silently
/// resolved to nothing. Locked by round-tripping a parameterized read
/// through PROFILE.
#[test]
fn profile_query_resolves_supplied_parameters() {
    let ctx = crate::testing::TestContext::new();
    let mut engine = Engine::with_isolated_catalog(ctx.path()).unwrap();
    engine
        .execute_cypher("CREATE (n:PParam {x: 7})")
        .expect("seed CREATE");

    let mut params = std::collections::HashMap::new();
    params.insert("v".to_string(), serde_json::json!(7));
    let r = engine
        .execute_cypher_with_params(
            "PROFILE MATCH (n:PParam) WHERE n.x = $v RETURN n.x AS x",
            params,
        )
        .expect("PROFILE with $param must execute");
    let profile = &r.rows[0].values[0];
    assert_eq!(
        profile.get("rows_returned").and_then(|v| v.as_u64()),
        Some(1),
        "PROFILE must resolve $v and match the seeded node, got {profile:?}"
    );
}

/// Contract test for the retired params-dropping footgun (L5):
/// `execute_cypher(&str)` now delegates to
/// `execute_cypher_with_params(query, HashMap::new())`, which resets
/// `current_params` to empty for the duration of the call. A prior
/// `execute_cypher_with_params` call's parameters must never leak into a
/// later param-less `execute_cypher` call.
#[test]
fn execute_cypher_no_params_does_not_leak_prior_call_params() {
    let ctx = crate::testing::TestContext::new();
    let mut engine = Engine::with_isolated_catalog(ctx.path()).unwrap();

    let mut params = std::collections::HashMap::new();
    params.insert("v".to_string(), serde_json::json!(99));
    engine
        .execute_cypher_with_params("CREATE (n:ExecCypherLeak {x: $v})", params)
        .expect("seed CREATE with params");

    let err = engine
        .execute_cypher("CREATE (n:ExecCypherLeak2 {x: $v})")
        .expect_err("a param-less execute_cypher must not resolve $v from a prior call");
    assert!(
        err.to_string().contains('v'),
        "error should name the missing parameter; got {err}"
    );
}
