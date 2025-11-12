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
pub mod graph_correlation;
pub mod health;
pub mod ingest;
pub mod knn;
pub mod openapi;
pub mod property_keys;
pub mod schema;
pub mod stats;
pub mod streaming;
