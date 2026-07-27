//! MCP StreamableHTTP service — `NexusMcpService` and its `ServerHandler` impl.

use std::sync::Arc;

use rmcp::ServerHandler;
use rmcp::model::{
    CallToolRequestParams, CallToolResult, ErrorData, Implementation, ListResourcesResult,
    ListToolsResult, PaginatedRequestParams, ServerCapabilities, ServerInfo,
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
        ServerInfo::new(ServerCapabilities::builder().enable_tools().build())
            .with_server_info(
                Implementation::new("nexus-server", env!("CARGO_PKG_VERSION"))
                    .with_title("Nexus Graph Database Server")
                    .with_website_url("https://github.com/hivellm/nexus"),
            )
            .with_instructions(
                "Nexus Graph Database - High-performance property graph database with native vector search and MCP integration.",
            )
    }

    async fn list_tools(
        &self,
        _request: Option<PaginatedRequestParams>,
        _context: RequestContext<rmcp::RoleServer>,
    ) -> Result<ListToolsResult, ErrorData> {
        let tools = get_nexus_mcp_tools();

        Ok(ListToolsResult::with_all_items(tools))
    }

    async fn call_tool(
        &self,
        request: CallToolRequestParams,
        _context: RequestContext<rmcp::RoleServer>,
    ) -> Result<CallToolResult, ErrorData> {
        handle_nexus_mcp_tool(request, self.server.clone()).await
    }

    async fn list_resources(
        &self,
        _request: Option<PaginatedRequestParams>,
        _context: RequestContext<rmcp::RoleServer>,
    ) -> Result<ListResourcesResult, ErrorData> {
        Ok(ListResourcesResult::with_all_items(vec![]))
    }
}
