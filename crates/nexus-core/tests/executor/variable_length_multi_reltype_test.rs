//! Regression tests: a variable-length relationship pattern naming more
//! than one type — `[:R1|R2*1..3]` — must traverse EVERY named type, not
//! just the first one parsed.
//!
//! Root cause: `Operator::VariableLengthPath` carried a single
//! `type_id: Option<u32>` field instead of a `type_ids: Vec<u32>` list, so
//! the legacy (non-QPP) lowering in
//! `crates/nexus-core/src/executor/planner/queries/relationships.rs` did
//! `let type_id = type_ids.first().copied();`, silently discarding every
//! type but the first. Rows reachable ONLY through the other named
//! type(s) were dropped with no error.

use nexus_core::Engine;
use nexus_core::testing::TestContext;

/// Collect every string value of the single returned column across all
/// result rows, for order-independent membership assertions.
fn returned_names(result: &nexus_core::executor::ResultSet) -> Vec<String> {
    result
        .rows
        .iter()
        .filter_map(|row| row.values.first())
        .filter_map(|v| v.as_str().map(str::to_string))
        .collect()
}

/// FOLLOWS-only reachability: `Carol` is reachable from `Alice` ONLY via a
/// `:FOLLOWS` edge — there is no `:KNOWS` edge in the graph at all. A
/// `[:KNOWS|FOLLOWS*1..3]` pattern must still reach her through the
/// second named type.
#[test]
fn variable_length_multi_type_reaches_node_via_second_type_only() {
    let ctx = TestContext::new();
    let mut engine = Engine::with_isolated_catalog(ctx.path()).expect("engine init");

    engine
        .execute_cypher("CREATE (:Person {name: 'Alice'})-[:FOLLOWS]->(:Person {name: 'Carol'})")
        .expect("seed a FOLLOWS-only edge");

    let result = engine
        .execute_cypher(
            "MATCH (a:Person {name: 'Alice'})-[:KNOWS|FOLLOWS*1..3]->(b:Person) \
             RETURN b.name",
        )
        .expect("variable-length multi-type query must succeed");

    let names = returned_names(&result);
    assert!(
        names.iter().any(|n| n == "Carol"),
        "Carol is reachable only via :FOLLOWS, which is the second named \
         type in `[:KNOWS|FOLLOWS*1..3]`; expected her in the result, got {names:?}"
    );
}

/// Mixed-type path: `Alice -[:KNOWS]-> Bob -[:FOLLOWS]-> Dave`. Both hops
/// use different named types from the same union, so both `Bob` (reached
/// via the first named type) and `Dave` (reached via a path that also
/// uses the second named type) must appear.
#[test]
fn variable_length_multi_type_returns_nodes_reached_via_either_type() {
    let ctx = TestContext::new();
    let mut engine = Engine::with_isolated_catalog(ctx.path()).expect("engine init");

    engine
        .execute_cypher(
            "CREATE (:Person {name: 'Alice'})-[:KNOWS]->(:Person {name: 'Bob'})\
             -[:FOLLOWS]->(:Person {name: 'Dave'})",
        )
        .expect("seed a mixed-type chain");

    let result = engine
        .execute_cypher(
            "MATCH (a:Person {name: 'Alice'})-[:KNOWS|FOLLOWS*1..3]->(b:Person) \
             RETURN b.name",
        )
        .expect("variable-length multi-type query must succeed");

    let names = returned_names(&result);
    assert!(
        names.iter().any(|n| n == "Bob"),
        "Bob is reachable via the first named type (:KNOWS); expected him \
         in the result, got {names:?}"
    );
    assert!(
        names.iter().any(|n| n == "Dave"),
        "Dave is reachable only by continuing through the second named \
         type (:FOLLOWS) after the first hop; expected him in the result, \
         got {names:?}"
    );
}

/// Control: an UNQUALIFIED variable-length pattern (`[*1..3]`, no type
/// filter at all) must keep returning every reachable node regardless of
/// relationship type. This guards the "empty `type_ids` = match all"
/// semantics that the fix must not disturb.
#[test]
fn variable_length_unqualified_pattern_still_matches_every_type() {
    let ctx = TestContext::new();
    let mut engine = Engine::with_isolated_catalog(ctx.path()).expect("engine init");

    engine
        .execute_cypher(
            "CREATE (:Person {name: 'Alice'})-[:KNOWS]->(:Person {name: 'Bob'})\
             -[:FOLLOWS]->(:Person {name: 'Dave'})",
        )
        .expect("seed a mixed-type chain");

    let result = engine
        .execute_cypher("MATCH (a:Person {name: 'Alice'})-[*1..3]->(b:Person) RETURN b.name")
        .expect("unqualified variable-length query must succeed");

    let names = returned_names(&result);
    assert!(
        names.iter().any(|n| n == "Bob"),
        "unqualified `[*1..3]` must still reach Bob; got {names:?}"
    );
    assert!(
        names.iter().any(|n| n == "Dave"),
        "unqualified `[*1..3]` must still reach Dave; got {names:?}"
    );
}

/// Three-way type union: `[:A|B|C*1..2]` over a graph exercising all
/// three named types must return every node reachable through any one of
/// them, including the third type that a naive "first type only" fix
/// (e.g. reading only `type_ids[0]` and `type_ids[1]`) would still miss.
#[test]
fn variable_length_three_way_type_union_reaches_every_named_type() {
    let ctx = TestContext::new();
    let mut engine = Engine::with_isolated_catalog(ctx.path()).expect("engine init");

    engine
        .execute_cypher(
            "CREATE (a:Person {name: 'Alice'})-[:A]->(:Person {name: 'ViaA'}), \
                    (a)-[:B]->(:Person {name: 'ViaB'}), \
                    (a)-[:C]->(:Person {name: 'ViaC'})",
        )
        .expect("seed a single Alice node with one outgoing edge per named type");

    let result = engine
        .execute_cypher("MATCH (a:Person {name: 'Alice'})-[:A|B|C*1..2]->(b:Person) RETURN b.name")
        .expect("three-way variable-length multi-type query must succeed");

    let names = returned_names(&result);
    for expected in ["ViaA", "ViaB", "ViaC"] {
        assert!(
            names.iter().any(|n| n == expected),
            "{expected} is reachable via a named type in `[:A|B|C*1..2]`; \
             expected it in the result, got {names:?}"
        );
    }
}
