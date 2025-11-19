//! Benchmark for relationship traversal performance (Phase 3)
//!
//! Tests adjacency list performance for:
//! - Single-hop traversal
//! - Relationship count
//! - Filtered traversal by type

#[cfg(test)]
mod tests {
    use nexus_core::Engine;
    use std::time::Instant;
    use tempfile::TempDir;
    use tracing;

    #[test]
    #[ignore = "Slow benchmark test - run explicitly with cargo test -- --ignored"]
    fn benchmark_relationship_traversal() {
        tracing::info!("=== Phase 3: Relationship Traversal Benchmark ===\n");

        let dir = TempDir::new().unwrap();
        let mut engine = Engine::with_data_dir(dir.path()).unwrap();

        // Create test data
        tracing::info!("Creating test data...");
        let create_start = Instant::now();

        // Create 1000 nodes using Cypher
        let mut node_ids = Vec::new();
        for i in 0..1000 {
            let query = format!("CREATE (n:Person {{id: {}}}) RETURN id(n) as id", i);
            let result = engine.execute_cypher(&query).unwrap();
            if let Some(row) = result.rows.first() {
                if let Some(serde_json::Value::Number(id)) = row.values.first() {
                    node_ids.push(id.as_u64().unwrap());
                }
            }
        }

        // Create relationships: each node has 10 outgoing relationships
        // Mix of 3 different relationship types
        let mut rel_count = 0;
        for i in 0..1000 {
            let from = node_ids[i];
            for j in 0..10 {
                let to = node_ids[(i + j + 1) % 1000];
                let type_name = format!("REL_TYPE_{}", (j % 3) + 1);
                let query = format!(
                    "MATCH (a), (b) WHERE id(a) = {} AND id(b) = {} CREATE (a)-[r:{}]->(b) RETURN id(r) as id",
                    from, to, type_name
                );
                engine.execute_cypher(&query).unwrap();
                rel_count += 1;
            }
        }

        let create_time = create_start.elapsed();
        tracing::info!(
            "Created {} nodes and {} relationships in {:?}\n",
            1000,
            rel_count,
            create_time
        );

        // Benchmark 1: Single-hop traversal (all relationships)
        tracing::info!("=== Benchmark 1: Single-hop traversal (all relationships) ===");
        let mut times = Vec::new();
        for _ in 0..100 {
            let start = Instant::now();
            let _relationships = engine
                .execute_cypher("MATCH (n:Person)-[r]->(m) RETURN n, r, m LIMIT 1000")
                .unwrap();
            let elapsed = start.elapsed();
            times.push(elapsed.as_millis() as f64);
        }
        let avg = times.iter().sum::<f64>() / times.len() as f64;
        let min = times.iter().fold(f64::INFINITY, |a, &b| a.min(b));
        let max = times.iter().fold(0.0f64, |a, &b| a.max(b));
        tracing::info!("Average: {:.2}ms", avg);
        tracing::info!("Min: {:.2}ms", min);
        tracing::info!("Max: {:.2}ms", max);
        tracing::info!("Target: ≤ 3.5ms average\n");

        // Benchmark 2: Relationship count
        tracing::info!("=== Benchmark 2: Relationship count ===");
        let mut times = Vec::new();
        for _ in 0..100 {
            let start = Instant::now();
            let _result = engine
                .execute_cypher("MATCH (n:Person)-[r]->() RETURN count(r) as count")
                .unwrap();
            let elapsed = start.elapsed();
            times.push(elapsed.as_millis() as f64);
        }
        let avg = times.iter().sum::<f64>() / times.len() as f64;
        let min = times.iter().fold(f64::INFINITY, |a, &b| a.min(b));
        let max = times.iter().fold(0.0f64, |a, &b| a.max(b));
        tracing::info!("Average: {:.2}ms", avg);
        tracing::info!("Min: {:.2}ms", min);
        tracing::info!("Max: {:.2}ms", max);
        tracing::info!("Target: ≤ 2ms average\n");

        // Benchmark 3: Filtered traversal by type
        tracing::info!("=== Benchmark 3: Filtered traversal by type ===");
        let mut times = Vec::new();
        for _ in 0..100 {
            let start = Instant::now();
            let _relationships = engine
                .execute_cypher("MATCH (n:Person)-[r:REL_TYPE_1]->(m) RETURN n, r, m LIMIT 500")
                .unwrap();
            let elapsed = start.elapsed();
            times.push(elapsed.as_millis() as f64);
        }
        let avg = times.iter().sum::<f64>() / times.len() as f64;
        let min = times.iter().fold(f64::INFINITY, |a, &b| a.min(b));
        let max = times.iter().fold(0.0f64, |a, &b| a.max(b));
        tracing::info!("Average: {:.2}ms", avg);
        tracing::info!("Min: {:.2}ms", min);
        tracing::info!("Max: {:.2}ms", max);
        tracing::info!("Target: ≤ 3.5ms average\n");

        // Benchmark 4: High-degree node traversal
        tracing::info!("=== Benchmark 4: High-degree node traversal ===");
        // Create a node with many relationships
        let result = engine
            .execute_cypher("CREATE (n:Person {name: 'high_degree'}) RETURN id(n) as id")
            .unwrap();
        let high_degree_node = if let Some(row) = result.rows.first() {
            if let Some(serde_json::Value::Number(id)) = row.values.first() {
                id.as_u64().unwrap()
            } else {
                0
            }
        } else {
            0
        };

        for i in 0..1000 {
            let to = node_ids[i % 1000];
            let query = format!(
                "MATCH (a), (b) WHERE id(a) = {} AND id(b) = {} CREATE (a)-[r:REL_TYPE_1]->(b)",
                high_degree_node, to
            );
            engine.execute_cypher(&query).unwrap();
        }

        let mut times = Vec::new();
        for _ in 0..50 {
            let start = Instant::now();
            let _relationships = engine
                .execute_cypher(&format!(
                    "MATCH (n)-[r]->(m) WHERE id(n) = {} RETURN count(r) as count",
                    high_degree_node
                ))
                .unwrap();
            let elapsed = start.elapsed();
            times.push(elapsed.as_millis() as f64);
        }
        let avg = times.iter().sum::<f64>() / times.len() as f64;
        let min = times.iter().fold(f64::INFINITY, |a, &b| a.min(b));
        let max = times.iter().fold(0.0f64, |a, &b| a.max(b));
        tracing::info!("Average: {:.2}ms", avg);
        tracing::info!("Min: {:.2}ms", min);
        tracing::info!("Max: {:.2}ms", max);
        tracing::info!("Target: ≤ 5ms average for 1000 relationships\n");

        tracing::info!("=== Benchmark Complete ===");
    }
}
