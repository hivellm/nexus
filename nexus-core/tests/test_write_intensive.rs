//! Write-Intensive Tests
//!
//! Tests for high-concurrency write operations to verify performance and consistency
//! under heavy write loads (Phase 1 Week 5).

use nexus_core::Engine;
use tempfile::TempDir;

/// Helper function to extract count from result
fn extract_count(result: nexus_core::executor::ResultSet) -> u64 {
    result
        .rows
        .first()
        .and_then(|row| row.values.first())
        .and_then(|v| v.as_u64())
        .unwrap_or(0)
}

/// Test 1000 concurrent CREATE operations
#[tokio::test]
async fn test_1000_concurrent_create_operations() {
    let dir = TempDir::new().unwrap();
    let engine = Engine::with_data_dir(dir.path()).unwrap();
    let engine = std::sync::Arc::new(std::sync::Mutex::new(engine));

    let num_operations = 1000;
    let mut handles = vec![];

    // Create 1000 nodes concurrently
    for i in 0..num_operations {
        let engine_clone = engine.clone();
        let handle = tokio::spawn(async move {
            let mut engine = engine_clone.lock().unwrap();
            let query = format!(
                "CREATE (n:Person {{id: {}, name: 'Person{}', age: {}}})",
                i,
                i,
                20 + (i % 50)
            );
            engine.execute_cypher(&query).unwrap();
        });
        handles.push(handle);
    }

    // Wait for all operations to complete
    for handle in handles {
        handle.await.unwrap();
    }

    // Verify all nodes were created
    let mut engine = engine.lock().unwrap();
    let result = engine
        .execute_cypher("MATCH (n:Person) RETURN count(n) as count")
        .unwrap();
    let count = extract_count(result);
    assert_eq!(
        count, num_operations as u64,
        "Should have created {} Person nodes",
        num_operations
    );
}

/// Test relationship creation throughput
#[tokio::test]
async fn test_relationship_creation_throughput() {
    let dir = TempDir::new().unwrap();
    let mut engine = Engine::with_data_dir(dir.path()).unwrap();

    // Create base nodes first
    engine.execute_cypher("BEGIN TRANSACTION").unwrap();
    for i in 0..100 {
        let query = format!("CREATE (n:Person {{id: {}, name: 'Person{}'}})", i, i);
        engine.execute_cypher(&query).unwrap();
    }
    engine.execute_cypher("COMMIT TRANSACTION").unwrap();

    // Now create relationships concurrently
    let engine = std::sync::Arc::new(std::sync::Mutex::new(engine));
    let num_relationships = 500;
    let mut handles = vec![];

    for i in 0..num_relationships {
        let engine_clone = engine.clone();
        let handle = tokio::spawn(async move {
            let mut engine = engine_clone.lock().unwrap();
            let source = i % 100;
            let target = (i + 1) % 100;
            let query = format!(
                "MATCH (a:Person {{id: {}}}), (b:Person {{id: {}}}) CREATE (a)-[:KNOWS]->(b)",
                source, target
            );
            engine.execute_cypher(&query).unwrap();
        });
        handles.push(handle);
    }

    // Wait for all relationships to be created
    for handle in handles {
        handle.await.unwrap();
    }

    // Verify relationships were created
    let mut engine = engine.lock().unwrap();
    let result = engine
        .execute_cypher("MATCH ()-[r:KNOWS]->() RETURN count(r) as count")
        .unwrap();
    let count = extract_count(result);
    assert!(
        count >= num_relationships as u64,
        "Should have created at least {} KNOWS relationships",
        num_relationships
    );
}

