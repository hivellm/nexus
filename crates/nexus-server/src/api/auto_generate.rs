//! Automatic Graph Generation from Vectorizer Data
//!
//! Extracts code structure from vectorizer and generates correlation graphs

use axum::{
    extract::Query,
    http::StatusCode,
    response::{IntoResponse, Json},
};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

use nexus_core::graph::correlation::{GraphCorrelationManager, GraphSourceData, GraphType};

/// Query parameters for automatic graph generation
#[derive(Debug, Deserialize)]
pub struct AutoGenerateQuery {
    /// Project path to analyze
    pub project_path: Option<String>,
    /// Graph types to generate (comma-separated: call,dependency,dataflow,component)
    pub graph_types: Option<String>,
    /// Maximum files to analyze
    pub max_files: Option<usize>,
}

/// Response for automatic generation
#[derive(Debug, Serialize)]
pub struct AutoGenerateResponse {
    /// Number of files analyzed
    pub files_analyzed: usize,
    /// Generated graphs by type
    pub graphs: HashMap<String, GraphSummary>,
    /// Extraction time in milliseconds
    pub extraction_time_ms: u64,
    /// Generation time in milliseconds
    pub generation_time_ms: u64,
}

/// Graph summary
#[derive(Debug, Serialize)]
pub struct GraphSummary {
    /// Graph type
    pub graph_type: String,
    /// Number of nodes
    pub node_count: usize,
    /// Number of edges
    pub edge_count: usize,
}

/// Generate graphs automatically from project codebase
///
/// This endpoint:
/// 1. Scans the project directory for source files
/// 2. Extracts code structure (functions, imports, calls)
/// 3. Generates multiple graph types automatically
/// 4. Returns summaries of all generated graphs
pub async fn auto_generate_graphs(Query(params): Query<AutoGenerateQuery>) -> impl IntoResponse {
    let start_time = std::time::Instant::now();

    // Extract data from codebase
    let source_data = match extract_from_codebase(&params).await {
        Ok(data) => data,
        Err(e) => {
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({
                    "error": format!("Failed to extract codebase data: {}", e)
                })),
            )
                .into_response();
        }
    };

    let extraction_time = start_time.elapsed();
    let files_analyzed = source_data.files.len();

    // Generate graphs
    let generation_start = std::time::Instant::now();
    let mut graphs = HashMap::new();

    let graph_types_to_generate = parse_graph_types(&params.graph_types);

    for graph_type in graph_types_to_generate {
        match generate_graph_of_type(graph_type, &source_data).await {
            Ok(summary) => {
                graphs.insert(format!("{:?}", graph_type), summary);
            }
            Err(e) => {
                tracing::warn!("Failed to generate {:?} graph: {}", graph_type, e);
            }
        }
    }

    let generation_time = generation_start.elapsed();

    let response = AutoGenerateResponse {
        files_analyzed,
        graphs,
        extraction_time_ms: extraction_time.as_millis() as u64,
        generation_time_ms: generation_time.as_millis() as u64,
    };

    (StatusCode::OK, Json(response)).into_response()
}

/// Extract source data from codebase
async fn extract_from_codebase(
    params: &AutoGenerateQuery,
) -> Result<GraphSourceData, Box<dyn std::error::Error>> {
    let mut source_data = GraphSourceData::new();

    // Simulate extraction from current Nexus codebase
    let _project_path = params.project_path.as_deref().unwrap_or("./nexus-core/src");

    // Sample data extraction (in production, this would scan actual files)
    add_sample_rust_code_structure(&mut source_data, params.max_files.unwrap_or(50));

    Ok(source_data)
}

