//! Phase 9: Memory and Concurrency Optimization
//!
//! This module provides:
//! - NUMA-aware memory allocation
//! - Advanced caching strategies
//! - Lock-free data structures

pub mod cache;
pub mod lockfree;
pub mod numa;

pub use cache::{CacheStats, NumaPartitionedCache, PredictivePrefetcher, PrefetcherStats};
pub use lockfree::{LockFreeCounter, LockFreeHashMap, LockFreeStack};
pub use numa::{NumaAllocator, NumaConfig, NumaScheduler, NumaStats, detect_numa_nodes};
