//! Regression coverage for WHERE-predicate operator precedence.
//!
//! WHERE clauses are lowered by serialising the parsed `Expression` AST to a
//! plain string (`QueryPlanner::expression_to_string`) which is stored on
//! `Operator::Filter` / `Operator::OptionalFilter` and re-parsed later by the
//! `Filter` operator. Before the fix, the serialiser rendered `BinaryOp` as
//! `"{left} {op} {right}"` with no parentheses, so grouping was lost on the
//! round trip: `(n.a OR n.b) AND n.c` serialised to `n.a OR n.b AND n.c`,
//! which re-parses under default precedence as `n.a OR (n.b AND n.c)` — a
//! different, silently wrong, boolean expression.
//!
//! Every test here uses an isolated per-test catalog via
//! `Engine::with_isolated_catalog` + `testing::TestContext`, matching the
//! pattern in `side_effects.rs` / `write_refresh_visibility_test.rs`.

use nexus_core::Engine;
use nexus_core::testing::TestContext;

#[test]
fn parenthesised_or_then_and_respects_grouping_over_booleans() {
    let ctx = TestContext::new();
    let mut engine = Engine::with_isolated_catalog(ctx.path()).expect("engine init");

    engine
        .execute_cypher("CREATE (:Flag {a: true, b: false, c: false})")
        .expect("seed CREATE must succeed");

    // Author's intended grouping: (a OR b) AND c
    //   = (true OR false) AND false
    //   = true AND false
    //   = false  -> 0 rows
    //
    // Buggy round trip drops the parens and re-parses as a OR (b AND c):
    //   = true OR (false AND false)
    //   = true OR false
    //   = true  -> 1 row (WRONG)
    let result = engine
        .execute_cypher("MATCH (n:Flag) WHERE (n.a OR n.b) AND n.c RETURN n")
        .expect("MATCH ... WHERE must succeed");

    assert_eq!(
        result.rows.len(),
        0,
        "(n.a OR n.b) AND n.c must evaluate to false for a=true,b=false,c=false \
         (author's grouping), got {} row(s) — the WHERE predicate lost its \
         parenthesised grouping on the string round trip",
        result.rows.len()
    );
}

#[test]
fn parenthesised_subtract_then_multiply_respects_grouping_over_arithmetic() {
    let ctx = TestContext::new();
    let mut engine = Engine::with_isolated_catalog(ctx.path()).expect("engine init");

    // All-positive literals (CREATE property values support only literals /
    // parameters — no unary-minus expressions — so the discriminating case
    // is built from positive numbers instead of a literal negative).
    engine
        .execute_cypher("CREATE (:M {x: 10, y: 4, z: 3})")
        .expect("seed CREATE must succeed");

    // Author's intended grouping: (x - y) * z
    //   = (10 - 4) * 3
    //   = 6 * 3
    //   = 18  -> > 0 -> 1 row
    //
    // Buggy round trip drops the parens and re-parses as x - (y * z):
    //   = 10 - (4 * 3)
    //   = 10 - 12
    //   = -2  -> not > 0 -> 0 rows (WRONG)
    let result = engine
        .execute_cypher("MATCH (n:M) WHERE (n.x - n.y) * n.z > 0 RETURN n")
        .expect("MATCH ... WHERE must succeed");

    assert_eq!(
        result.rows.len(),
        1,
        "(n.x - n.y) * n.z > 0 must evaluate to true for x=10,y=4,z=3 \
         (author's grouping), got {} row(s) — the WHERE predicate lost its \
         parenthesised grouping on the string round trip",
        result.rows.len()
    );
}

