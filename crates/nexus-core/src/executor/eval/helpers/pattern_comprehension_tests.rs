//! Unit coverage for pattern comprehensions
//! (`[(a)-[:T]->(b) | b.name]`, `[p = (a)-->(b) | p]`) — the
//! full-enumeration walk in `pattern_comprehension.rs` plus the
//! `[ident = (` parser lookahead in
//! `parser/expressions/literals.rs`. Split out of `tests.rs` to keep
//! both files under the workspace's 1500-line cap; `tests.rs` retains
//! the `EXISTS { … }` / cartesian-product coverage it was already
//! carrying.

use crate::executor::Query;
use crate::testing::create_test_executor;
use serde_json::Value;
use serde_json::json;
use std::collections::HashMap;

/// A pattern comprehension without a path-binding variable walks the
/// graph and transforms each match — the base case the old stub could
/// never satisfy (it only ever inspected the current row's already-bound
/// variables, so any pattern variable the comprehension itself declares,
/// like `x` below, always fell through to "return `[]`").
#[test]
fn pattern_comprehension_projects_target_properties() {
    let (mut executor, _ctx) = create_test_executor();

    let create = Query {
        cypher: "CREATE (a:PatternCompA {name: 'a'})-[:PC_KNOWS]->(b:PatternCompA {name: 'b'}), \
                 (a)-[:PC_KNOWS]->(c:PatternCompA {name: 'c'})"
            .to_string(),
        params: HashMap::new(),
    };
    executor.execute(&create).expect("create should succeed");

    let query = Query {
        cypher: "MATCH (n:PatternCompA {name: 'a'}) \
                 RETURN [(n)-[:PC_KNOWS]->(x) | x.name] AS names"
            .to_string(),
        params: HashMap::new(),
    };
    let result = executor.execute(&query).expect("match should succeed");

    assert_eq!(result.rows.len(), 1);
    let mut names: Vec<&str> = result.rows[0].values[0]
        .as_array()
        .expect("names is an array")
        .iter()
        .map(|v| v.as_str().expect("name is a string"))
        .collect();
    names.sort_unstable();
    assert_eq!(names, vec!["b", "c"]);
}

/// `[p = (n)-->() | p]` binds `p` to a path value shaped exactly like
/// `shortestPath`'s output (`{nodes: [...], relationships: [...]}` in
/// traversal order) — the shape the TCK runner's `@tck_path` matcher
/// compares against.
#[test]
fn pattern_comprehension_path_binding_yields_path_shaped_values() {
    let (mut executor, _ctx) = create_test_executor();

    let create = Query {
        cypher: "CREATE (a:PatternCompB {name: 'a'})-[:PC_STEP]->(b:PatternCompB {name: 'b'})"
            .to_string(),
        params: HashMap::new(),
    };
    executor.execute(&create).expect("create should succeed");

    let query = Query {
        cypher: "MATCH (n:PatternCompB {name: 'a'}) \
                 RETURN [p = (n)-->() | p] AS paths"
            .to_string(),
        params: HashMap::new(),
    };
    let result = executor.execute(&query).expect("match should succeed");

    assert_eq!(result.rows.len(), 1);
    let paths = result.rows[0].values[0]
        .as_array()
        .expect("paths is an array");
    assert_eq!(paths.len(), 1, "exactly one outgoing relationship from a");

    let path_obj = paths[0].as_object().expect("path is an object");
    let nodes = path_obj["nodes"].as_array().expect("nodes is an array");
    let rels = path_obj["relationships"]
        .as_array()
        .expect("relationships is an array");
    assert_eq!(nodes.len(), 2, "path visits the anchor and the target node");
    assert_eq!(rels.len(), 1, "path traverses exactly one relationship");
    assert_eq!(nodes[0]["name"], json!("a"));
    assert_eq!(nodes[1]["name"], json!("b"));
}

