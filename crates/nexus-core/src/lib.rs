//! Nexus Core - Property Graph Database Engine
//!
//! This crate provides the core graph database engine for Nexus, implementing:
//! - Property graph model (nodes with labels, edges with types, properties)
//! - Neo4j-inspired record stores (nodes.store, rels.store, props.store)
//! - Page cache with eviction policies (clock/2Q/TinyLFU)
//! - Write-ahead log (WAL) with MVCC by epoch
//! - Cypher subset executor (pattern matching, expand, filter, project)
//! - Multi-index subsystem (label bitmap, B-tree, full-text, KNN)
//! - Graph validation and integrity checks
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

#![allow(missing_docs)]
#![allow(warnings)] // Suppress all warnings
#![allow(dead_code)] // Allow during initial scaffolding

use serde_json::{Map, Value};
use std::collections::{HashMap, HashSet};
use tracing;

pub mod auth;
pub mod cache;
pub mod catalog;
pub mod cluster;
pub mod concurrent_access;
pub mod coordinator;
pub mod database;
pub mod error;
pub mod execution;
pub mod executor;
pub mod geospatial;
pub mod graph; // Unified graph module with submodules
pub mod index;
pub mod loader;
pub mod memory;
pub mod memory_management;
pub mod monitoring;
pub mod page_cache;
pub mod performance;
pub mod plugin;
pub mod query_cache;
pub mod relationship;
pub mod replication;
pub mod retry;
pub mod security;
pub mod session;
pub mod sharding;
pub mod simd;
pub mod storage;
pub mod transaction;
pub mod udf;
pub mod validation;
pub mod vectorizer_cache;
pub mod wal;

// Testing infrastructure - exposed unconditionally for integration tests
// The module provides isolated test environments that prevent LMDB conflicts
pub mod testing;

pub use error::{Error, Result};
pub use graph::clustering::{
    Cluster, ClusteringAlgorithm, ClusteringConfig, ClusteringEngine, ClusteringMetrics,
    ClusteringResult, DistanceMetric, FeatureStrategy, LinkageType,
};
pub use graph::comparison::{
    ComparisonOptions, DiffSummary, EdgeChanges, EdgeModification, GraphComparator, GraphDiff,
    NodeChanges, NodeModification, PropertyValueChange,
};
pub use graph::construction::{
    CircularLayout, ConnectedComponents, ForceDirectedLayout, GraphLayout, GridLayout,
    HierarchicalLayout, KMeansClustering, LayoutDirection, LayoutEdge, LayoutNode, Point2D,
};
pub use graph::correlation::NodeType;
pub use graph::simple::{
    Edge as SimpleEdge, EdgeId as SimpleEdgeId, Graph as SimpleGraph,
    GraphStats as SimpleGraphStats, Node as SimpleNode, NodeId as SimpleNodeId, PropertyValue,
};
pub use graph::{Edge, EdgeId, Graph, GraphStats, Node, NodeId};
pub use validation::{
    GraphValidator, ValidationConfig, ValidationError, ValidationErrorType, ValidationResult,
    ValidationSeverity, ValidationStats, ValidationWarning, ValidationWarningType,
};

pub mod engine;
pub use engine::{Engine, EngineConfig, EngineStats, GraphStatistics, HealthState, HealthStatus};
