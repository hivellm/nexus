//! Neo4j Compatibility Tests — additional numbered compatibility coverage.
//!
//! Split from `neo4j_compatibility_test.rs` (tier-3 oversized-module refactor).
//! Hosts the 65 short, sequentially-numbered `neo4j_compat_*` / `neo4j_test_*`
//! cases. Each test is a minimal scenario exercising a single behaviour
//! (count, labels, keys, id, type, LIMIT, DISTINCT, property types, etc.).
//!
//! NOTE: These tests use `#[serial]` to prevent LMDB resource contention when
//! running with high parallelism (e.g., nextest).

use nexus_core::Engine;
use nexus_core::testing::setup_test_engine;
use serde_json::json;
use serial_test::serial;

/// Helper function to execute a Cypher query via Engine
fn execute_query(
    engine: &mut Engine,
    query: &str,
) -> Result<nexus_core::executor::ResultSet, String> {
    engine
        .execute_cypher(query)
        .map_err(|e| format!("Query execution failed: {}", e))
}

#[test]
#[serial]
fn neo4j_compat_01() {
    let (mut e, _ctx) = setup_test_engine().unwrap();
    e.create_node(vec!["T".to_string()], json!({})).unwrap();
    e.refresh_executor().unwrap();
    execute_query(&mut e, "MATCH (n:T) RETURN count(*) AS c").unwrap();
}

#[test]
#[serial]
fn neo4j_compat_02() {
    let (mut e, _ctx) = setup_test_engine().unwrap();
    for _i in 0..5 {
        e.create_node(vec!["T".to_string()], json!({})).unwrap();
    }
    e.refresh_executor().unwrap();
    let r = execute_query(&mut e, "MATCH (n:T) RETURN count(*) AS c").unwrap();
    assert_eq!(r.rows[0].values[0], json!(5));
}

#[test]
#[serial]
fn neo4j_compat_04() {
    let (mut e, _ctx) = setup_test_engine().unwrap();
    e.create_node(vec!["T".to_string()], json!({"k": 1}))
        .unwrap();
    e.refresh_executor().unwrap();
    execute_query(&mut e, "MATCH (n:T) RETURN keys(n) AS k").unwrap();
}

#[test]
#[serial]
fn neo4j_compat_05() {
    let (mut e, _ctx) = setup_test_engine().unwrap();
    let id = e.create_node(vec!["T".to_string()], json!({})).unwrap();
    e.refresh_executor().unwrap();
    let r = execute_query(&mut e, "MATCH (n:T) RETURN id(n) AS i").unwrap();
    assert_eq!(r.rows[0].values[0], json!(id));
}

#[test]
#[serial]
fn neo4j_compat_06() {
    let (mut e, _ctx) = setup_test_engine().unwrap();
    let a = e.create_node(vec!["N".to_string()], json!({})).unwrap();
    let b = e.create_node(vec!["N".to_string()], json!({})).unwrap();
    e.refresh_executor().unwrap();
    e.create_relationship(a, b, "R".to_string(), json!({}))
        .unwrap();
    e.refresh_executor().unwrap();
    execute_query(&mut e, "MATCH ()-[r]->() RETURN type(r) AS t").unwrap();
}

#[test]
#[serial]
fn neo4j_compat_07() {
    let (mut e, _ctx) = setup_test_engine().unwrap();
    for _i in 0..3 {
        e.create_node(vec!["T".to_string()], json!({})).unwrap();
    }
    e.refresh_executor().unwrap();
    let r = execute_query(&mut e, "MATCH (n:T) RETURN count(n) AS c").unwrap();
    assert_eq!(r.rows[0].values[0], json!(3));
}

#[test]
#[serial]
fn neo4j_compat_08() {
    let (mut e, _ctx) = setup_test_engine().unwrap();
    e.create_node(vec!["T".to_string()], json!({"a": 1, "b": 2}))
        .unwrap();
    e.refresh_executor().unwrap();
    execute_query(&mut e, "MATCH (n:T) RETURN n.a AS a, n.b AS b").unwrap();
}