/// The comprehension's own `WHERE` filters candidate bindings one at a
/// time (same per-candidate timing as `EXISTS`'s inner `WHERE`), not the
/// whole comprehension result as a single unit.
#[test]
fn pattern_comprehension_where_clause_filters_candidates() {
    let (mut executor, _ctx) = create_test_executor();

    let create = Query {
        cypher: "CREATE (a:PatternCompC {name: 'a'})-[:PC_KNOWS]->\
                 (b:PatternCompC {name: 'b', age: 10}), \
                 (a)-[:PC_KNOWS]->(c:PatternCompC {name: 'c', age: 30})"
            .to_string(),
        params: HashMap::new(),
    };
    executor.execute(&create).expect("create should succeed");

    let query = Query {
        cypher: "MATCH (n:PatternCompC {name: 'a'}) \
                 RETURN [(n)-[:PC_KNOWS]->(x) WHERE x.age > 20 | x.name] AS names"
            .to_string(),
        params: HashMap::new(),
    };
    let result = executor.execute(&query).expect("match should succeed");

    assert_eq!(result.rows.len(), 1);
    let names: Vec<&str> = result.rows[0].values[0]
        .as_array()
        .expect("names is an array")
        .iter()
        .map(|v| v.as_str().expect("name is a string"))
        .collect();
    assert_eq!(names, vec!["c"]);
}

/// A pattern that never matches any relationship yields `[]`, not an
/// error — mirrors `EXISTS`'s "no witness found" outcome, just without
/// the boolean wrapper.
#[test]
fn pattern_comprehension_unmatched_pattern_yields_empty_array() {
    let (mut executor, _ctx) = create_test_executor();

    let create = Query {
        cypher: "CREATE (a:PatternCompD {name: 'a'})".to_string(),
        params: HashMap::new(),
    };
    executor.execute(&create).expect("create should succeed");

    let query = Query {
        cypher: "MATCH (n:PatternCompD {name: 'a'}) \
                 RETURN [(n)-[:PC_NONE]->(x) | x.name] AS names"
            .to_string(),
        params: HashMap::new(),
    };
    let result = executor.execute(&query).expect("match should succeed");

    assert_eq!(result.rows.len(), 1);
    assert_eq!(result.rows[0].values[0], Value::Array(Vec::new()));
}

/// A comprehension whose pattern reuses a variable that is already
/// bound to `NULL` in the row (the classic `OPTIONAL MATCH` failure
/// shape) yields `[]` rather than propagating three-valued `NULL` —
/// there is no boolean result here for `NULL` to compose with, so this
/// intentionally diverges from `EXISTS`'s tri-state and instead mirrors
/// `[x IN NULL | …]`, which is also `[]`.
#[test]
fn pattern_comprehension_over_null_correlated_variable_yields_empty_array() {
    let (mut executor, _ctx) = create_test_executor();

    let create = Query {
        cypher: "CREATE (a:PatternCompE {name: 'a'})".to_string(),
        params: HashMap::new(),
    };
    executor.execute(&create).expect("create should succeed");

    let query = Query {
        cypher: "MATCH (a:PatternCompE {name: 'a'}) \
                 OPTIONAL MATCH (a)-[:PC_MISSING]->(m:PatternCompE) \
                 RETURN [(m)-->() | m.name] AS names"
            .to_string(),
        params: HashMap::new(),
    };
    let result = executor.execute(&query).expect("match should succeed");

    assert_eq!(result.rows.len(), 1);
    assert_eq!(result.rows[0].values[0], Value::Array(Vec::new()));
}

/// A bounded variable-length hop inside a comprehension enumerates one
/// list element per distinct trail (one per hop count in range), not
/// just the longest or shortest one.
#[test]
fn pattern_comprehension_var_length_enumerates_each_trail() {
    let (mut executor, _ctx) = create_test_executor();

    let create = Query {
        cypher: "CREATE (a:PatternCompF {name: 'a'})-[:PC_STEP]->\
                 (b:PatternCompF {name: 'b'})-[:PC_STEP]->(c:PatternCompF {name: 'c'})"
            .to_string(),
        params: HashMap::new(),
    };
    executor.execute(&create).expect("create should succeed");

    let query = Query {
        cypher: "MATCH (n:PatternCompF {name: 'a'}) \
                 RETURN [(n)-[:PC_STEP*1..2]->(x) | x.name] AS names"
            .to_string(),
        params: HashMap::new(),
    };
    let result = executor.execute(&query).expect("match should succeed");

    assert_eq!(result.rows.len(), 1);
    let mut names: Vec<&str> = result.rows[0].values[0]
        .as_array()
        .expect("names is an array")
        .iter()
        .map(|v| v.as_str().expect("name is a string"))
        .collect();
    names.sort_unstable();
    assert_eq!(names, vec!["b", "c"]);
}

