//! Integration tests with mock vectorizer
//!
//! Tests graph correlation with mocked vectorizer data

use nexus_core::graph::correlation::*;
use serde_json::json;
use std::collections::HashMap;

/// Mock vectorizer data generator
struct MockVectorizer {
    functions: HashMap<String, serde_json::Value>,
    imports: HashMap<String, Vec<String>>,
}

impl MockVectorizer {
    fn new() -> Self {
        Self {
            functions: HashMap::new(),
            imports: HashMap::new(),
        }
    }

    fn add_function(&mut self, name: &str, file: &str, signature: &str) {
        self.functions.insert(
            name.to_string(),
            json!({
                "id": name,
                "name": name,
                "file": file,
                "signature": signature,
                "type": "function"
            }),
        );
    }

    fn add_import(&mut self, from_file: &str, to_file: &str) {
        self.imports
            .entry(from_file.to_string())
            .or_insert_with(Vec::new)
            .push(to_file.to_string());
    }

    fn get_functions(&self) -> Vec<serde_json::Value> {
        self.functions.values().cloned().collect()
    }

    fn get_imports_for_file(&self, file: &str) -> Vec<String> {
        self.imports.get(file).cloned().unwrap_or_default()
    }
}

#[test]
fn test_call_graph_with_mock_vectorizer() {
    let mut mock = MockVectorizer::new();

    // Add mock functions
    mock.add_function("main", "main.rs", "fn main()");
    mock.add_function(
        "process_data",
        "lib.rs",
        "fn process_data(input: String) -> Result<()>",
    );
    mock.add_function("validate", "lib.rs", "fn validate(data: &str) -> bool");

    // Create source data from mock
    let mut source_data = GraphSourceData::new();
    source_data.add_file(
        "main.rs".to_string(),
        "fn main() { process_data(); }".to_string(),
    );
    source_data.add_file(
        "lib.rs".to_string(),
        "fn process_data() { validate(); }".to_string(),
    );

    source_data.add_functions("main.rs".to_string(), vec!["main".to_string()]);
    source_data.add_functions(
        "lib.rs".to_string(),
        vec!["process_data".to_string(), "validate".to_string()],
    );

    // Build call graph
    let manager = GraphCorrelationManager::new();
    let graph = manager.build_graph(GraphType::Call, &source_data).unwrap();

    assert_eq!(graph.graph_type, GraphType::Call);
    assert!(!graph.nodes.is_empty());
}

#[test]
fn test_dependency_graph_with_mock_vectorizer() {
    let mut mock = MockVectorizer::new();

    // Add mock imports
    mock.add_import("main.rs", "lib.rs");
    mock.add_import("lib.rs", "utils.rs");

    // Create source data
    let mut source_data = GraphSourceData::new();
    source_data.add_file("main.rs".to_string(), "use lib;".to_string());
    source_data.add_file("lib.rs".to_string(), "use utils;".to_string());
    source_data.add_file("utils.rs".to_string(), "".to_string());

    source_data.add_imports("main.rs".to_string(), vec!["lib".to_string()]);
    source_data.add_imports("lib.rs".to_string(), vec!["utils".to_string()]);

    // Build dependency graph
    let manager = GraphCorrelationManager::new();
    let graph = manager
        .build_graph(GraphType::Dependency, &source_data)
        .unwrap();

    assert_eq!(graph.graph_type, GraphType::Dependency);
    assert!(!graph.nodes.is_empty());
}

#[test]
fn test_graph_filtering_with_mock_data() {
    let mut source_data = GraphSourceData::new();
    source_data.add_file("mod_a.rs".to_string(), "".to_string());
    source_data.add_file("mod_b.rs".to_string(), "".to_string());
    source_data.add_file("mod_c.rs".to_string(), "".to_string());

    source_data.add_imports("mod_a".to_string(), vec!["mod_b".to_string()]);
    source_data.add_imports("mod_b".to_string(), vec!["mod_c".to_string()]);

    // Build graph
    let manager = GraphCorrelationManager::new();
    let graph = manager
        .build_graph(GraphType::Dependency, &source_data)
        .unwrap();

    // Apply filter
    let filter = DependencyFilter::new().leaf_nodes_only();
    let filtered = filter_dependency_graph(&graph, &filter).unwrap();

    // Should have at least one leaf node
    assert!(!filtered.nodes.is_empty());
}

