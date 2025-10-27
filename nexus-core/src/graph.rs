//! Nexus Graph - Unified graph module
//!
//! This module provides comprehensive graph functionality including:
//! - Core graph structures (nodes, edges, properties)
//! - Graph algorithms (traversal, shortest path, centrality)
//! - Graph construction and layout
//! - Graph comparison and diff
//! - Clustering algorithms
//! - Graph correlation analysis

// Submodules
pub mod algorithms;
pub mod clustering;
pub mod comparison;
pub mod construction;
pub mod correlation;
pub mod simple;

// Re-export main types from the original graph module
mod core;
pub use core::*;