/// Relationship isomorphism holds within a single comprehension
/// element: a two-hop chain through a self-loop can't satisfy both hops
/// with the same edge, so a node whose only outgoing edge is a self-loop
/// produces no two-hop witness.
#[test]
fn pattern_comprehension_isomorphism_blocks_reusing_the_only_edge() {
    let (mut executor, _ctx) = create_test_executor();

    let create = Query {
        cypher: "CREATE (a:PatternCompG {name: 'a'})-[:PC_LOOP]->(a)".to_string(),
        params: HashMap::new(),
    };
    executor.execute(&create).expect("create should succeed");

    let query = Query {
        cypher: "MATCH (n:PatternCompG {name: 'a'}) \
                 RETURN [(n)-[:PC_LOOP]->(x)-[:PC_LOOP]->(y) | y.name] AS names"
            .to_string(),
        params: HashMap::new(),
    };
    let result = executor.execute(&query).expect("match should succeed");

    assert_eq!(result.rows.len(), 1);
    assert_eq!(result.rows[0].values[0], Value::Array(Vec::new()));
}

/// `size()` over a pattern comprehension counts its matches like any
/// other list-valued expression — no special-casing needed once the
/// comprehension itself returns a real `Value::Array`.
#[test]
fn pattern_comprehension_size_counts_matches() {
    let (mut executor, _ctx) = create_test_executor();

    let create = Query {
        cypher: "CREATE (a:PatternCompH {name: 'a'})-[:PC_KNOWS]->(b:PatternCompH {name: 'b'}), \
                 (a)-[:PC_KNOWS]->(c:PatternCompH {name: 'c'})"
            .to_string(),
        params: HashMap::new(),
    };
    executor.execute(&create).expect("create should succeed");

    let query = Query {
        cypher: "MATCH (n:PatternCompH {name: 'a'}) \
                 RETURN size([(n)-[:PC_KNOWS]->(x) | x.name]) AS cnt"
            .to_string(),
        params: HashMap::new(),
    };
    let result = executor.execute(&query).expect("match should succeed");

    assert_eq!(result.rows.len(), 1);
    assert_eq!(result.rows[0].values[0], json!(2));
}

/// `[ident = ...]` inputs that are NOT a path-bound pattern comprehension
/// must still parse as plain list literals holding a boolean-equality
/// expression — the parser's pattern-comprehension lookahead is only a
/// tentative attempt, never an irreversible commit. `[a = b]` and
/// `[x = 1]` don't even reach the tentative pattern parse (the `=` isn't
/// followed by `(`), so this pins the pre-existing lookahead-only
/// behavior as a regression guard around the surrounding rewrite.
///
/// This pins PARSER behavior only, via the low-level `Executor` (no
/// semantic-validation pass): `a`, `b`, `x` are never bound by any
/// clause, so through the full engine pipeline these same queries
/// would be rejected with `UndefinedVariable` before execution.
#[test]
fn pattern_comprehension_parser_leaves_bare_equality_lists_alone() {
    let (mut executor, _ctx) = create_test_executor();

    let query = Query {
        cypher: "WITH 1 AS _seed \
                 RETURN [a = b] AS bare_vars, [x = 1] AS var_and_literal"
            .to_string(),
        params: HashMap::new(),
    };
    let result = executor.execute(&query).expect("query should succeed");

    assert_eq!(result.rows.len(), 1, "no full-graph scan — a single row");
    // `a` and `b` are undefined variables (both NULL); Cypher's `NULL =
    // NULL` is NULL, not true — still a one-element list, never a
    // pattern-comprehension result shape (which would be `{nodes, ...}`
    // objects or a bare scalar list from a totally different query).
    assert_eq!(
        result.rows[0].values[0],
        Value::Array(vec![Value::Null]),
        "bare identifier equality with two undefined variables"
    );
    assert_eq!(
        result.rows[0].values[1],
        Value::Array(vec![Value::Null]),
        "bare identifier equality against a literal, x undefined"
    );
}

