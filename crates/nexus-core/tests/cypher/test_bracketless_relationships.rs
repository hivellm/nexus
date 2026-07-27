#![allow(unused_mut)] // test fixtures declare `mut` preemptively

use nexus_core::executor::Query;
use nexus_core::testing::create_isolated_test_executor;
use serde_json::Value;
use std::collections::HashMap;

#[test]
fn test_bracketless_outgoing_relationship_matches() {
    let (mut executor, _ctx) = create_isolated_test_executor();

    let query = Query {
        cypher: "CREATE (a:Person {name: 'Alice'}), (b:Person {name: 'Bob'})".to_string(),
        params: HashMap::new(),
    };
    executor.execute(&query).unwrap();

    let query = Query {
        cypher:
            "MATCH (a:Person {name: 'Alice'}), (b:Person {name: 'Bob'}) CREATE (a)-[:KNOWS]->(b)"
                .to_string(),
        params: HashMap::new(),
    };
    executor.execute(&query).unwrap();

    // Bracket-less outgoing relationship: `-->`.
    let query = Query {
        cypher: "MATCH (a:Person {name: 'Alice'})-->(b) RETURN b.name AS name".to_string(),
        params: HashMap::new(),
    };
    let result = executor.execute(&query).unwrap();

    assert_eq!(result.rows.len(), 1);
    assert_eq!(result.rows[0].values[0], Value::String("Bob".to_string()));
}

#[test]
fn test_bracketless_undirected_relationship_matches_both_directions() {
    let (mut executor, _ctx) = create_isolated_test_executor();

    let query = Query {
        cypher: "CREATE (a:Person {name: 'Alice'}), (b:Person {name: 'Bob'})".to_string(),
        params: HashMap::new(),
    };
    executor.execute(&query).unwrap();

    let query = Query {
        cypher:
            "MATCH (a:Person {name: 'Alice'}), (b:Person {name: 'Bob'}) CREATE (a)-[:KNOWS]->(b)"
                .to_string(),
        params: HashMap::new(),
    };
    executor.execute(&query).unwrap();

    // Bracket-less undirected relationship: `--` must traverse both ways.
    let query = Query {
        cypher: "MATCH (a:Person {name: 'Bob'})--(b) RETURN b.name AS name".to_string(),
        params: HashMap::new(),
    };
    let result = executor.execute(&query).unwrap();

    assert_eq!(result.rows.len(), 1);
    assert_eq!(result.rows[0].values[0], Value::String("Alice".to_string()));
}

#[test]
fn test_bracketless_incoming_relationship_matches() {
    let (mut executor, _ctx) = create_isolated_test_executor();

    let query = Query {
        cypher: "CREATE (a:Person {name: 'Alice'}), (b:Person {name: 'Bob'})".to_string(),
        params: HashMap::new(),
    };
    executor.execute(&query).unwrap();

    let query = Query {
        cypher:
            "MATCH (a:Person {name: 'Alice'}), (b:Person {name: 'Bob'}) CREATE (a)-[:KNOWS]->(b)"
                .to_string(),
        params: HashMap::new(),
    };
    executor.execute(&query).unwrap();

    // Bracket-less incoming relationship: `<--`.
    let query = Query {
        cypher: "MATCH (b:Person {name: 'Bob'})<--(a) RETURN a.name AS name".to_string(),
        params: HashMap::new(),
    };
    let result = executor.execute(&query).unwrap();

    assert_eq!(result.rows.len(), 1);
    assert_eq!(result.rows[0].values[0], Value::String("Alice".to_string()));
}

#[test]
fn test_relationship_type_alternation_colon_prefixed_matches() {
    let (mut executor, _ctx) = create_isolated_test_executor();

    let query = Query {
        cypher: "CREATE (a:Person {name: 'Alice'}), (b:Person {name: 'Bob'}), (c:Company {name: 'TechCorp'})".to_string(),
        params: HashMap::new(),
    };
    executor.execute(&query).unwrap();

    let query = Query {
        cypher:
            "MATCH (a:Person {name: 'Alice'}), (b:Person {name: 'Bob'}) CREATE (a)-[:KNOWS]->(b)"
                .to_string(),
        params: HashMap::new(),
    };
    executor.execute(&query).unwrap();

    let query = Query {
        cypher: "MATCH (a:Person {name: 'Alice'}), (c:Company {name: 'TechCorp'}) CREATE (a)-[:WORKS_AT]->(c)".to_string(),
        params: HashMap::new(),
    };
    executor.execute(&query).unwrap();

    // Colon-prefixed alternation `[:A|:B]` must match the same as `[:A|B]`.
    let query = Query {
        cypher: "MATCH (a:Person {name: 'Alice'})-[r:KNOWS|:WORKS_AT]->(b) RETURN count(r) AS cnt"
            .to_string(),
        params: HashMap::new(),
    };
    let result = executor.execute(&query).unwrap();

    assert_eq!(result.rows.len(), 1);
    assert_eq!(
        result.rows[0].values[0],
        Value::Number(serde_json::Number::from(2))
    );
}