#[test]
#[serial]
fn neo4j_compat_10() {
    let (mut e, _ctx) = setup_test_engine().unwrap();
    for _i in 0..15 {
        e.create_node(vec!["T".to_string()], json!({})).unwrap();
    }
    e.refresh_executor().unwrap();
    let r = execute_query(&mut e, "MATCH (n:T) RETURN n LIMIT 10").unwrap();
    assert_eq!(r.rows.len(), 10);
}

#[test]
#[serial]
fn neo4j_test_11() {
    let (mut e, _ctx) = setup_test_engine().unwrap();
    e.create_node(vec!["T".to_string()], json!({})).unwrap();
    e.refresh_executor().unwrap();
    execute_query(&mut e, "MATCH (n:T) RETURN count(*) AS c").unwrap();
}

#[test]
#[serial]
fn neo4j_test_12() {
    let (mut e, _ctx) = setup_test_engine().unwrap();
    for _i in 0..7 {
        e.create_node(vec!["T".to_string()], json!({})).unwrap();
    }
    e.refresh_executor().unwrap();
    let r = execute_query(&mut e, "MATCH (n:T) RETURN count(*) AS c").unwrap();
    assert_eq!(r.rows[0].values[0], json!(7));
}

#[test]
#[serial]
fn neo4j_test_13() {
    let (mut e, _ctx) = setup_test_engine().unwrap();
    e.create_node(vec!["A".to_string()], json!({})).unwrap();
    e.create_node(vec!["B".to_string()], json!({})).unwrap();
    e.refresh_executor().unwrap();
    execute_query(&mut e, "MATCH (n) RETURN labels(n) AS l").unwrap();
}

#[test]
#[serial]
fn neo4j_test_14() {
    let (mut e, _ctx) = setup_test_engine().unwrap();
    e.create_node(vec!["T".to_string()], json!({"p": 1}))
        .unwrap();
    e.refresh_executor().unwrap();
    execute_query(&mut e, "MATCH (n:T) RETURN keys(n) AS k").unwrap();
}

#[test]
#[serial]
fn neo4j_test_15() {
    let (mut e, _ctx) = setup_test_engine().unwrap();
    e.create_node(vec!["T".to_string()], json!({})).unwrap();
    e.refresh_executor().unwrap();
    execute_query(&mut e, "MATCH (n:T) RETURN id(n) AS i").unwrap();
}

#[test]
#[serial]
fn neo4j_test_16() {
    let (mut e, _ctx) = setup_test_engine().unwrap();
    let a = e.create_node(vec!["N".to_string()], json!({})).unwrap();
    let b = e.create_node(vec!["N".to_string()], json!({})).unwrap();
    e.refresh_executor().unwrap();
    e.create_relationship(a, b, "R".to_string(), json!({}))
        .unwrap();
    e.refresh_executor().unwrap();
    execute_query(&mut e, "MATCH ()-[r:R]->() RETURN count(*) AS c").unwrap();
}

#[test]
#[serial]
fn neo4j_test_17() {
    let (mut e, _ctx) = setup_test_engine().unwrap();
    for _i in 0..12 {
        e.create_node(vec!["T".to_string()], json!({})).unwrap();
    }
    e.refresh_executor().unwrap();
    let r = execute_query(&mut e, "MATCH (n:T) RETURN count(*) AS c").unwrap();
    assert_eq!(r.rows[0].values[0], json!(12));
}

#[test]
#[serial]
fn neo4j_test_18() {
    let (mut e, _ctx) = setup_test_engine().unwrap();
    e.create_node(vec!["T1".to_string(), "T2".to_string()], json!({}))
        .unwrap();
    e.refresh_executor().unwrap();
    execute_query(&mut e, "MATCH (n:T1) RETURN count(*) AS c").unwrap();
}

