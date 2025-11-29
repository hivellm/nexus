//! Graph-Native Storage Engine
//!
//! This module implements a custom storage engine optimized for graph workloads,
//! designed to achieve Neo4j performance parity by eliminating LMDB overhead.
//!
//! Key optimizations:
//! - Relationship-centric storage (relationships as first-class citizens)
//! - Single large memory-mapped file (eliminates cache thrashing)
//! - Type-based segmentation (groups relationships by type for locality)
//! - Direct I/O optimization (bypasses OS caching for SSD performance)
//! - Compression algorithms (reduces memory usage and I/O)

pub mod bench;
pub mod compression;
pub mod engine;
pub mod format;
pub mod io;
pub mod migration;

pub use compression::RelationshipCompressor;
pub use engine::GraphStorageEngine;
pub use format::{
    BloomFilter, BloomFilterStats, NodeRecord, RelationshipRecord, RelationshipSegment, SkipList,
    SkipListNode, SkipListStats, StorageLayout,
};
pub use io::DirectFile;
pub use migration::{
    MigrationOptions, MigrationStats, export_to_record_store, migrate_to_graph_engine,
};
