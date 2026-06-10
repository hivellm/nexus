//! MCP tool dispatcher — `handle_nexus_mcp_tool` with cache and stats.

use std::sync::Arc;
use std::time::Instant;

use rmcp::model::{CallToolRequestParam, CallToolResult, Content, ErrorData};
use serde_json::Value;

use crate::NexusServer;

use super::handlers::{
    handle_create_node, handle_create_relationship, handle_execute_cypher, handle_get_stats,
    handle_graph_correlation_analyze, handle_graph_correlation_export,
    handle_graph_correlation_generate, handle_graph_correlation_types, handle_knn_search,
};

/// Handle MCP tool calls for Nexus with performance monitoring and caching
pub async fn handle_nexus_mcp_tool(
    request: CallToolRequestParam,
    server: Arc<NexusServer>,
) -> Result<CallToolResult, ErrorData> {
    let tool_name = request.name.clone();
    let start_time = Instant::now();

    // Check cache for idempotent tools
    let cacheable_tools = [
        "graph_correlation_generate",
        "graph_correlation_analyze",
        "graph_correlation_export",
        "graph_correlation_types",
    ];

    let is_cacheable = cacheable_tools.iter().any(|&t| t == tool_name);
    let cache_hit = false;

    if is_cacheable {
        if let Some(args) = &request.arguments {
            let args_value = Value::Object(args.clone());
            if let Some(cached_result) = server.mcp_tool_cache.get(&tool_name, &args_value) {
                // Return cached result
                let execution_time = start_time.elapsed();

                let input_size = serde_json::to_string(args).ok().map(|s| s.len() as u64);
                let output_size = serde_json::to_string(&cached_result)
                    .ok()
                    .map(|s| s.len() as u64);
                server.mcp_tool_stats.record_tool_call(
                    &tool_name,
                    execution_time,
                    true,
                    None,
                    input_size,
                    output_size,
                    Some(true), // Cache hit
                );

                return Ok(CallToolResult::success(vec![Content::text(
                    cached_result.to_string(),
                )]));
            }
        }
    }

    // Execute tool handler
    let result = match tool_name.as_ref() {
        "create_node" => handle_create_node(request.clone(), server.clone()).await,
        "create_relationship" => handle_create_relationship(request.clone(), server.clone()).await,
        "execute_cypher" => handle_execute_cypher(request.clone(), server.clone()).await,
        "knn_search" => handle_knn_search(request.clone(), server.clone()).await,
        "get_stats" => handle_get_stats(request.clone(), server.clone()).await,
        "graph_correlation_generate" => {
            handle_graph_correlation_generate(request.clone(), server.clone()).await
        }
        "graph_correlation_analyze" => {
            handle_graph_correlation_analyze(request.clone(), server.clone()).await
        }
        "graph_correlation_export" => {
            handle_graph_correlation_export(request.clone(), server.clone()).await
        }
        "graph_correlation_types" => {
            handle_graph_correlation_types(request.clone(), server.clone()).await
        }
        _ => Err(ErrorData::invalid_params("Unknown tool", None)),
    };

    let execution_time = start_time.elapsed();
    let success = result.is_ok();
    let error = result.as_ref().err().map(|e| format!("{:?}", e));

    // Calculate sizes
    let input_size = request
        .arguments
        .as_ref()
        .and_then(|args| serde_json::to_string(args).ok())
        .map(|s| s.len() as u64);
    let output_size = result
        .as_ref()
        .ok()
        .and_then(|r| serde_json::to_string(r).ok())
        .map(|s| s.len() as u64);

    // Record statistics
    server.mcp_tool_stats.record_tool_call(
        &tool_name,
        execution_time,
        success,
        error,
        input_size,
        output_size,
        if is_cacheable { Some(cache_hit) } else { None },
    );

    // Cache successful results for cacheable tools
    if is_cacheable
        && success
        && let Some(args) = &request.arguments
    {
        let args_value = Value::Object(args.clone());
        if let Ok(result_value) = &result
            && let Ok(result_json) = serde_json::to_value(result_value)
        {
            server
                .mcp_tool_cache
                .put(&tool_name, &args_value, result_json, None);
        }
    }

    result
}