#[test]
#[serial]
fn neo4j_test_19() {
    let (mut e, _ctx) = setup_test_engine().unwrap();
    for _i in 0..8 {
        e.create_node(vec!["T".to_string()], json!({"v": _i}))
            .unwrap();
    }
    e.refresh_executor().unwrap();
    execute_query(&mut e, "MATCH (n:T) WHERE n.v > 3 RETURN count(*) AS c").unwrap();
}

#[test]
#[serial]
fn neo4j_test_20() {
    let (mut e, _ctx) = setup_test_engine().unwrap();
    for _i in 0..6 {
        e.create_node(vec!["T".to_string()], json!({})).unwrap();
    }
    e.refresh_executor().unwrap();
    let r = execute_query(&mut e, "MATCH (n:T) RETURN n LIMIT 3").unwrap();
    assert_eq!(r.rows.len(), 3);
}

#[test]
#[serial]
fn neo4j_test_21() {
    let (mut e, _ctx) = setup_test_engine().unwrap();
    e.create_node(vec!["T".to_string()], json!({"name": "test"}))
        .unwrap();
    e.refresh_executor().unwrap();
    execute_query(&mut e, "MATCH (n:T {name: 'test'}) RETURN n").unwrap();
}

#[test]
#[serial]
fn neo4j_test_22() {
    let (mut e, _ctx) = setup_test_engine().unwrap();
    for _i in 0..4 {
        e.create_node(vec!["T".to_string()], json!({})).unwrap();
    }
    e.refresh_executor().unwrap();
    execute_query(&mut e, "MATCH (n:T) RETURN count(n) AS c").unwrap();
}

#[test]
#[serial]
fn neo4j_test_23() {
    let (mut e, _ctx) = setup_test_engine().unwrap();
    e.create_node(vec!["T".to_string()], json!({})).unwrap();
    e.refresh_executor().unwrap();
    execute_query(&mut e, "MATCH (n:T) RETURN labels(n) AS l, id(n) AS i").unwrap();
}

#[test]
#[serial]
fn neo4j_test_24() {
    let (mut e, _ctx) = setup_test_engine().unwrap();
    for _i in 0..9 {
        e.create_node(vec!["T".to_string()], json!({})).unwrap();
    }
    e.refresh_executor().unwrap();
    let r = execute_query(&mut e, "MATCH (n:T) RETURN count(*) AS c").unwrap();
    assert_eq!(r.rows[0].values[0], json!(9));
}

#[test]
#[serial]
fn neo4j_test_25() {
    let (mut e, _ctx) = setup_test_engine().unwrap();
    e.create_node(vec!["T".to_string()], json!({"v": true}))
        .unwrap();
    e.refresh_executor().unwrap();
    execute_query(&mut e, "MATCH (n:T) RETURN n.v AS v").unwrap();
}

#[test]
#[serial]
fn neo4j_test_26() {
    let (mut e, _ctx) = setup_test_engine().unwrap();
    e.create_node(vec!["T".to_string()], json!({"v": 3.15}))
        .unwrap();
    e.refresh_executor().unwrap();
    execute_query(&mut e, "MATCH (n:T) RETURN n.v AS v").unwrap();
}

#[test]
#[serial]
fn neo4j_test_27() {
    let (mut e, _ctx) = setup_test_engine().unwrap();
    for _i in 0..11 {
        e.create_node(vec!["T".to_string()], json!({})).unwrap();
    }
    e.refresh_executor().unwrap();
    let r = execute_query(&mut e, "MATCH (n:T) RETURN count(*) AS c").unwrap();
    assert_eq!(r.rows[0].values[0], json!(11));
}

#[test]
#[serial]
fn neo4j_test_29() {
    let (mut e, _ctx) = setup_test_engine().unwrap();
    let a = e.create_node(vec!["N".to_string()], json!({})).unwrap();
    let b = e.create_node(vec!["N".to_string()], json!({})).unwrap();
    e.refresh_executor().unwrap();
    e.create_relationship(a, b, "KNOWS".to_string(), json!({}))
        .unwrap();
    e.refresh_executor().unwrap();
    execute_query(&mut e, "MATCH (a)-[r:KNOWS]->(b) RETURN a, r, b").unwrap();
}