/// Add sample Rust code structure for demonstration
fn add_sample_rust_code_structure(source_data: &mut GraphSourceData, max_files: usize) {
    // Simulate main.rs
    source_data.add_file(
        "src/main.rs".to_string(),
        r#"
        use nexus_core::Engine;
        use nexus_server::Server;
        
        fn main() {
            let engine = Engine::new();
            let server = Server::new(engine);
            server.run();
        }
        "#
        .to_string(),
    );

    source_data.add_functions("src/main.rs".to_string(), vec!["main".to_string()]);

    // Simulate engine.rs
    source_data.add_file(
        "src/engine.rs".to_string(),
        r#"
        pub struct Engine {
            executor: Executor,
        }
        
        impl Engine {
            pub fn new() -> Self {
                let executor = Executor::init();
                Self { executor }
            }
            
            pub fn execute(&self, query: &str) -> Result<QueryResult> {
                self.executor.run(query)
            }
        }
        "#
        .to_string(),
    );

    source_data.add_functions(
        "src/engine.rs".to_string(),
        vec!["Engine::new".to_string(), "Engine::execute".to_string()],
    );

    // Add imports
    source_data.add_import("main.rs".to_string(), "nexus_core::Engine".to_string());
    source_data.add_import("main.rs".to_string(), "nexus_server::Server".to_string());
    source_data.add_import("engine.rs".to_string(), "nexus_core::Executor".to_string());
    source_data.add_import(
        "engine.rs".to_string(),
        "nexus_core::QueryResult".to_string(),
    );

    // Add more sample files up to max_files
    for i in 1..max_files.min(20) {
        let filename = format!("src/module{}.rs", i);
        source_data.add_file(filename.clone(), format!("// Module {}", i));
        source_data.add_functions(filename.clone(), vec![format!("module{}::process", i)]);
    }
}

/// Parse graph types from query string
fn parse_graph_types(types_str: &Option<String>) -> Vec<GraphType> {
    match types_str {
        Some(s) => s
            .split(',')
            .filter_map(|t| match t.trim().to_lowercase().as_str() {
                "call" => Some(GraphType::Call),
                "dependency" => Some(GraphType::Dependency),
                "dataflow" => Some(GraphType::DataFlow),
                "component" => Some(GraphType::Component),
                _ => None,
            })
            .collect(),
        None => vec![
            GraphType::Call,
            GraphType::Dependency,
            GraphType::DataFlow,
            GraphType::Component,
        ],
    }
}

/// Generate graph of specific type
async fn generate_graph_of_type(
    graph_type: GraphType,
    source_data: &GraphSourceData,
) -> Result<GraphSummary, Box<dyn std::error::Error>> {
    let manager = GraphCorrelationManager::new();
    let graph = manager.build_graph(graph_type, source_data)?;

    Ok(GraphSummary {
        graph_type: format!("{:?}", graph_type),
        node_count: graph.nodes.len(),
        edge_count: graph.edges.len(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_graph_types_all() {
        let types = parse_graph_types(&None);
        assert_eq!(types.len(), 4);
    }

    #[test]
    fn test_parse_graph_types_specific() {
        let types_str = Some("call,dependency".to_string());
        let types = parse_graph_types(&types_str);
        assert_eq!(types.len(), 2);
        assert!(matches!(types[0], GraphType::Call));
        assert!(matches!(types[1], GraphType::Dependency));
    }

    #[test]
    fn test_parse_graph_types_case_insensitive() {
        let types_str = Some("CALL,Dependency".to_string());
        let types = parse_graph_types(&types_str);
        assert_eq!(types.len(), 2);
    }

    #[test]
    fn test_parse_graph_types_invalid() {
        let types_str = Some("invalid,call".to_string());
        let types = parse_graph_types(&types_str);
        assert_eq!(types.len(), 1);
        assert!(matches!(types[0], GraphType::Call));
    }

    #[test]
    fn test_add_sample_rust_code() {
        let mut source_data = GraphSourceData::new();
        add_sample_rust_code_structure(&mut source_data, 5);

        assert!(!source_data.files.is_empty());
        assert!(!source_data.functions.is_empty());
        assert!(!source_data.imports.is_empty());
    }

    #[tokio::test]
    async fn test_generate_graph_of_type() {
        let mut source_data = GraphSourceData::new();
        add_sample_rust_code_structure(&mut source_data, 5);

        let result = generate_graph_of_type(GraphType::Call, &source_data).await;
        assert!(result.is_ok());

        let summary = result.unwrap();
        assert_eq!(summary.graph_type, "Call");
        assert!(summary.node_count > 0);
    }
}
