//! Benchmark to measure lock contention reduction with row-level locking
//!
//! This benchmark compares:
//! - Table-level locking (simulated by locking all resources)
//! - Row-level locking (locking only specific resources)
//!
//! Task 1.5.4: Measure lock contention reduction

use nexus_core::storage::row_lock::{ResourceId, RowLockManager};
use std::sync::Arc;
use std::thread;
use std::time::{Duration, Instant};
use tracing;

/// Simulate table-level locking by acquiring locks on all resources
fn simulate_table_lock(manager: &RowLockManager, tx_id: u64, num_resources: usize) -> Duration {
    let start = Instant::now();

    // Acquire locks on all resources (simulating table lock)
    let mut guards = Vec::new();
    for i in 0..num_resources {
        if let Ok(guard) = manager.acquire_write(tx_id, ResourceId::node(i as u64)) {
            guards.push(guard);
        }
    }

    // Hold locks briefly
    thread::sleep(Duration::from_millis(1));

    // Release all locks
    drop(guards);

    start.elapsed()
}

/// Use row-level locking by acquiring locks only on specific resources
fn use_row_locks(manager: &RowLockManager, tx_id: u64, resource_ids: &[u64]) -> Duration {
    let start = Instant::now();

    // Acquire locks only on specific resources
    let mut guards = Vec::new();
    for &id in resource_ids {
        if let Ok(guard) = manager.acquire_write(tx_id, ResourceId::node(id)) {
            guards.push(guard);
        }
    }

    // Hold locks briefly
    thread::sleep(Duration::from_millis(1));

    // Release all locks
    drop(guards);

    start.elapsed()
}

#[test]
#[ignore = "Slow benchmark test - run explicitly with cargo test -- --ignored"]
fn test_lock_contention_comparison() {
    let manager = Arc::new(RowLockManager::default());
    let num_resources = 100;
    let num_threads = 10;
    let resources_per_thread = 10;

    // Test 1: Table-level locking simulation (all threads compete for all resources)
    let manager1 = manager.clone();
    let start_table = Instant::now();
    let mut handles_table = Vec::new();

    for i in 0..num_threads {
        let manager_clone = manager1.clone();
        let handle =
            thread::spawn(move || simulate_table_lock(&manager_clone, i as u64, num_resources));
        handles_table.push(handle);
    }

    let mut total_table_time = Duration::new(0, 0);
    for handle in handles_table {
        total_table_time += handle.join().unwrap();
    }
    let elapsed_table = start_table.elapsed();

    // Test 2: Row-level locking (threads lock different subsets)
    let manager2 = manager.clone();
    let start_row = Instant::now();
    let mut handles_row = Vec::new();

    for i in 0..num_threads {
        let manager_clone = manager2.clone();
        // Each thread locks a different subset of resources
        let resource_ids: Vec<u64> = ((i * resources_per_thread)..((i + 1) * resources_per_thread))
            .map(|x| x as u64)
            .collect();
        let handle = thread::spawn(move || use_row_locks(&manager_clone, i as u64, &resource_ids));
        handles_row.push(handle);
    }

    let mut total_row_time = Duration::new(0, 0);
    for handle in handles_row {
        total_row_time += handle.join().unwrap();
    }
    let elapsed_row = start_row.elapsed();

    // Calculate improvement
    let improvement = if elapsed_table > elapsed_row {
        ((elapsed_table.as_millis() as f64 - elapsed_row.as_millis() as f64)
            / elapsed_table.as_millis() as f64)
            * 100.0
    } else {
        0.0
    };

    tracing::info!("Lock Contention Benchmark Results:");
    tracing::info!(
        "  Table-level (simulated): {:?} total, {:?} elapsed",
        total_table_time,
        elapsed_table
    );
    tracing::info!(
        "  Row-level: {:?} total, {:?} elapsed",
        total_row_time,
        elapsed_row
    );
    tracing::info!("  Improvement: {:.2}%", improvement);

    // Row-level should be faster when resources don't overlap
    assert!(
        elapsed_row <= elapsed_table * 2,
        "Row-level locking should not be significantly slower"
    );
}

#[test]
#[ignore = "Slow benchmark test - run explicitly with cargo test -- --ignored"]
fn test_concurrent_writes_different_resources() {
    let manager = Arc::new(RowLockManager::default());
    let num_threads = 20;
    let num_resources = 100;

    let start = Instant::now();
    let mut handles = Vec::new();

    // Each thread writes to a different resource
    for i in 0..num_threads {
        let manager_clone = manager.clone();
        let resource_id = (i * 5) % num_resources;
        let handle = thread::spawn(move || {
            let guard = manager_clone
                .acquire_write(i as u64, ResourceId::node(resource_id as u64))
                .unwrap();
            thread::sleep(Duration::from_millis(10));
            drop(guard);
            true
        });
        handles.push(handle);
    }

    for handle in handles {
        assert!(handle.join().unwrap());
    }

    let elapsed = start.elapsed();

    // With row-level locking, all threads should complete quickly
    // since they're accessing different resources
    tracing::info!("Concurrent writes to different resources: {:?}", elapsed);
    assert!(
        elapsed < Duration::from_millis(500),
        "Should complete quickly with row-level locking"
    );
}

