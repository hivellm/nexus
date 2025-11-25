//! Detailed profiling benchmark for CREATE operations
//! This benchmark helps identify performance bottlenecks

#[cfg(test)]
mod tests {
    use nexus_core::Engine;
    use std::time::{Duration, Instant};
    use tracing;

    #[test]
    #[ignore = "Slow benchmark test - run explicitly with cargo test -- --ignored"]
    fn profile_create_node_breakdown() {
        let mut engine = Engine::new().unwrap();

        // Warm up
        for _ in 0..10 {
            engine
                .execute_cypher("CREATE (n:Person {name: 'Test'})")
                .unwrap();
        }

        let iterations = 100;
        let mut total_parse = Duration::ZERO;
        let mut total_execute = Duration::ZERO;
        let mut total_commit = Duration::ZERO;
        let mut total_flush = Duration::ZERO;

        for i in 0..iterations {
            let query = format!(
                "CREATE (n:Person {{name: 'Person{}', age: {}}})",
                i,
                i % 100
            );

            let start = Instant::now();
            let parse_start = Instant::now();
            // Parse is done internally, we measure total time
            let parse_time = parse_start.elapsed();
            total_parse += parse_time;

            let execute_start = Instant::now();
            let result = engine.execute_cypher(&query).unwrap();
            let execute_time = execute_start.elapsed();
            total_execute += execute_time;

            // Estimate commit/flush time (part of execute)
            let total_time = start.elapsed();
            if total_time > execute_time {
                let overhead = total_time - execute_time;
                total_commit += overhead;
            }
        }

        tracing::info!(
            "\n=== CREATE Node Profiling ({} iterations) ===",
            iterations
        );
        tracing::info!(
            "Average parse time: {:.2}μs",
            total_parse.as_micros() as f64 / iterations as f64
        );
        tracing::info!(
            "Average execute time: {:.2}ms",
            total_execute.as_millis() as f64 / iterations as f64
        );
        tracing::info!(
            "Average commit/flush overhead: {:.2}μs",
            total_commit.as_micros() as f64 / iterations as f64
        );
        tracing::info!(
            "Total average: {:.2}ms",
            (total_parse + total_execute + total_commit).as_millis() as f64 / iterations as f64
        );
    }

    #[test]
    #[ignore = "Slow benchmark test - run explicitly with cargo test -- --ignored"]
    fn profile_create_relationship_breakdown() {
        let mut engine = Engine::new().unwrap();

        // Create nodes first
        for i in 0..100 {
            engine
                .execute_cypher(&format!("CREATE (n:Person {{id: {}}})", i))
                .unwrap();
        }

        // Warm up
        for _ in 0..10 {
            engine
                .execute_cypher(
                    "MATCH (a:Person {id: 0}), (b:Person {id: 1}) CREATE (a)-[:KNOWS]->(b)",
                )
                .unwrap();
        }

        let iterations = 100;
        let mut total_time = Duration::ZERO;

        for i in 0..iterations {
            let query = format!(
                "MATCH (a:Person {{id: {}}}), (b:Person {{id: {}}}) CREATE (a)-[:KNOWS {{since: {}}}]->(b)",
                i % 50,
                (i + 1) % 50,
                i
            );

            let start = Instant::now();
            engine.execute_cypher(&query).unwrap();
            total_time += start.elapsed();
        }

        let avg = total_time.as_millis() as f64 / iterations as f64;
        tracing::info!(
            "\n=== CREATE Relationship Profiling ({} iterations) ===",
            iterations
        );
        tracing::info!("Average time: {:.2}ms", avg);
    }

    #[test]
    #[ignore = "Slow benchmark test - run explicitly with cargo test -- --ignored"]
    fn profile_property_serialization() {
        use serde_json::Value;
        use std::collections::HashMap;

        let iterations = 10000;
        let mut total_to_vec = Duration::ZERO;
        let mut total_to_string = Duration::ZERO;
        let mut total_to_writer = Duration::ZERO;

        for i in 0..iterations {
            let mut props = HashMap::new();
            props.insert("name".to_string(), Value::String(format!("Person{}", i)));
            props.insert("age".to_string(), Value::Number((i % 100).into()));
            props.insert("active".to_string(), Value::Bool(i % 2 == 0));
            let json = Value::Object(serde_json::Map::from_iter(props));

            // Test to_vec
            let start = Instant::now();
            let _ = serde_json::to_vec(&json).unwrap();
            total_to_vec += start.elapsed();

            // Test to_string
            let start = Instant::now();
            let _ = serde_json::to_string(&json).unwrap();
            total_to_string += start.elapsed();

            // Test to_writer
            let start = Instant::now();
            let mut buffer = Vec::new();
            serde_json::to_writer(&mut buffer, &json).unwrap();
            total_to_writer += start.elapsed();
        }

        tracing::info!(
            "\n=== Property Serialization Profiling ({} iterations) ===",
            iterations
        );
        tracing::info!(
            "Average to_vec: {:.2}μs",
            total_to_vec.as_micros() as f64 / iterations as f64
        );
        tracing::info!(
            "Average to_string: {:.2}μs",
            total_to_string.as_micros() as f64 / iterations as f64
        );
        tracing::info!(
            "Average to_writer: {:.2}μs",
            total_to_writer.as_micros() as f64 / iterations as f64
        );
    }
}