/// Test write + read mixed workload
#[tokio::test]
async fn test_write_read_mixed_workload() {
    let dir = TempDir::new().unwrap();
    let engine = Engine::with_data_dir(dir.path()).unwrap();
    let engine = std::sync::Arc::new(std::sync::Mutex::new(engine));

    let num_writers = 10;
    let num_readers = 20;
    let writes_per_writer = 50;
    let reads_per_reader = 100;

    let mut handles = vec![];

    // Spawn writers
    for writer_id in 0..num_writers {
        let engine_clone = engine.clone();
        let handle = tokio::spawn(async move {
            let mut engine = engine_clone.lock().unwrap();
            for i in 0..writes_per_writer {
                let node_id = writer_id * writes_per_writer + i;
                let query = format!(
                    "CREATE (n:Person {{id: {}, name: 'Person{}', writer: {}}})",
                    node_id, node_id, writer_id
                );
                engine.execute_cypher(&query).unwrap();
            }
        });
        handles.push(handle);
    }

    // Spawn readers (they will read while writes are happening)
    for reader_id in 0..num_readers {
        let engine_clone = engine.clone();
        let handle = tokio::spawn(async move {
            let mut engine = engine_clone.lock().unwrap();
            for _ in 0..reads_per_reader {
                let query = format!(
                    "MATCH (n:Person) WHERE n.writer = {} RETURN count(n) as count",
                    reader_id % num_writers
                );
                let result = engine.execute_cypher(&query).unwrap();
                let _count = extract_count(result);
                // Just verify query executed successfully
            }
        });
        handles.push(handle);
    }

    // Wait for all operations to complete
    for handle in handles {
        handle.await.unwrap();
    }

    // Verify data consistency
    let mut engine = engine.lock().unwrap();
    let result = engine
        .execute_cypher("MATCH (n:Person) RETURN count(n) as count")
        .unwrap();
    let count = extract_count(result);
    assert_eq!(
        count,
        (num_writers * writes_per_writer) as u64,
        "Should have {} Person nodes from writers",
        num_writers * writes_per_writer
    );
}

/// Test data consistency after concurrent writes
#[tokio::test]
async fn test_data_consistency_after_concurrent_writes() {
    let dir = TempDir::new().unwrap();
    let engine = Engine::with_data_dir(dir.path()).unwrap();
    let engine = std::sync::Arc::new(std::sync::Mutex::new(engine));

    let num_threads = 20;
    let nodes_per_thread = 50;
    let mut handles = vec![];

    // Each thread creates nodes with unique IDs
    for thread_id in 0..num_threads {
        let engine_clone = engine.clone();
        let handle = tokio::spawn(async move {
            let mut engine = engine_clone.lock().unwrap();
            engine.execute_cypher("BEGIN TRANSACTION").unwrap();

            for i in 0..nodes_per_thread {
                let node_id = thread_id * nodes_per_thread + i;
                let query = format!(
                    "CREATE (n:Person {{id: {}, thread: {}, name: 'Person{}'}})",
                    node_id, thread_id, node_id
                );
                engine.execute_cypher(&query).unwrap();
            }

            engine.execute_cypher("COMMIT TRANSACTION").unwrap();
        });
        handles.push(handle);
    }

    // Wait for all threads to complete
    for handle in handles {
        handle.await.unwrap();
    }

    // Verify data consistency
    let mut engine = engine.lock().unwrap();

    // Check total count
    let result = engine
        .execute_cypher("MATCH (n:Person) RETURN count(n) as count")
        .unwrap();
    let total_count = extract_count(result);
    assert_eq!(
        total_count,
        (num_threads * nodes_per_thread) as u64,
        "Should have {} total Person nodes",
        num_threads * nodes_per_thread
    );

    // Check that each thread's nodes exist
    for thread_id in 0..num_threads {
        let query = format!(
            "MATCH (n:Person) WHERE n.thread = {} RETURN count(n) as count",
            thread_id
        );
        let result = engine.execute_cypher(&query).unwrap();
        let count = extract_count(result);
        assert_eq!(
            count, nodes_per_thread as u64,
            "Thread {} should have created {} nodes",
            thread_id, nodes_per_thread
        );
    }

    // Check for duplicate IDs (should not exist)
    let result = engine
        .execute_cypher(
            "MATCH (n:Person)
             WITH n.id as id, count(*) as cnt
             WHERE cnt > 1
             RETURN count(*) as duplicates",
        )
        .unwrap();
    let duplicates = extract_count(result);
    assert_eq!(
        duplicates, 0,
        "Should have no duplicate node IDs, found {}",
        duplicates
    );
}
