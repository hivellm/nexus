//! Performance benchmarks for vectorizer data extraction
//!
//! Benchmarks graph correlation operations at various scales

use criterion::{black_box, criterion_group, criterion_main, BenchmarkId, Criterion};
use nexus_core::graph_correlation::*;

/// Create test data with varying sizes
fn create_test_data(num_files: usize, funcs_per_file: usize) -> GraphSourceData {
    let mut source_data = GraphSourceData::new();

    for i in 0..num_files {
        let file_name = format!("file_{}.rs", i);
        let content = format!("// File {}", i);
        source_data.add_file(file_name.clone(), content);

        let mut functions = Vec::new();
        for j in 0..funcs_per_file {
            functions.push(format!("func_{}_{}", i, j));
        }
        source_data.add_functions(file_name, functions);
    }

    source_data
}

/// Benchmark graph building at different scales
fn bench_graph_building(c: &mut Criterion) {
    let mut group = c.benchmark_group("graph_building");

    for size in [10, 50, 100, 500].iter() {
        let source_data = create_test_data(*size, 5);
        let manager = GraphCorrelationManager::new();

        group.bench_with_input(
            BenchmarkId::new("call_graph", size),
            &source_data,
            |b, data| {
                b.iter(|| {
                    manager
                        .build_graph(black_box(GraphType::Call), black_box(data))
                        .unwrap()
                })
            },
        );

        group.bench_with_input(
            BenchmarkId::new("dependency_graph", size),
            &source_data,
            |b, data| {
                b.iter(|| {
                    manager
                        .build_graph(black_box(GraphType::Dependency), black_box(data))
                        .unwrap()
                })
            },
        );
    }

    group.finish();
}

/// Benchmark filtering operations
fn bench_filtering(c: &mut Criterion) {
    let mut group = c.benchmark_group("filtering");

    let source_data = create_test_data(100, 10);
    let manager = GraphCorrelationManager::new();
    let graph = manager
        .build_graph(GraphType::Dependency, &source_data)
        .unwrap();

    group.bench_function("filter_by_node_type", |b| {
        let filter = DependencyFilter::new().with_node_types(vec![NodeType::Module]);
        b.iter(|| filter_dependency_graph(black_box(&graph), black_box(&filter)).unwrap())
    });

    group.bench_function("filter_leaf_nodes", |b| {
        let filter = DependencyFilter::new().leaf_nodes_only();
        b.iter(|| filter_dependency_graph(black_box(&graph), black_box(&filter)).unwrap())
    });

    group.bench_function("filter_circular", |b| {
        let filter = DependencyFilter::new().circular_only();
        b.iter(|| filter_dependency_graph(black_box(&graph), black_box(&filter)).unwrap())
    });

    group.finish();
}

/// Benchmark impact analysis
fn bench_impact_analysis(c: &mut Criterion) {
    let mut group = c.benchmark_group("impact_analysis");

    let source_data = create_test_data(50, 5);
    let manager = GraphCorrelationManager::new();
    let graph = manager
        .build_graph(GraphType::Dependency, &source_data)
        .unwrap();

    if let Some(node) = graph.nodes.first() {
        group.bench_function("analyze_impact", |b| {
            b.iter(|| analyze_impact(black_box(&graph), black_box(&node.id)).unwrap())
        });

        group.bench_function("calculate_propagation", |b| {
            b.iter(|| calculate_propagation_distance(black_box(&graph), black_box(&node.id)))
        });
    }

    group.bench_function("identify_critical", |b| {
        b.iter(|| identify_critical_nodes(black_box(&graph)).unwrap())
    });

    group.finish();
}

/// Benchmark graph comparison
fn bench_graph_comparison(c: &mut Criterion) {
    let mut group = c.benchmark_group("graph_comparison");

    let data1 = create_test_data(50, 5);
    let data2 = create_test_data(55, 5);
    let manager = GraphCorrelationManager::new();
    let graph1 = manager.build_graph(GraphType::Call, &data1).unwrap();
    let graph2 = manager.build_graph(GraphType::Call, &data2).unwrap();

    group.bench_function("compare_graphs", |b| {
        b.iter(|| compare_graphs(black_box(&graph1), black_box(&graph2)).unwrap())
    });

    group.bench_function("structural_similarity", |b| {
        b.iter(|| calculate_structural_similarity(black_box(&graph1), black_box(&graph2)))
    });

    group.finish();
}

