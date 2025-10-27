//! Nexus Protocol - Integration with external services
//!
//! Provides client abstractions for:
//! - REST/HTTP streaming (default integration)
//! - MCP (Model Context Protocol)
//! - UMICP (Universal Model Interoperability Protocol)

#![warn(clippy::all)]
#![allow(dead_code)] // Allow during initial scaffolding

pub mod mcp;
pub mod rest;
pub mod umicp;

pub use rest::RestClient;