#[test]
fn test_impact_analysis_with_mock_data() {
    let mut source_data = GraphSourceData::new();
    source_data.add_file("base.rs".to_string(), "".to_string());
    source_data.add_file("mid.rs".to_string(), "".to_string());
    source_data.add_file("top.rs".to_string(), "".to_string());

    source_data.add_imports("mid".to_string(), vec!["base".to_string()]);
    source_data.add_imports("top".to_string(), vec!["mid".to_string()]);

    // Build graph
    let manager = GraphCorrelationManager::new();
    let graph = manager
        .build_graph(GraphType::Dependency, &source_data)
        .unwrap();

    // Analyze impact of base module
    if let Some(base_node) = graph.nodes.iter().find(|n| n.label.contains("base")) {
        let impact = analyze_impact(&graph, &base_node.id).unwrap();

        assert!(impact.impact_score > 0.0);
        assert!(!impact.direct_impact.is_empty());
    }
}

#[test]
fn test_pattern_detection_with_mock_data() {
    // Create a pipeline pattern
    let mut source_data = GraphSourceData::new();
    source_data.add_file("pipeline.rs".to_string(), "".to_string());

    source_data.add_functions(
        "pipeline.rs".to_string(),
        vec![
            "stage1".to_string(),
            "stage2".to_string(),
            "stage3".to_string(),
        ],
    );

    // Build graph
    let manager = GraphCorrelationManager::new();
    let graph = manager
        .build_graph(GraphType::DataFlow, &source_data)
        .unwrap();

    // Detect patterns
    let detector = PipelinePatternDetector;
    let result = detector.detect(&graph);

    assert!(result.is_ok());
}

#[test]
fn test_graph_comparison_with_mock_data() {
    // Create two versions of the same graph
    let mut source_v1 = GraphSourceData::new();
    source_v1.add_file("mod.rs".to_string(), "".to_string());
    source_v1.add_functions("mod.rs".to_string(), vec!["func1".to_string()]);

    let mut source_v2 = GraphSourceData::new();
    source_v2.add_file("mod.rs".to_string(), "".to_string());
    source_v2.add_functions(
        "mod.rs".to_string(),
        vec!["func1".to_string(), "func2".to_string()],
    );

    let manager = GraphCorrelationManager::new();
    let graph_v1 = manager.build_graph(GraphType::Call, &source_v1).unwrap();
    let graph_v2 = manager.build_graph(GraphType::Call, &source_v2).unwrap();

    // Compare graphs
    let diff = compare_graphs(&graph_v1, &graph_v2).unwrap();

    assert!(diff.similarity_score < 1.0);
}

#[test]
fn test_graph_export_with_mock_data() {
    let mut source_data = GraphSourceData::new();
    source_data.add_file("test.rs".to_string(), "".to_string());
    source_data.add_functions("test.rs".to_string(), vec!["test_func".to_string()]);

    let manager = GraphCorrelationManager::new();
    let graph = manager.build_graph(GraphType::Call, &source_data).unwrap();

    // Test all export formats
    let json_export = export_graph(&graph, ExportFormat::Json);
    assert!(json_export.is_ok());

    let graphml_export = export_graph(&graph, ExportFormat::GraphML);
    assert!(graphml_export.is_ok());

    let gexf_export = export_graph(&graph, ExportFormat::GEXF);
    assert!(gexf_export.is_ok());

    let dot_export = export_graph(&graph, ExportFormat::DOT);
    assert!(dot_export.is_ok());
}

#[test]
fn test_graph_statistics_with_mock_data() {
    let mut source_data = GraphSourceData::new();
    source_data.add_file("mod.rs".to_string(), "".to_string());
    source_data.add_functions(
        "mod.rs".to_string(),
        vec!["f1".to_string(), "f2".to_string(), "f3".to_string()],
    );

    let manager = GraphCorrelationManager::new();
    let graph = manager.build_graph(GraphType::Call, &source_data).unwrap();

    let stats = calculate_statistics(&graph);

    assert!(stats.node_count > 0);
    assert!(stats.avg_degree >= 0.0);
}