/// `[ident = (...)]` where the parenthesized content is NOT a valid
/// pattern (here, an arithmetic expression) must restore position and
/// fall through to normal list/expression parsing instead of
/// propagating the inner pattern-parse failure as a hard syntax error —
/// this is the core MAJOR-2 regression: the parser must never commit
/// irreversibly once it has consumed `ident = (`.
///
/// This pins PARSER behavior only, via the low-level `Executor` (no
/// semantic-validation pass): `x` is never bound by any clause, so
/// through the full engine pipeline this same query would be rejected
/// with `UndefinedVariable` before execution.
#[test]
fn pattern_comprehension_parser_backtracks_on_non_pattern_parenthesized_expression() {
    let (mut executor, _ctx) = create_test_executor();

    let query = Query {
        cypher: "WITH 1 AS _seed RETURN [x = (1 + 2)] AS r".to_string(),
        params: HashMap::new(),
    };
    let result = executor.execute(&query).expect(
        "a failed tentative pattern parse must fall back to list/expression parsing, not error",
    );

    assert_eq!(result.rows.len(), 1);
    // `x` is undefined (NULL); `NULL = 3` is NULL under three-valued logic.
    assert_eq!(result.rows[0].values[0], Value::Array(vec![Value::Null]));
}

/// `[ident = (bare_node)]` — a syntactically valid pattern with zero
/// relationships — is never a path-bound comprehension (openCypher
/// requires a `RelationshipsPattern`); the parser must reject the
/// binding-variable shape constraint and fall through to plain
/// list/expression parsing, evaluating it as an equality between `a`
/// and the parenthesized value of `b`, not silently accept `(b)` as a
/// one-node comprehension.
///
/// This pins PARSER behavior only, via the low-level `Executor` (no
/// semantic-validation pass): `a`, `b` are never bound by any clause,
/// so through the full engine pipeline this same query would be
/// rejected with `UndefinedVariable` before execution.
#[test]
fn pattern_comprehension_parser_rejects_bare_node_as_path_binding() {
    let (mut executor, _ctx) = create_test_executor();

    let query = Query {
        cypher: "WITH 1 AS _seed RETURN [a = (b)] AS r".to_string(),
        params: HashMap::new(),
    };
    let result = executor
        .execute(&query)
        .expect("a bare-node `ident = (b)` shape must fall back to equality, not a comprehension");

    assert_eq!(result.rows.len(), 1);
    // `a` and `b` are undefined (NULL); `NULL = NULL` is NULL.
    assert_eq!(result.rows[0].values[0], Value::Array(vec![Value::Null]));
}

/// `[a = (b), (c)]` has no relationship anywhere in the parsed pattern
/// (`(b), (c)` is two bare nodes) — this must NOT be treated as a
/// comma-separated path-binding attempt (which hard-errors once a
/// relationship has parsed, per the MINOR-2 fix); it is genuinely
/// ambiguous, and its only valid reading is the outer LIST LITERAL's
/// own two elements (`a = (b)`, `(c)`). Regression test for reordering
/// the relationship check ahead of the comma-separated check: with the
/// relationship check first, `has_relationship` is `false` here, so
/// every failure — including the comma — stays soft and falls back.
///
/// This pins PARSER behavior only, via the low-level `Executor` (no
/// semantic-validation pass): `a`, `b`, `c` are never bound by any
/// clause, so through the full engine pipeline this same query would
/// be rejected with `UndefinedVariable` before execution.
#[test]
fn pattern_comprehension_parser_treats_relationshipless_comma_pattern_as_two_list_elements() {
    let (mut executor, _ctx) = create_test_executor();

    let query = Query {
        cypher: "WITH 1 AS _seed RETURN [a = (b), (c)] AS r".to_string(),
        params: HashMap::new(),
    };
    let result = executor.execute(&query).expect(
        "a relationship-less comma pattern must fall back to a two-element list, not error",
    );

    assert_eq!(result.rows.len(), 1);
    let list = result.rows[0].values[0]
        .as_array()
        .expect("r is a two-element list");
    assert_eq!(list.len(), 2, "the outer list literal's own two elements");
    // `a`, `b`, `c` are all undefined (NULL); `NULL = NULL` is NULL,
    // and a bare undefined variable is also NULL.
    assert_eq!(list[0], Value::Null);
    assert_eq!(list[1], Value::Null);
}

