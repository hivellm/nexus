//! MCP StreamableHTTP service — `NexusMcpService` and its `ServerHandler` impl.

use std::sync::Arc;

use rmcp::ServerHandler;
use rmcp::model::{
    CallToolRequestParam, CallToolResult, ErrorData, Implementation, ListResourcesResult,
    ListToolsResult, ProtocolVersion, ServerCapabilities, ServerInfo,
};
use rmcp::service::RequestContext;

use crate::NexusServer;

use super::dispatcher::handle_nexus_mcp_tool;
use super::tools::get_nexus_mcp_tools;

/// StreamableHTTP service implementation for Nexus
#[derive(Clone)]
pub struct NexusMcpService {
    /// Nexus server state
    pub server: Arc<NexusServer>,
}

impl NexusMcpService {
    /// Create a new MCP service instance
    pub fn new(server: Arc<NexusServer>) -> Self {
        Self { server }
    }
}

impl ServerHandler for NexusMcpService {
    fn get_info(&self) -> ServerInfo {
        ServerInfo {
            protocol_version: ProtocolVersion::default(),
            capabilities: ServerCapabilities::builder()
                .enable_tools()
                .build(),
            server_info: Implementation {
                name: "nexus-server".to_string(),
                title: Some("Nexus Graph Database Server".to_string()),
                version: env!("CARGO_PKG_VERSION").to_string(),
                website_url: Some("https://github.com/hivellm/nexus".to_string()),
                icons: None,
            },
            instructions: Some("Nexus Graph Database - High-performance property graph database with native vector search and MCP integration.".to_string()),
        }
    }

    async fn list_tools(
        &self,
        _request: Option<rmcp::model::PaginatedRequestParam>,
        _context: RequestContext<rmcp::RoleServer>,
    ) -> Result<ListToolsResult, ErrorData> {
        let tools = get_nexus_mcp_tools();

        Ok(ListToolsResult {
            tools,
            next_cursor: None,
        })
    }

    async fn call_tool(
        &self,
        request: CallToolRequestParam,
        _context: RequestContext<rmcp::RoleServer>,
    ) -> Result<CallToolResult, ErrorData> {
        handle_nexus_mcp_tool(request, self.server.clone()).await
    }

    async fn list_resources(
        &self,
        _request: Option<rmcp::model::PaginatedRequestParam>,
        _context: RequestContext<rmcp::RoleServer>,
    ) -> Result<ListResourcesResult, ErrorData> {
        Ok(ListResourcesResult {
            resources: vec![],
            next_cursor: None,
        })
    }
}
