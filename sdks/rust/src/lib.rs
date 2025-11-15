//! Nexus Rust SDK
//!
//! Official Rust SDK for Nexus graph database.
//!
//! # Example
//!
//! ```no_run
//! use nexus_sdk_rust::NexusClient;
//!
//! #[tokio::main]
//! async fn main() -> Result<(), Box<dyn std::error::Error>> {
//!     // Create a client
//!     let client = NexusClient::new("http://localhost:15474")?;
//!
//!     // Execute a Cypher query
//!     let result = client.execute_cypher("MATCH (n) RETURN n LIMIT 10", None).await?;
//!
//!     println!("Found {} rows", result.rows.len());
//!     Ok(())
//! }
//! ```

pub mod client;
pub mod error;
pub mod models;
pub mod query;

pub use client::NexusClient;
pub use error::{NexusError, Result};
pub use models::*;