#[test]
#[serial]
fn neo4j_test_30() {
    let (mut e, _ctx) = setup_test_engine().unwrap();
    for _i in 0..13 {
        e.create_node(vec!["T".to_string()], json!({})).unwrap();
    }
    e.refresh_executor().unwrap();
    let r = execute_query(&mut e, "MATCH (n:T) RETURN count(*) AS c").unwrap();
    assert_eq!(r.rows[0].values[0], json!(13));
}

#[test]
#[serial]
fn neo4j_test_31() {
    let (mut e, _ctx) = setup_test_engine().unwrap();
    e.create_node(vec!["A".to_string()], json!({})).unwrap();
    e.create_node(vec!["B".to_string()], json!({})).unwrap();
    e.refresh_executor().unwrap();
    let _r = execute_query(&mut e, "MATCH (a:A), (b:B) RETURN a, b").ok();
}

#[test]
#[serial]
fn neo4j_test_32() {
    let (mut e, _ctx) = setup_test_engine().unwrap();
    e.create_node(vec!["T".to_string()], json!({"name": "Alice"}))
        .unwrap();
    e.refresh_executor().unwrap();
    execute_query(&mut e, "MATCH (n:T) RETURN n.name AS name").unwrap();
}

#[test]
#[serial]
fn neo4j_test_33() {
    let (mut e, _ctx) = setup_test_engine().unwrap();
    for _i in 0..16 {
        e.create_node(vec!["T".to_string()], json!({})).unwrap();
    }
    e.refresh_executor().unwrap();
    let r = execute_query(&mut e, "MATCH (n:T) RETURN count(*) AS c").unwrap();
    assert_eq!(r.rows[0].values[0], json!(16));
}

#[test]
#[serial]
fn neo4j_test_34() {
    let (mut e, _ctx) = setup_test_engine().unwrap();
    e.create_node(vec!["T".to_string()], json!({})).unwrap();
    e.refresh_executor().unwrap();
    execute_query(&mut e, "MATCH (n) RETURN count(*) AS c").unwrap();
}

#[test]
#[serial]
fn neo4j_test_35() {
    let (mut e, _ctx) = setup_test_engine().unwrap();
    for _i in 0..18 {
        e.create_node(vec!["T".to_string()], json!({})).unwrap();
    }
    e.refresh_executor().unwrap();
    let r = execute_query(&mut e, "MATCH (n:T) RETURN count(*) AS c").unwrap();
    assert_eq!(r.rows[0].values[0], json!(18));
}

#[test]
#[serial]
fn neo4j_test_36() {
    let (mut e, _ctx) = setup_test_engine().unwrap();
    e.create_node(vec!["T".to_string()], json!({"a": 1, "b": 2, "c": 3}))
        .unwrap();
    e.refresh_executor().unwrap();
    let r = execute_query(&mut e, "MATCH (n:T) RETURN keys(n) AS k").unwrap();
    assert!(!r.rows.is_empty());
}

#[test]
#[serial]
fn neo4j_test_37() {
    let (mut e, _ctx) = setup_test_engine().unwrap();
    for _i in 0..20 {
        e.create_node(vec!["T".to_string()], json!({})).unwrap();
    }
    e.refresh_executor().unwrap();
    let r = execute_query(&mut e, "MATCH (n:T) RETURN count(*) AS c").unwrap();
    assert_eq!(r.rows[0].values[0], json!(20));
}

#[test]
#[serial]
fn neo4j_test_38() {
    let (mut e, _ctx) = setup_test_engine().unwrap();
    e.create_node(
        vec!["A".to_string(), "B".to_string(), "C".to_string()],
        json!({}),
    )
    .unwrap();
    e.refresh_executor().unwrap();
    execute_query(&mut e, "MATCH (n:A:B:C) RETURN count(*) AS c").unwrap();
}

