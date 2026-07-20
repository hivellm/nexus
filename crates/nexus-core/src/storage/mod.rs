//! Storage layer for Nexus graph database
//!
//! This module provides the core storage functionality including:
//! - Record stores for nodes and relationships
//! - File-based storage with growth capabilities
//! - Memory-mapped file access for performance
//! - CRUD operations for graph entities
//! - Property storage and retrieval

pub mod adjacency_list;
pub mod crypto;
pub mod external_id;
pub mod graph_engine;
pub mod property_store;
pub mod record_store;
pub mod record_store_ops;
pub mod records;
pub mod row_lock;
pub mod write_buffer;

pub use external_id::{ConflictPolicy, ExternalId};

// Record layout types — constants and structs
pub use records::{
    FLAG_ALLOCATED, FLAG_DELETED, NODE_RECORD_SIZE, NodeRecord, REL_RECORD_SIZE, RecordStoreStats,
    RelationshipRecord,
};

// RecordStore — struct + lifecycle methods (record_store.rs) and operations
// (record_store_ops.rs, which is an impl block extension).
pub use record_store::RecordStore;