/// An incoming-direction hop (`<--`) records its trail in traversal
/// order — the anchor node first, then the node the edge actually
/// originates from — not source/target storage order.
#[test]
fn pattern_comprehension_incoming_direction_trail_order() {
    let (mut executor, _ctx) = create_test_executor();

    let create = Query {
        cypher: "CREATE (a:PatternCompI {name: 'a'})-[:PC_IN]->(b:PatternCompI {name: 'b'})"
            .to_string(),
        params: HashMap::new(),
    };
    executor.execute(&create).expect("create should succeed");

    let query = Query {
        cypher: "MATCH (n:PatternCompI {name: 'b'}) \
                 RETURN [p = (n)<--(x) | p] AS list"
            .to_string(),
        params: HashMap::new(),
    };
    let result = executor.execute(&query).expect("match should succeed");

    assert_eq!(result.rows.len(), 1);
    let paths = result.rows[0].values[0]
        .as_array()
        .expect("list is an array");
    assert_eq!(paths.len(), 1);
    let path_obj = paths[0].as_object().expect("path is an object");
    let nodes = path_obj["nodes"].as_array().expect("nodes is an array");
    let rels = path_obj["relationships"]
        .as_array()
        .expect("relationships is an array");
    assert_eq!(nodes.len(), 2);
    assert_eq!(rels.len(), 1);
    // Traversal order: anchor `b` first, then the node reached by
    // walking the incoming edge backward, `a`.
    assert_eq!(nodes[0]["name"], json!("b"));
    assert_eq!(nodes[1]["name"], json!("a"));
}

/// A two-hop fixed-length pattern comprehension records every node
/// and relationship it traverses, in the order it traverses them.
#[test]
fn pattern_comprehension_two_hop_trail_order() {
    let (mut executor, _ctx) = create_test_executor();

    let create = Query {
        cypher: "CREATE (a:PatternCompJ {name: 'a'})-[:PC_STEP1]->\
                 (b:PatternCompJ {name: 'b'})-[:PC_STEP2]->(c:PatternCompJ {name: 'c'})"
            .to_string(),
        params: HashMap::new(),
    };
    executor.execute(&create).expect("create should succeed");

    let query = Query {
        cypher: "MATCH (n:PatternCompJ {name: 'a'}) \
                 RETURN [p = (n)-->()-->() | p] AS list"
            .to_string(),
        params: HashMap::new(),
    };
    let result = executor.execute(&query).expect("match should succeed");

    assert_eq!(result.rows.len(), 1);
    let paths = result.rows[0].values[0]
        .as_array()
        .expect("list is an array");
    assert_eq!(paths.len(), 1);
    let path_obj = paths[0].as_object().expect("path is an object");
    let nodes = path_obj["nodes"].as_array().expect("nodes is an array");
    let rels = path_obj["relationships"]
        .as_array()
        .expect("relationships is an array");
    assert_eq!(nodes.len(), 3);
    assert_eq!(rels.len(), 2);
    assert_eq!(nodes[0]["name"], json!("a"));
    assert_eq!(nodes[1]["name"], json!("b"));
    assert_eq!(nodes[2]["name"], json!("c"));
    assert_eq!(rels[0]["_nexus_rel_type"], json!("PC_STEP1"));
    assert_eq!(rels[1]["_nexus_rel_type"], json!("PC_STEP2"));
}