/// Benchmark pattern detection
fn bench_pattern_detection(c: &mut Criterion) {
    let mut group = c.benchmark_group("pattern_detection");

    let source_data = create_test_data(30, 10);
    let manager = GraphCorrelationManager::new();
    let graph = manager
        .build_graph(GraphType::DataFlow, &source_data)
        .unwrap();

    group.bench_function("pipeline_detection", |b| {
        let detector = PipelinePatternDetector;
        b.iter(|| detector.detect(black_box(&graph)).unwrap())
    });

    group.bench_function("event_driven_detection", |b| {
        let detector = EventDrivenPatternDetector;
        b.iter(|| detector.detect(black_box(&graph)).unwrap())
    });

    group.bench_function("architectural_detection", |b| {
        let detector = ArchitecturalPatternDetector;
        b.iter(|| detector.detect(black_box(&graph)).unwrap())
    });

    group.finish();
}

/// Benchmark export operations
fn bench_export(c: &mut Criterion) {
    let mut group = c.benchmark_group("export");

    let source_data = create_test_data(50, 5);
    let manager = GraphCorrelationManager::new();
    let graph = manager.build_graph(GraphType::Call, &source_data).unwrap();

    group.bench_function("export_json", |b| {
        b.iter(|| export_graph(black_box(&graph), black_box(ExportFormat::Json)).unwrap())
    });

    group.bench_function("export_graphml", |b| {
        b.iter(|| export_graph(black_box(&graph), black_box(ExportFormat::GraphML)).unwrap())
    });

    group.bench_function("export_gexf", |b| {
        b.iter(|| export_graph(black_box(&graph), black_box(ExportFormat::GEXF)).unwrap())
    });

    group.bench_function("export_dot", |b| {
        b.iter(|| export_graph(black_box(&graph), black_box(ExportFormat::DOT)).unwrap())
    });

    group.finish();
}

/// Benchmark statistics calculation
fn bench_statistics(c: &mut Criterion) {
    let mut group = c.benchmark_group("statistics");

    for size in [50, 100, 500].iter() {
        let source_data = create_test_data(*size, 5);
        let manager = GraphCorrelationManager::new();
        let graph = manager.build_graph(GraphType::Call, &source_data).unwrap();

        group.bench_with_input(BenchmarkId::from_parameter(size), &graph, |b, g| {
            b.iter(|| calculate_statistics(black_box(g)))
        });
    }

    group.finish();
}

/// Benchmark cache operations
fn bench_cache(c: &mut Criterion) {
    let mut group = c.benchmark_group("cache");

    let source_data = create_test_data(100, 10);
    let manager = GraphCorrelationManager::new();
    let graph = manager.build_graph(GraphType::Call, &source_data).unwrap();

    group.bench_function("cache_build", |b| {
        b.iter(|| {
            let mut cache = GraphCache::new();
            cache.build_from_graph(black_box(&graph))
        })
    });

    group.bench_function("cache_lookups", |b| {
        let mut cache = GraphCache::new();
        cache.build_from_graph(&graph);
        
        b.iter(|| {
            for node in &graph.nodes {
                let _ = cache.get_node_degree(black_box(&node.id));
            }
        })
    });

    group.bench_function("vectorizer_cache_insert", |b| {
        let mut cache = VectorizerQueryCache::new();
        b.iter(|| {
            for i in 0..100 {
                cache.insert(
                    black_box(format!("key_{}", i)),
                    black_box(serde_json::json!({"data": i})),
                );
            }
        })
    });

    group.bench_function("vectorizer_cache_get", |b| {
        let mut cache = VectorizerQueryCache::new();
        for i in 0..100 {
            cache.insert(format!("key_{}", i), serde_json::json!({"data": i}));
        }
        
        b.iter(|| {
            for i in 0..100 {
                let _ = cache.get(black_box(&format!("key_{}", i)));
            }
        })
    });

    group.finish();
}

/// Benchmark optimization operations
fn bench_optimization(c: &mut Criterion) {
    let mut group = c.benchmark_group("optimization");

    let source_data = create_test_data(100, 10);
    let manager = GraphCorrelationManager::new();
    let mut graph = manager.build_graph(GraphType::Call, &source_data).unwrap();

    // Add some duplicate edges for testing
    if graph.edges.len() > 0 {
        let first_edge = graph.edges[0].clone();
        graph.edges.push(first_edge.clone());
        graph.edges.push(first_edge);
    }

    group.bench_function("optimize_graph", |b| {
        b.iter(|| {
            let mut g = graph.clone();
            optimize_graph(black_box(&mut g)).unwrap()
        })
    });

    group.bench_function("calculate_complexity", |b| {
        b.iter(|| calculate_complexity(black_box(&graph)))
    });

    group.finish();
}

criterion_group!(
    benches,
    bench_graph_building,
    bench_filtering,
    bench_impact_analysis,
    bench_graph_comparison,
    bench_pattern_detection,
    bench_export,
    bench_statistics,
    bench_cache,
    bench_optimization
);

criterion_main!(benches);

