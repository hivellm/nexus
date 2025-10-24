//! Nexus Core - Property Graph Database Engine
//!
//! This crate provides the core graph database engine for Nexus, implementing:
//! - Property graph model (nodes with labels, edges with types, properties)
//! - Neo4j-inspired record stores (nodes.store, rels.store, props.store)
//! - Page cache with eviction policies (clock/2Q/TinyLFU)
//! - Write-ahead log (WAL) with MVCC by epoch
//! - Cypher subset executor (pattern matching, expand, filter, project)
//! - Multi-index subsystem (label bitmap, B-tree, full-text, KNN)
//!
//! # Architecture
//!
//! ```text
//! ┌─────────────────────────────────────────────┐
//! │           Cypher Executor                    │
//! │   (Pattern Match, Expand, Filter, Project)  │
//! └──────────────┬──────────────────────────────┘
//!                │
//! ┌──────────────┴──────────────────────────────┐
//! │          Transaction Layer                   │
//! │        (MVCC, Locking, Isolation)           │
//! └──────────────┬──────────────────────────────┘
//!                │
//! ┌──────────────┴──────────────────────────────┐
//! │            Index Layer                       │
//! │  (Label Bitmap, B-tree, Full-text, KNN)     │
//! └──────────────┬──────────────────────────────┘
//!                │
//! ┌──────────────┴──────────────────────────────┐
//! │           Storage Layer                      │
//! │  (Record Stores, Page Cache, WAL, Catalog)  │
//! └─────────────────────────────────────────────┘
//! ```

#![deny(missing_docs)]
#![warn(clippy::all)]
#![allow(dead_code)] // Allow during initial scaffolding

pub mod catalog;
pub mod error;
pub mod executor;
pub mod index;
pub mod page_cache;
pub mod storage;
pub mod transaction;
pub mod wal;

pub use error::{Error, Result};

/// Graph database engine
pub struct Engine {
    // Will be populated during implementation
}

impl Engine {
    /// Create a new engine instance
    pub fn new() -> Result<Self> {
        todo!("Engine::new - to be implemented in MVP")
    }
}

impl Default for Engine {
    fn default() -> Self {
        Self::new().expect("Failed to create default engine")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_error_storage() {
        let err = Error::storage("test error");
        assert!(matches!(err, Error::Storage(_)));
        assert_eq!(err.to_string(), "Storage error: test error");
    }

    #[test]
    fn test_error_page_cache() {
        let err = Error::page_cache("cache full");
        assert!(matches!(err, Error::PageCache(_)));
    }

    #[test]
    fn test_error_wal() {
        let err = Error::wal("checkpoint failed");
        assert!(matches!(err, Error::Wal(_)));
    }
}