/// A bounded variable-length pattern comprehension with a path-binding
/// variable produces one path object per distinct trail length — a
/// 1-hop trail and a 2-hop trail are two separate list elements, each
/// with its own node/relationship arrays in traversal order, not a
/// single path collapsed to the longest (or shortest) trail.
#[test]
fn pattern_comprehension_var_length_trail_per_distinct_hop_count() {
    let (mut executor, _ctx) = create_test_executor();

    let create = Query {
        cypher: "CREATE (a:PatternCompK {name: 'a'})-[:PC_VSTEP]->\
                 (b:PatternCompK {name: 'b'})-[:PC_VSTEP]->(c:PatternCompK {name: 'c'})"
            .to_string(),
        params: HashMap::new(),
    };
    executor.execute(&create).expect("create should succeed");

    let query = Query {
        cypher: "MATCH (n:PatternCompK {name: 'a'}) \
                 RETURN [p = (n)-[:PC_VSTEP*1..2]->(x) | p] AS list"
            .to_string(),
        params: HashMap::new(),
    };
    let result = executor.execute(&query).expect("match should succeed");

    assert_eq!(result.rows.len(), 1);
    let paths = result.rows[0].values[0]
        .as_array()
        .expect("list is an array");
    assert_eq!(paths.len(), 2, "one path per distinct hop count (1 and 2)");

    let one_hop = paths
        .iter()
        .find(|p| p["nodes"].as_array().expect("nodes is an array").len() == 2)
        .expect("a 1-hop path (2 nodes) must be present");
    let one_hop_nodes = one_hop["nodes"].as_array().expect("nodes is an array");
    let one_hop_rels = one_hop["relationships"]
        .as_array()
        .expect("relationships is an array");
    assert_eq!(one_hop_rels.len(), 1);
    assert_eq!(one_hop_nodes[0]["name"], json!("a"));
    assert_eq!(one_hop_nodes[1]["name"], json!("b"));

    let two_hop = paths
        .iter()
        .find(|p| p["nodes"].as_array().expect("nodes is an array").len() == 3)
        .expect("a 2-hop path (3 nodes) must be present");
    let two_hop_nodes = two_hop["nodes"].as_array().expect("nodes is an array");
    let two_hop_rels = two_hop["relationships"]
        .as_array()
        .expect("relationships is an array");
    assert_eq!(two_hop_rels.len(), 2);
    assert_eq!(two_hop_nodes[0]["name"], json!("a"));
    assert_eq!(two_hop_nodes[1]["name"], json!("b"));
    assert_eq!(two_hop_nodes[2]["name"], json!("c"));
}

/// A comma-separated pattern under a `p =` path binding has no single
/// traversal order to bind `p` to, and no other valid Cypher reading
/// (unlike a bare-node or transform-less pattern) — the parser must
/// raise a clear syntax error rather than silently building a
/// nonsense multi-component path or falling back to a confusing
/// generic parse failure.
#[test]
fn pattern_comprehension_comma_separated_pattern_with_path_binding_is_rejected() {
    let (mut executor, _ctx) = create_test_executor();

    let query = Query {
        cypher: "WITH 1 AS _seed \
                 RETURN [p = (a)-->(b), (c)-->(d) | p] AS list"
            .to_string(),
        params: HashMap::new(),
    };
    let err = executor
        .execute(&query)
        .expect_err("a comma-separated pattern under a path binding must be rejected");
    // Assert the sentinel token itself, not just the error variant —
    // `CypherSyntax` alone can't distinguish this from any other parse
    // failure and wouldn't actually catch a regression back to the
    // swallowed/fallback behavior this test guards against.
    match err {
        crate::Error::CypherSyntax(msg) => assert!(
            msg.contains("ERR_PATTERN_COMPREHENSION_COMMA_SEPARATED_PATH_BINDING"),
            "expected the comma-separated-path-binding sentinel, got: {msg}"
        ),
        other => panic!("expected CypherSyntax, got {other:?}"),
    }
}