#[test]
#[serial]
fn neo4j_test_39() {
    let (mut e, _ctx) = setup_test_engine().unwrap();
    for _i in 0..22 {
        e.create_node(vec!["T".to_string()], json!({})).unwrap();
    }
    e.refresh_executor().unwrap();
    let r = execute_query(&mut e, "MATCH (n:T) RETURN count(*) AS c").unwrap();
    assert_eq!(r.rows[0].values[0], json!(22));
}

#[test]
#[serial]
fn neo4j_test_40() {
    let (mut e, _ctx) = setup_test_engine().unwrap();
    let a = e.create_node(vec!["N".to_string()], json!({})).unwrap();
    let b = e.create_node(vec!["N".to_string()], json!({})).unwrap();
    e.refresh_executor().unwrap();
    for _i in 0..5 {
        e.create_relationship(a, b, "R".to_string(), json!({}))
            .unwrap();
    }
    e.refresh_executor().unwrap();
    let r = execute_query(&mut e, "MATCH ()-[r:R]->() RETURN count(*) AS c").unwrap();
    assert_eq!(r.rows[0].values[0], json!(5));
}

#[test]
#[serial]
fn neo4j_test_41() {
    let (mut e, _ctx) = setup_test_engine().unwrap();
    for _i in 0..25 {
        e.create_node(vec!["T".to_string()], json!({})).unwrap();
    }
    e.refresh_executor().unwrap();
    let r = execute_query(&mut e, "MATCH (n:T) RETURN count(*) AS c").unwrap();
    assert_eq!(r.rows[0].values[0], json!(25));
}

#[test]
#[serial]
fn neo4j_test_42() {
    let (mut e, _ctx) = setup_test_engine().unwrap();
    e.create_node(vec!["T".to_string()], json!({"v": -100}))
        .unwrap();
    e.refresh_executor().unwrap();
    execute_query(&mut e, "MATCH (n:T) RETURN n.v AS v").unwrap();
}

#[test]
#[serial]
fn neo4j_test_43() {
    let (mut e, _ctx) = setup_test_engine().unwrap();
    for _i in 0..28 {
        e.create_node(vec!["T".to_string()], json!({})).unwrap();
    }
    e.refresh_executor().unwrap();
    let r = execute_query(&mut e, "MATCH (n:T) RETURN count(*) AS c").unwrap();
    assert_eq!(r.rows[0].values[0], json!(28));
}

#[test]
#[serial]
fn neo4j_test_44() {
    let (mut e, _ctx) = setup_test_engine().unwrap();
    e.create_node(vec!["T".to_string()], json!({"str": "test", "num": 42}))
        .unwrap();
    e.refresh_executor().unwrap();
    execute_query(&mut e, "MATCH (n:T) RETURN n.str AS s, n.num AS n").unwrap();
}

#[test]
#[serial]
fn neo4j_test_45() {
    let (mut e, _ctx) = setup_test_engine().unwrap();
    for _i in 0..30 {
        e.create_node(vec!["T".to_string()], json!({})).unwrap();
    }
    e.refresh_executor().unwrap();
    let r = execute_query(&mut e, "MATCH (n:T) RETURN count(*) AS c").unwrap();
    assert_eq!(r.rows[0].values[0], json!(30));
}

#[test]
#[serial]
fn neo4j_test_46() {
    let (mut e, _ctx) = setup_test_engine().unwrap();
    let a = e.create_node(vec!["N".to_string()], json!({})).unwrap();
    let b = e.create_node(vec!["N".to_string()], json!({})).unwrap();
    e.refresh_executor().unwrap();
    e.create_relationship(a, b, "R".to_string(), json!({"p": 1}))
        .unwrap();
    e.refresh_executor().unwrap();
    execute_query(&mut e, "MATCH ()-[r:R]->() RETURN r.p AS p").unwrap();
}