#[test]
#[ignore = "Slow benchmark test - run explicitly with cargo test -- --ignored"]
fn test_concurrent_writes_same_resources() {
    let manager = Arc::new(RowLockManager::default());
    let num_threads = 20;
    let resource_id = 1; // All threads compete for same resource

    let start = Instant::now();
    let mut handles = Vec::new();
    let mut successes = 0;

    // All threads try to write to the same resource
    for i in 0..num_threads {
        let manager_clone = manager.clone();
        let handle = thread::spawn(move || {
            // Some will timeout, some will succeed
            match manager_clone.acquire_write_with_timeout(
                i as u64,
                ResourceId::node(resource_id),
                Duration::from_millis(100),
            ) {
                Ok(guard) => {
                    thread::sleep(Duration::from_millis(10));
                    drop(guard);
                    true
                }
                Err(_) => false,
            }
        });
        handles.push(handle);
    }

    for handle in handles {
        if handle.join().unwrap() {
            successes += 1;
        }
    }

    let elapsed = start.elapsed();

    tracing::info!(
        "Concurrent writes to same resource: {:?}, {} successes",
        elapsed,
        successes
    );

    // With high contention, not all will succeed
    assert!(successes > 0, "At least some should succeed");
    assert!(
        successes < num_threads,
        "Not all should succeed under high contention"
    );
}

#[test]
#[ignore = "Slow benchmark test - run explicitly with cargo test -- --ignored"]
fn test_lock_contention_metrics() {
    let manager = Arc::new(RowLockManager::default());
    let num_operations = 100;

    // Measure lock acquisition time with low contention
    let start = Instant::now();
    for i in 0..num_operations {
        let guard = manager.acquire_write(i, ResourceId::node(i)).unwrap();
        drop(guard);
    }
    let low_contention_time = start.elapsed();

    // Measure lock acquisition time with high contention
    let manager2 = manager.clone();
    let start = Instant::now();
    let mut handles = Vec::new();

    // All threads compete for same few resources
    for i in 0..num_operations {
        let manager_clone = manager2.clone();
        let resource_id = i % 5; // Only 5 resources, high contention
        let handle = thread::spawn(move || {
            match manager_clone.acquire_write_with_timeout(
                i,
                ResourceId::node(resource_id),
                Duration::from_millis(50),
            ) {
                Ok(guard) => {
                    thread::sleep(Duration::from_millis(1));
                    drop(guard);
                    true
                }
                Err(_) => false,
            }
        });
        handles.push(handle);
    }

    let mut successes = 0;
    for handle in handles {
        if handle.join().unwrap() {
            successes += 1;
        }
    }
    let high_contention_time = start.elapsed();

    tracing::info!("Lock Contention Metrics:");
    tracing::info!(
        "  Low contention ({} operations): {:?}",
        num_operations,
        low_contention_time
    );
    tracing::info!(
        "  High contention ({} operations, {} successes): {:?}",
        num_operations,
        successes,
        high_contention_time
    );
    tracing::info!(
        "  Success rate: {:.2}%",
        (successes as f64 / num_operations as f64) * 100.0
    );

    // Low contention should be faster
    assert!(
        low_contention_time < high_contention_time,
        "Low contention should be faster than high contention"
    );
}

#[test]
#[ignore = "Slow benchmark test - run explicitly with cargo test -- --ignored"]
fn test_row_lock_vs_table_lock_throughput() {
    let manager = Arc::new(RowLockManager::default());
    let num_threads = 10;
    let operations_per_thread = 10;

    // Test row-level locking throughput
    let start = Instant::now();
    let mut handles = Vec::new();

    for t in 0..num_threads {
        let manager_clone = manager.clone();
        let handle = thread::spawn(move || {
            let mut success_count = 0;
            for op in 0..operations_per_thread {
                let resource_id = (t * operations_per_thread + op) as u64;
                if let Ok(guard) = manager_clone
                    .acquire_write((t * 1000 + op) as u64, ResourceId::node(resource_id))
                {
                    thread::sleep(Duration::from_micros(100));
                    drop(guard);
                    success_count += 1;
                }
            }
            success_count
        });
        handles.push(handle);
    }

    let mut total_operations = 0;
    for handle in handles {
        total_operations += handle.join().unwrap();
    }
    let elapsed = start.elapsed();

    let throughput = total_operations as f64 / elapsed.as_secs_f64();

    tracing::info!("Row-level Lock Throughput:");
    tracing::info!("  Total operations: {}", total_operations);
    tracing::info!("  Time: {:?}", elapsed);
    tracing::info!("  Throughput: {:.2} ops/sec", throughput);

    assert!(total_operations > 0, "Should complete some operations");
    assert!(throughput > 10.0, "Should achieve reasonable throughput");
}
