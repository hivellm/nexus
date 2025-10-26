//! Graph Correlation Performance Benchmark
//!
//! This benchmark measures the performance of graph correlation analysis:
//! - Graph generation speed
//! - Memory usage during graph building
//! - Different graph types (Call, Dependency, DataFlow, Component)
//! - Large-scale graph handling

use criterion::{BenchmarkId, Criterion, black_box, criterion_group, criterion_main};
use nexus_core::graph_correlation::{
    CorrelationGraph, GraphCorrelationManager, GraphSourceData, GraphType,
};
use std::collections::HashMap;

fn benchmark_call_graph_generation(c: &mut Criterion) {
    let manager = GraphCorrelationManager::new();

    // Test different scales
    let scales = vec![10, 50, 100, 500, 1000];

    let mut group = c.benchmark_group("call_graph_generation");
    group.sample_size(10);

    for scale in scales {
        group.bench_with_input(
            BenchmarkId::from_parameter(format!("{}_nodes", scale)),
            &scale,
            |b, &scale| {
                // Generate test data
                let mut source_data = create_test_data(scale, GraphType::Call);

                b.iter(|| {
                    let result = manager.build_graph(GraphType::Call, black_box(&source_data));
                    assert!(result.is_ok());
                    black_box(result)
                });
            },
        );
    }
    group.finish();
}

fn benchmark_dependency_graph_generation(c: &mut Criterion) {
    let manager = GraphCorrelationManager::new();

    let scales = vec![10, 50, 100, 500];

    let mut group = c.benchmark_group("dependency_graph_generation");
    group.sample_size(10);

    for scale in scales {
        group.bench_with_input(
            BenchmarkId::from_parameter(format!("{}_nodes", scale)),
            &scale,
            |b, &scale| {
                let mut source_data = create_test_data(scale, GraphType::Dependency);

                b.iter(|| {
                    let result =
                        manager.build_graph(GraphType::Dependency, black_box(&source_data));
                    assert!(result.is_ok());
                    black_box(result)
                });
            },
        );
    }
    group.finish();
}

fn benchmark_graph_filtering(c: &mut Criterion) {
    let manager = GraphCorrelationManager::new();

    // Create a larger graph for filtering benchmarks
    let mut source_data = create_test_data(100, GraphType::Call);
    let graph = manager.build_graph(GraphType::Call, &source_data).unwrap();

    let mut group = c.benchmark_group("graph_filtering");
    group.sample_size(100);

    // Benchmark filter by function name
    group.bench_function("filter_by_function_name", |b| {
        b.iter(|| {
            let filtered: Vec<_> = graph
                .nodes
                .iter()
                .filter(|node| node.name.contains("func"))
                .collect();
            black_box(filtered)
        });
    });

    // Benchmark filter by depth
    group.bench_function("filter_by_depth", |b| {
        b.iter(|| {
            let filtered: Vec<_> = graph
                .nodes
                .iter()
                .filter(|node| node.hierarchical_info.depth <= 2)
                .collect();
            black_box(filtered)
        });
    });

    group.finish();
}

fn create_test_data(scale: usize, graph_type: GraphType) -> GraphSourceData {
    let mut source_data = GraphSourceData::new();

    match graph_type {
        GraphType::Call => {
            // Create function call relationships
            for i in 0..scale {
                let file_path = format!("src/file_{}.rs", i);
                let content = format!("pub fn func_{}() {{ }}", i);
                source_data.add_file(file_path.clone(), content);

                let mut functions = vec![format!("func_{}", i)];
                if i > 0 {
                    functions.push(format!("calls_func_{}", i - 1));
                }
                source_data.add_functions(file_path, functions);
            }
        }
        GraphType::Dependency => {
            // Create module dependencies
            for i in 0..scale {
                let file_path = format!("src/module_{}.rs", i);
                let content = format!("pub mod module_{} {{ }}", i);
                source_data.add_file(file_path.clone(), content);

                let imports = if i > 0 {
                    vec![format!("module_{}", i - 1)]
                } else {
                    vec![]
                };
                source_data.add_imports(file_path, imports);
            }
        }
        _ => {
            // Generic test data
            for i in 0..scale {
                let file_path = format!("src/test_{}.rs", i);
                let content = format!("test content {}", i);
                source_data.add_file(file_path, content);
            }
        }
    }

    source_data
}

criterion_group!(
    benches,
    benchmark_call_graph_generation,
    benchmark_dependency_graph_generation,
    benchmark_graph_filtering
);
criterion_main!(benches);
