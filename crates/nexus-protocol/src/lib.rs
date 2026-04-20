//! Nexus Protocol - Integration with external services
//!
//! Provides client abstractions for:
//! - REST/HTTP streaming (default integration)
//! - MCP (Model Context Protocol)
//! - UMICP (Universal Model Interoperability Protocol)
//! - Binary RPC (native transport shared with the Rust SDK)

#![allow(warnings)] // Suppress all warnings
#![allow(dead_code)] // Allow during initial scaffolding

pub mod mcp;
pub mod resp3;
pub mod rest;
pub mod rpc;
pub mod umicp;

pub use mcp::{McpClient, McpClientError};
pub use rest::{RestClient, RestClientError};
pub use umicp::{UmicpClient, UmicpClientError};