#[test]
#[serial]
fn neo4j_test_47() {
    let (mut e, _ctx) = setup_test_engine().unwrap();
    for _i in 0..35 {
        e.create_node(vec!["T".to_string()], json!({})).unwrap();
    }
    e.refresh_executor().unwrap();
    let r = execute_query(&mut e, "MATCH (n:T) RETURN count(*) AS c").unwrap();
    assert_eq!(r.rows[0].values[0], json!(35));
}

#[test]
#[serial]
fn neo4j_test_49() {
    let (mut e, _ctx) = setup_test_engine().unwrap();
    for _i in 0..40 {
        e.create_node(vec!["T".to_string()], json!({})).unwrap();
    }
    e.refresh_executor().unwrap();
    let r = execute_query(&mut e, "MATCH (n:T) RETURN count(*) AS c").unwrap();
    assert_eq!(r.rows[0].values[0], json!(40));
}

#[test]
#[serial]
fn neo4j_test_51() {
    let (mut e, _ctx) = setup_test_engine().unwrap();
    for _i in 0..50 {
        e.create_node(vec!["T".to_string()], json!({})).unwrap();
    }
    e.refresh_executor().unwrap();
    let r = execute_query(&mut e, "MATCH (n:T) RETURN count(*) AS c").unwrap();
    assert_eq!(r.rows[0].values[0], json!(50));
}

#[test]
#[serial]
fn neo4j_test_52() {
    let (mut e, _ctx) = setup_test_engine().unwrap();
    e.create_node(vec!["T".to_string()], json!({})).unwrap();
    e.refresh_executor().unwrap();
    execute_query(&mut e, "MATCH (n:T) RETURN n").unwrap();
}

#[test]
#[serial]
fn neo4j_test_53() {
    let (mut e, _ctx) = setup_test_engine().unwrap();
    for _i in 0..55 {
        e.create_node(vec!["T".to_string()], json!({})).unwrap();
    }
    e.refresh_executor().unwrap();
    let r = execute_query(&mut e, "MATCH (n:T) RETURN count(*) AS c").unwrap();
    assert_eq!(r.rows[0].values[0], json!(55));
}

#[test]
#[serial]
fn neo4j_test_54() {
    let (mut e, _ctx) = setup_test_engine().unwrap();
    let a = e.create_node(vec!["N".to_string()], json!({})).unwrap();
    let b = e.create_node(vec!["N".to_string()], json!({})).unwrap();
    e.refresh_executor().unwrap();
    e.create_relationship(a, b, "R".to_string(), json!({}))
        .unwrap();
    e.create_relationship(b, a, "R".to_string(), json!({}))
        .unwrap();
    e.refresh_executor().unwrap();
    let r = execute_query(&mut e, "MATCH ()-[r:R]->() RETURN count(*) AS c").unwrap();
    assert_eq!(r.rows[0].values[0], json!(2));
}

#[test]
#[serial]
fn neo4j_test_55() {
    let (mut e, _ctx) = setup_test_engine().unwrap();
    for _i in 0..60 {
        e.create_node(vec!["T".to_string()], json!({})).unwrap();
    }
    e.refresh_executor().unwrap();
    let r = execute_query(&mut e, "MATCH (n:T) RETURN count(*) AS c").unwrap();
    assert_eq!(r.rows[0].values[0], json!(60));
}

#[test]
#[serial]
fn neo4j_test_56() {
    let (mut e, _ctx) = setup_test_engine().unwrap();
    e.create_node(vec!["T".to_string()], json!({"v": 0}))
        .unwrap();
    e.refresh_executor().unwrap();
    execute_query(&mut e, "MATCH (n:T) WHERE n.v = 0 RETURN n").unwrap();
}

