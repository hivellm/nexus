//! API handlers

pub mod auth;
pub mod auto_generate;
pub mod clustering;
pub mod comparison;
pub mod cypher;
#[cfg(test)]
#[path = "cypher_test.rs"]
pub mod cypher_test;
pub mod data;
pub mod database;
pub mod export;
pub mod graph_correlation;
#[cfg(test)]
#[path = "graph_correlation_mcp_tests.rs"]
pub mod graph_correlation_mcp_tests;
pub mod graph_correlation_umicp;
pub mod health;
pub mod ingest;
pub mod knn;
pub mod mcp_performance;
pub mod openapi;
pub mod performance;
pub mod prometheus;
pub mod property_keys;
pub mod schema;
pub mod stats;
pub mod streaming;
