//! Nexus Rust SDK
//!
//! Official Rust SDK for Nexus graph database.
//!
//! # Example
//!
//! ```no_run
//! use nexus_sdk::NexusClient;
//!
//! #[tokio::main]
//! async fn main() -> Result<(), Box<dyn std::error::Error>> {
//!     // Create a client
//!     let client = NexusClient::new("http://localhost:15474")?;
//!
//!     // Execute a Cypher query
//!     let result = client.execute_cypher("MATCH (n) RETURN n LIMIT 10", None).await?;
//!
//!     tracing::info!("Found {} rows", result.rows.len());
//!     Ok(())
//! }
//! ```

pub mod batch;
pub mod client;
pub mod data;
pub mod error;
pub mod models;
pub mod performance;
pub mod query;
pub mod query_builder;
pub mod schema;
pub mod transaction;

pub use batch::*;
pub use client::NexusClient;
pub use data::*;
pub use error::{NexusError, Result};
pub use models::*;
pub use performance::*;
pub use query_builder::{BuiltQuery, QueryBuilder};
pub use schema::*;
pub use transaction::{Transaction, TransactionStatus};