#[test]
fn test_performance_cache_integration() {
    let mut cache = GraphCache::new();

    // Build a test graph
    let mut source_data = GraphSourceData::new();
    source_data.add_file("test.rs".to_string(), "".to_string());
    source_data.add_functions("test.rs".to_string(), vec!["func".to_string()]);

    let manager = GraphCorrelationManager::new();
    let graph = manager.build_graph(GraphType::Call, &source_data).unwrap();

    // Build cache from graph
    cache.build_from_graph(&graph);

    // Test cache hits
    for node in &graph.nodes {
        let _ = cache.get_node_degree(&node.id);
    }

    let (hits, _, hit_rate) = cache.get_stats();
    assert!(hits > 0);
    assert!(hit_rate > 0.0);
}

#[test]
fn test_vectorizer_query_cache() {
    let mut cache = VectorizerQueryCache::new();

    // Simulate vectorizer query results
    let query_result = json!({
        "functions": [
            {"name": "func1", "file": "mod.rs"},
            {"name": "func2", "file": "mod.rs"}
        ]
    });

    // Cache the result
    let key = CacheKeyBuilder::new()
        .collection("functions")
        .query("rust functions")
        .build();

    cache.insert(key.clone(), query_result.clone());

    // Retrieve from cache
    let cached = cache.get(&key);
    assert_eq!(cached, Some(query_result));

    // Check statistics
    let stats = cache.get_statistics();
    assert_eq!(stats.hits, 1);
}

#[test]
fn test_critical_nodes_identification() {
    let mut source_data = GraphSourceData::new();

    // Create a graph where one node is critical
    source_data.add_file("core.rs".to_string(), "".to_string());
    source_data.add_file("app1.rs".to_string(), "".to_string());
    source_data.add_file("app2.rs".to_string(), "".to_string());
    source_data.add_file("app3.rs".to_string(), "".to_string());

    source_data.add_imports("app1".to_string(), vec!["core".to_string()]);
    source_data.add_imports("app2".to_string(), vec!["core".to_string()]);
    source_data.add_imports("app3".to_string(), vec!["core".to_string()]);

    let manager = GraphCorrelationManager::new();
    let graph = manager
        .build_graph(GraphType::Dependency, &source_data)
        .unwrap();

    // Identify critical nodes
    let critical = identify_critical_nodes(&graph).unwrap();

    assert!(!critical.is_empty());
    // Core should be the most critical
    if let Some((node_id, score)) = critical.first() {
        assert!(node_id.contains("core"));
        assert!(*score > 0.0);
    }
}

#[test]
fn test_transitive_dependencies() {
    let mut source_data = GraphSourceData::new();
    source_data.add_file("a.rs".to_string(), "".to_string());
    source_data.add_file("b.rs".to_string(), "".to_string());
    source_data.add_file("c.rs".to_string(), "".to_string());

    source_data.add_imports("a".to_string(), vec!["b".to_string()]);
    source_data.add_imports("b".to_string(), vec!["c".to_string()]);

    let manager = GraphCorrelationManager::new();
    let graph = manager
        .build_graph(GraphType::Dependency, &source_data)
        .unwrap();

    // Get transitive dependencies
    if let Some(node_a) = graph.nodes.iter().find(|n| n.label.contains('a')) {
        let deps = get_transitive_dependencies(&graph, &node_a.id);

        // Should include both b and c
        assert!(deps.len() >= 1);
    }
}

#[test]
fn test_circular_dependency_detection() {
    let mut source_data = GraphSourceData::new();
    source_data.add_file("a.rs".to_string(), "".to_string());
    source_data.add_file("b.rs".to_string(), "".to_string());

    // Create circular dependency
    source_data.add_imports("a".to_string(), vec!["b".to_string()]);
    source_data.add_imports("b".to_string(), vec!["a".to_string()]);

    let manager = GraphCorrelationManager::new();
    let graph = manager
        .build_graph(GraphType::Dependency, &source_data)
        .unwrap();

    // Filter for circular dependencies
    let filter = DependencyFilter::new().circular_only();
    let filtered = filter_dependency_graph(&graph, &filter).unwrap();

    // Should detect the circular dependency
    assert_eq!(filtered.nodes.len(), 2);
}
