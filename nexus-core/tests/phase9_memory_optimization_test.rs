//! Phase 9: Memory and Concurrency Optimization Tests
//!
//! This module tests the Phase 9 optimizations:
//! - NUMA-aware memory allocation
//! - Advanced caching strategies
//! - Lock-free data structures

use nexus_core::Engine;
use nexus_core::memory::{
    CacheStats, LockFreeCounter, LockFreeHashMap, LockFreeStack, NumaAllocator, NumaConfig,
    NumaPartitionedCache, NumaScheduler, PredictivePrefetcher,
};
use nexus_core::testing::setup_test_engine;
use std::sync::Arc;
use std::thread;
use std::time::Instant;

#[test]
fn test_lock_free_counter() {
    tracing::info!("=== Phase 9.3: Lock-Free Counter Test ===");

    let counter = Arc::new(LockFreeCounter::new(0));

    // Test single-threaded operations
    assert_eq!(counter.increment(), 1);
    assert_eq!(counter.get(), 1);
    assert_eq!(counter.add(5), 1);
    assert_eq!(counter.get(), 6);
    assert_eq!(counter.decrement(), 5);
    assert_eq!(counter.get(), 5);

    // Test concurrent increments
    let counter_clone = counter.clone();
    let handle = thread::spawn(move || {
        for _ in 0..1000 {
            counter_clone.increment();
        }
    });

    for _ in 0..1000 {
        counter.increment();
    }

    handle.join().unwrap();
    assert_eq!(counter.get(), 2005); // 5 (initial) + 1000 + 1000

    tracing::info!("✅ Lock-free counter working correctly");
}

#[test]
fn test_lock_free_stack() {
    tracing::info!("=== Phase 9.3: Lock-Free Stack Test ===");

    let stack = Arc::new(LockFreeStack::new());

    // Single-threaded test
    stack.push(1);
    stack.push(2);
    stack.push(3);

    assert_eq!(stack.pop(), Some(3));
    assert_eq!(stack.pop(), Some(2));
    assert_eq!(stack.pop(), Some(1));
    assert_eq!(stack.pop(), None);

    // Concurrent push/pop test
    let stack_clone = stack.clone();
    let handle = thread::spawn(move || {
        for i in 0..100 {
            stack_clone.push(i);
        }
    });

    for i in 100..200 {
        stack.push(i);
    }

    handle.join().unwrap();

    // Should have 200 elements total
    let mut count = 0;
    while stack.pop().is_some() {
        count += 1;
    }
    assert_eq!(count, 200);

    tracing::info!("✅ Lock-free stack working correctly");
}

#[test]
fn test_lock_free_hash_map() {
    tracing::info!("=== Phase 9.3: Lock-Free Hash Map Test ===");

    let map = Arc::new(LockFreeHashMap::new(16));

    // Single-threaded test
    map.insert("key1".to_string(), "value1".to_string());
    assert_eq!(map.get(&"key1".to_string()), Some("value1".to_string()));

    let old = map.insert("key1".to_string(), "value2".to_string());
    assert_eq!(old, Some("value1".to_string()));

    let removed = map.remove(&"key1".to_string());
    assert_eq!(removed, Some("value2".to_string()));

    // Concurrent insert test
    let map_clone = map.clone();
    let handle = thread::spawn(move || {
        for i in 0..100 {
            map_clone.insert(format!("key{}", i), format!("value{}", i));
        }
    });

    for i in 100..200 {
        map.insert(format!("key{}", i), format!("value{}", i));
    }

    handle.join().unwrap();

    assert_eq!(map.len(), 200);

    tracing::info!("✅ Lock-free hash map working correctly");
}

#[test]
fn test_numa_partitioned_cache() {
    tracing::info!("=== Phase 9.2: NUMA-Partitioned Cache Test ===");

    let config = NumaConfig {
        enabled: false, // Disable for testing (no NUMA hardware required)
        preferred_node: None,
        num_nodes: 4,
    };
    let cache = NumaPartitionedCache::new(config);

    // Insert some values
    for i in 0..100 {
        cache.insert(format!("key{}", i), format!("value{}", i));
    }

    // Verify all values can be retrieved
    for i in 0..100 {
        assert_eq!(cache.get(&format!("key{}", i)), Some(format!("value{}", i)));
    }

    let stats = cache.stats();
    assert_eq!(stats.total_size, 100);
    assert_eq!(stats.num_partitions, 4);

    tracing::info!("✅ NUMA-partitioned cache working correctly");
}