#[test]
fn optional_match_where_respects_parenthesised_grouping() {
    let ctx = TestContext::new();
    let mut engine = Engine::with_isolated_catalog(ctx.path()).expect("engine init");

    engine
        .execute_cypher(
            "CREATE (:Anchor {name: 'root'})-[:LINK]->(:Leaf {a: true, b: false, c: false})",
        )
        .expect("seed CREATE must succeed");

    // Author's intended grouping on the OPTIONAL MATCH's WHERE clause:
    // (leaf.a OR leaf.b) AND leaf.c = (true OR false) AND false = false.
    // The join match exists, but the WHERE predicate rejects it, so the
    // OPTIONAL MATCH must fall back to a NULL-bound `leaf` (LEFT OUTER JOIN
    // semantics), not silently keep the row via the buggy re-grouping.
    let result = engine
        .execute_cypher(
            "MATCH (a:Anchor) \
             OPTIONAL MATCH (a)-[:LINK]->(leaf:Leaf) \
             WHERE (leaf.a OR leaf.b) AND leaf.c \
             RETURN leaf",
        )
        .expect("MATCH ... OPTIONAL MATCH ... WHERE must succeed");

    assert_eq!(
        result.rows.len(),
        1,
        "OPTIONAL MATCH must still emit one row for the anchor"
    );
    assert!(
        result.rows[0].values[0].is_null(),
        "(leaf.a OR leaf.b) AND leaf.c must be false for a=true,b=false,c=false \
         (author's grouping), so `leaf` must be NULL-bound (OptionalFilter \
         rejected the match), got {:?} — the WHERE predicate lost its \
         parenthesised grouping on the string round trip",
        result.rows[0].values[0]
    );
}

#[test]
fn and_with_parenthesised_or_operand_respects_grouping() {
    let ctx = TestContext::new();
    let mut engine = Engine::with_isolated_catalog(ctx.path()).expect("engine init");

    // a AND (b OR c) is unambiguous even without parens (AND binds tighter
    // than OR, and this shape already matches default precedence), but a
    // deeper nesting -- NOT (a AND b) -- is a real regression risk if NOT's
    // operand grouping is lost on the round trip.
    engine
        .execute_cypher("CREATE (:N {a: true, b: true})")
        .expect("seed CREATE must succeed");

    // NOT (a AND b) = NOT (true AND true) = NOT true = false -> 0 rows.
    // Buggy round trip (if NOT's operand parens were dropped) would
    // re-parse "NOT a AND b" as "(NOT a) AND b" = (NOT true) AND true =
    // false AND true = false -- coincidentally also 0 rows for this data,
    // so use values that discriminate: a=false, b=true.
    engine
        .execute_cypher("CREATE (:N {a: false, b: true})")
        .expect("seed CREATE must succeed");

    // For (a=false, b=true):
    //   NOT (a AND b) = NOT (false AND true) = NOT false = true -> matches.
    //   Buggy "(NOT a) AND b" = (NOT false) AND true = true AND true = true
    //   -> also matches (does not discriminate on this row alone), so assert
    //   the full result set: only the (false, true) row should match, and
    //   the (true, true) row should not.
    let result = engine
        .execute_cypher("MATCH (n:N) WHERE NOT (n.a AND n.b) RETURN n.a AS a, n.b AS b")
        .expect("MATCH ... WHERE must succeed");

    assert_eq!(
        result.rows.len(),
        1,
        "NOT (n.a AND n.b) must match exactly the a=false,b=true row, got {} row(s)",
        result.rows.len()
    );
    assert_eq!(result.rows[0].values[0].as_bool(), Some(false));
    assert_eq!(result.rows[0].values[1].as_bool(), Some(true));
}

#[test]
fn projected_grouped_boolean_expression_already_evaluates_correctly() {
    // Control case: a RETURN-projected (not WHERE-filtered) grouped boolean
    // expression carries the parsed AST straight through to evaluation
    // (no string round trip), so it must already be correct both before and
    // after the fix. This isolates the defect to the WHERE string lowering
    // path specifically.
    let ctx = TestContext::new();
    let mut engine = Engine::with_isolated_catalog(ctx.path()).expect("engine init");

    engine
        .execute_cypher("CREATE (:Flag {a: true, b: false, c: false})")
        .expect("seed CREATE must succeed");

    let result = engine
        .execute_cypher("MATCH (n:Flag) RETURN (n.a OR n.b) AND n.c AS flag")
        .expect("MATCH ... RETURN must succeed");

    assert_eq!(result.rows.len(), 1);
    assert_eq!(
        result.rows[0].values[0].as_bool(),
        Some(false),
        "(n.a OR n.b) AND n.c projected via RETURN must evaluate using the \
         author's grouping (true OR false) AND false = false, got {:?}",
        result.rows[0].values[0]
    );
}