#[test]
#[serial]
fn neo4j_test_57() {
    let (mut e, _ctx) = setup_test_engine().unwrap();
    for _i in 0..65 {
        e.create_node(vec!["T".to_string()], json!({})).unwrap();
    }
    e.refresh_executor().unwrap();
    let r = execute_query(&mut e, "MATCH (n:T) RETURN count(*) AS c").unwrap();
    assert_eq!(r.rows[0].values[0], json!(65));
}

#[test]
#[serial]
fn neo4j_test_58() {
    let (mut e, _ctx) = setup_test_engine().unwrap();
    e.create_node(vec!["T".to_string()], json!({})).unwrap();
    e.refresh_executor().unwrap();
    execute_query(
        &mut e,
        "MATCH (n:T) RETURN id(n) AS id, labels(n) AS labels",
    )
    .unwrap();
}

#[test]
#[serial]
fn neo4j_test_59() {
    let (mut e, _ctx) = setup_test_engine().unwrap();
    for _i in 0..70 {
        e.create_node(vec!["T".to_string()], json!({})).unwrap();
    }
    e.refresh_executor().unwrap();
    let r = execute_query(&mut e, "MATCH (n:T) RETURN count(*) AS c").unwrap();
    assert_eq!(r.rows[0].values[0], json!(70));
}

#[test]
#[serial]
fn neo4j_test_60() {
    let (mut e, _ctx) = setup_test_engine().unwrap();
    let nodes: Vec<u64> = (0..5)
        .map(|_| e.create_node(vec!["N".to_string()], json!({})).unwrap())
        .collect();
    e.refresh_executor().unwrap();
    for i in 0..4 {
        e.create_relationship(nodes[i], nodes[i + 1], "NEXT".to_string(), json!({}))
            .unwrap();
    }
    e.refresh_executor().unwrap();
    execute_query(&mut e, "MATCH (a)-[r:NEXT]->(b) RETURN count(*) AS c").unwrap();
}

#[test]
#[serial]
fn neo4j_test_62() {
    let (mut e, _ctx) = setup_test_engine().unwrap();
    e.create_node(
        vec!["T".to_string()],
        json!({"name": "test", "active": true}),
    )
    .unwrap();
    e.refresh_executor().unwrap();
    execute_query(
        &mut e,
        "MATCH (n:T) RETURN n.name AS name, n.active AS active",
    )
    .unwrap();
}

#[test]
#[serial]
fn neo4j_test_63() {
    let (mut e, _ctx) = setup_test_engine().unwrap();
    for _i in 0..80 {
        e.create_node(vec!["T".to_string()], json!({})).unwrap();
    }
    e.refresh_executor().unwrap();
    let r = execute_query(&mut e, "MATCH (n:T) RETURN count(*) AS c").unwrap();
    assert_eq!(r.rows[0].values[0], json!(80));
}

#[test]
#[serial]
fn neo4j_test_64() {
    let (mut e, _ctx) = setup_test_engine().unwrap();
    for _i in 0..3 {
        e.create_node(vec![format!("Label{}", _i)], json!({}))
            .unwrap();
    }
    e.refresh_executor().unwrap();
    execute_query(&mut e, "MATCH (n) RETURN labels(n) AS l").unwrap();
}

#[test]
#[serial]
fn neo4j_test_65() {
    let (mut e, _ctx) = setup_test_engine().unwrap();
    for _i in 0..85 {
        e.create_node(vec!["T".to_string()], json!({})).unwrap();
    }
    e.refresh_executor().unwrap();
    let r = execute_query(&mut e, "MATCH (n:T) RETURN count(*) AS c").unwrap();
    assert_eq!(r.rows[0].values[0], json!(85));
}

#[test]
#[serial]
fn neo4j_test_67() {
    let (mut e, _ctx) = setup_test_engine().unwrap();
    for _i in 0..90 {
        e.create_node(vec!["T".to_string()], json!({})).unwrap();
    }
    e.refresh_executor().unwrap();
    let r = execute_query(&mut e, "MATCH (n:T) RETURN count(*) AS c").unwrap();
    assert_eq!(r.rows[0].values[0], json!(90));
}