#[test]
fn test_predictive_prefetcher() {
    tracing::info!("=== Phase 9.2: Predictive Prefetcher Test ===");

    let prefetcher = PredictivePrefetcher::new(10);

    // Record some access patterns
    for i in 0..20 {
        prefetcher.record_access(format!("key{}", i));
    }

    let stats = prefetcher.stats();
    assert_eq!(stats.tracked_keys, 20);
    assert_eq!(stats.total_accesses, 20);

    // Test prediction
    let predictions = prefetcher.predict_next(&"key0".to_string());
    assert!(predictions.len() <= 5);

    tracing::info!("✅ Predictive prefetcher working correctly");
}

#[test]
fn test_numa_allocator() {
    tracing::info!("=== Phase 9.1: NUMA Allocator Test ===");

    let config = NumaConfig {
        enabled: false, // Disable for testing
        preferred_node: None,
        num_nodes: 2,
    };
    let allocator = NumaAllocator::new(config);

    let layout = std::alloc::Layout::from_size_align(1024, 8).unwrap();
    let ptr = allocator.allocate_on_node(layout, 0).unwrap();
    assert!(!ptr.is_null());

    unsafe {
        use std::alloc::GlobalAlloc;
        std::alloc::System.dealloc(ptr, layout);
    }

    let stats = allocator.get_stats();
    assert_eq!(stats.num_nodes, 2);

    tracing::info!("✅ NUMA allocator working correctly");
}

#[test]
fn test_numa_scheduler() {
    tracing::info!("=== Phase 9.1: NUMA Scheduler Test ===");

    let config = NumaConfig {
        enabled: false, // Disable for testing
        preferred_node: None,
        num_nodes: 2,
    };
    let scheduler = NumaScheduler::new(config);

    // Test scheduling on a specific node
    let result = scheduler
        .schedule_on_node(0, || {
            // Simulate some work
            42
        })
        .unwrap();
    assert_eq!(result, 42);

    tracing::info!("✅ NUMA scheduler working correctly");
}

#[test]
fn test_phase9_integration() {
    tracing::info!("=== Phase 9: Integration Test ===");

    let (mut engine, _ctx) = setup_test_engine().unwrap();

    // Create some data
    engine
        .execute_cypher("CREATE (a:Person {id: 1}), (b:Person {id: 2})")
        .unwrap();

    // Test that queries still work with Phase 9 optimizations
    let result = engine
        .execute_cypher("MATCH (n:Person) RETURN n.id as id ORDER BY n.id")
        .unwrap();

    assert_eq!(result.rows.len(), 2);
    assert_eq!(result.rows[0].values[0], serde_json::json!(1));
    assert_eq!(result.rows[1].values[0], serde_json::json!(2));

    tracing::info!("✅ Phase 9 integration working correctly");
}

#[test]
#[ignore = "Performance benchmark - run explicitly"]
fn benchmark_phase9_optimizations() {
    tracing::info!("=== Phase 9: Performance Benchmark ===");

    // Benchmark lock-free counter vs standard counter
    let lock_free_counter = Arc::new(LockFreeCounter::new(0));
    let start = Instant::now();
    for _ in 0..1_000_000 {
        lock_free_counter.increment();
    }
    let lock_free_time = start.elapsed();
    tracing::info!("Lock-free counter: {:?} for 1M increments", lock_free_time);

    // Benchmark NUMA-partitioned cache
    let config = NumaConfig {
        enabled: false,
        preferred_node: None,
        num_nodes: 4,
    };
    let cache = NumaPartitionedCache::new(config);
    let start = Instant::now();
    for i in 0..10_000 {
        cache.insert(format!("key{}", i), format!("value{}", i));
    }
    let insert_time = start.elapsed();
    tracing::info!("NUMA cache insert: {:?} for 10K entries", insert_time);

    let start = Instant::now();
    for i in 0..10_000 {
        cache.get(&format!("key{}", i));
    }
    let get_time = start.elapsed();
    tracing::info!("NUMA cache get: {:?} for 10K lookups", get_time);

    tracing::info!("=== Phase 9 Benchmark Complete ===");
}
