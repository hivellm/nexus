//! MCP StreamableHTTP implementation for Nexus
//!
//! This module provides MCP (Model Context Protocol) support using StreamableHTTP transport.
//! Based on the rmcp crate with transport-streamable-http-server.
//!
//! ## Protocol
//! - **MCP StreamableHTTP**: Primary protocol for AI integrations
//! - **Transport**: HTTP with chunked transfer encoding
//! - **Compatible with**: Vectorizer, Context7, and other MCP clients

mod dispatcher;
mod handlers;
mod service;
mod tools;

// Preserve the disabled test block in its own file so it stays compilable.
#[path = "tests.rs"]
mod streaming_tests;

// Facade re-exports — everything previously reachable at `crate::api::streaming::*`
pub use dispatcher::handle_nexus_mcp_tool;
pub use handlers::health_check;
pub use service::NexusMcpService;
pub use tools::get_nexus_mcp_tools;