#[test]
#[serial]
fn neo4j_test_68() {
    let (mut e, _ctx) = setup_test_engine().unwrap();
    e.create_node(vec!["T".to_string()], json!({"v": false}))
        .unwrap();
    e.refresh_executor().unwrap();
    execute_query(&mut e, "MATCH (n:T) RETURN n.v AS v").unwrap();
}

#[test]
#[serial]
fn neo4j_test_69() {
    let (mut e, _ctx) = setup_test_engine().unwrap();
    for _i in 0..95 {
        e.create_node(vec!["T".to_string()], json!({})).unwrap();
    }
    e.refresh_executor().unwrap();
    let r = execute_query(&mut e, "MATCH (n:T) RETURN count(*) AS c").unwrap();
    assert_eq!(r.rows[0].values[0], json!(95));
}

#[test]
#[serial]
fn neo4j_test_70() {
    let (mut e, _ctx) = setup_test_engine().unwrap();
    for _i in 0..100 {
        e.create_node(vec!["T".to_string()], json!({})).unwrap();
    }
    e.refresh_executor().unwrap();
    let r = execute_query(&mut e, "MATCH (n:T) RETURN count(*) AS c").unwrap();
    assert_eq!(r.rows[0].values[0], json!(100));
}

#[test]
#[serial]
fn neo4j_test_71() {
    let (mut e, _ctx) = setup_test_engine().unwrap();
    e.create_node(vec!["T".to_string()], json!({"name": "Bob"}))
        .unwrap();
    e.create_node(vec!["T".to_string()], json!({"name": "Alice"}))
        .unwrap();
    e.refresh_executor().unwrap();
    execute_query(&mut e, "MATCH (n:T) RETURN n.name AS name ORDER BY n.name").unwrap();
}

#[test]
#[serial]
fn neo4j_test_72() {
    let (mut e, _ctx) = setup_test_engine().unwrap();
    e.create_node(vec!["T".to_string()], json!({})).unwrap();
    e.refresh_executor().unwrap();
    let _r = execute_query(&mut e, "MATCH (n:T) RETURN count(n) AS c, id(n) AS i").ok();
}

#[test]
#[serial]
fn neo4j_test_73() {
    let (mut e, _ctx) = setup_test_engine().unwrap();
    for _i in 0..15 {
        e.create_node(vec!["T".to_string()], json!({"value": _i}))
            .unwrap();
    }
    e.refresh_executor().unwrap();
    execute_query(
        &mut e,
        "MATCH (n:T) WHERE n.value < 10 RETURN count(*) AS c",
    )
    .unwrap();
}

#[test]
#[serial]
fn neo4j_test_74() {
    let (mut e, _ctx) = setup_test_engine().unwrap();
    let a = e
        .create_node(vec!["Person".to_string()], json!({"name": "Alice"}))
        .unwrap();
    let b = e
        .create_node(vec!["Company".to_string()], json!({"name": "Acme"}))
        .unwrap();
    e.refresh_executor().unwrap();
    e.create_relationship(a, b, "WORKS_AT".to_string(), json!({}))
        .unwrap();
    e.refresh_executor().unwrap();
    execute_query(
        &mut e,
        "MATCH (p:Person)-[r:WORKS_AT]->(c:Company) RETURN p.name AS person, c.name AS company",
    )
    .unwrap();
}

#[test]
#[serial]
fn neo4j_test_75() {
    let (mut e, _ctx) = setup_test_engine().unwrap();
    e.create_node(vec!["T".to_string()], json!({"v": 1}))
        .unwrap();
    e.create_node(vec!["T".to_string()], json!({"v": 2}))
        .unwrap();
    e.create_node(vec!["T".to_string()], json!({"v": 3}))
        .unwrap();
    e.refresh_executor().unwrap();
    execute_query(&mut e, "MATCH (n:T) RETURN DISTINCT n.v AS v").unwrap();
}
