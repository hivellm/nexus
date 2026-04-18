//! Node Clustering and Grouping Example
//!
//! This example demonstrates various clustering algorithms and grouping strategies
//! available in the Nexus graph database engine.

use nexus_core::{
    ClusteringAlgorithm, ClusteringConfig, ClusteringEngine, DistanceMetric, FeatureStrategy,
    LinkageType, PropertyValue, SimpleGraph,
};
use tracing;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    tracing::info!("=== Nexus Node Clustering and Grouping Example ===\n");

    // Create a sample graph with diverse nodes
    let graph = create_sample_graph()?;
    tracing::info!("Created sample graph with {} nodes", graph.node_count()?);

    // Example 1: Label-based grouping
    tracing::info!("\n1. Label-based Grouping");
    tracing::info!("========================");
    let config = ClusteringConfig {
        algorithm: ClusteringAlgorithm::LabelBased,
        feature_strategy: FeatureStrategy::LabelBased,
        distance_metric: DistanceMetric::Euclidean,
        random_seed: Some(42),
    };
    let engine = ClusteringEngine::new(config);
    let result = engine.cluster(&graph)?;
    print_clustering_result(&result);

    // Example 2: Property-based grouping
    tracing::info!("\n2. Property-based Grouping (by department)");
    tracing::info!("===========================================");
    let config = ClusteringConfig {
        algorithm: ClusteringAlgorithm::PropertyBased {
            property_key: "department".to_string(),
        },
        feature_strategy: FeatureStrategy::PropertyBased {
            property_keys: vec!["department".to_string()],
        },
        distance_metric: DistanceMetric::Euclidean,
        random_seed: Some(42),
    };
    let engine = ClusteringEngine::new(config);
    let result = engine.cluster(&graph)?;
    print_clustering_result(&result);

    // Example 3: K-means clustering
    tracing::info!("\n3. K-means Clustering (k=3)");
    tracing::info!("===========================");
    let config = ClusteringConfig {
        algorithm: ClusteringAlgorithm::KMeans {
            k: 3,
            max_iterations: 50,
        },
        feature_strategy: FeatureStrategy::Structural,
        distance_metric: DistanceMetric::Euclidean,
        random_seed: Some(42),
    };
    let engine = ClusteringEngine::new(config);
    let result = engine.cluster(&graph)?;
    print_clustering_result(&result);

    // Example 4: Hierarchical clustering
    tracing::info!("\n4. Hierarchical Clustering");
    tracing::info!("==========================");
    let config = ClusteringConfig {
        algorithm: ClusteringAlgorithm::Hierarchical {
            linkage: LinkageType::Average,
        },
        feature_strategy: FeatureStrategy::Structural,
        distance_metric: DistanceMetric::Euclidean,
        random_seed: Some(42),
    };
    let engine = ClusteringEngine::new(config);
    let result = engine.cluster(&graph)?;
    print_clustering_result(&result);

    // Example 5: Community detection
    tracing::info!("\n5. Community Detection");
    tracing::info!("=====================");
    let config = ClusteringConfig {
        algorithm: ClusteringAlgorithm::CommunityDetection,
        feature_strategy: FeatureStrategy::Structural,
        distance_metric: DistanceMetric::Euclidean,
        random_seed: Some(42),
    };
    let engine = ClusteringEngine::new(config);
    let result = engine.cluster(&graph)?;
    print_clustering_result(&result);

    // Example 6: DBSCAN clustering
    tracing::info!("\n6. DBSCAN Clustering");
    tracing::info!("===================");
    let config = ClusteringConfig {
        algorithm: ClusteringAlgorithm::DBSCAN {
            eps: 2.0,
            min_points: 2,
        },
        feature_strategy: FeatureStrategy::Structural,
        distance_metric: DistanceMetric::Euclidean,
        random_seed: Some(42),
    };
    let engine = ClusteringEngine::new(config);
    let result = engine.cluster(&graph)?;
    print_clustering_result(&result);

    // Example 7: Combined feature strategy
    tracing::info!("\n7. Combined Feature Strategy Clustering");
    tracing::info!("=======================================");
    let config = ClusteringConfig {
        algorithm: ClusteringAlgorithm::KMeans {
            k: 2,
            max_iterations: 30,
        },
        feature_strategy: FeatureStrategy::Combined {
            strategies: vec![
                FeatureStrategy::LabelBased,
                FeatureStrategy::PropertyBased {
                    property_keys: vec!["age".to_string(), "salary".to_string()],
                },
                FeatureStrategy::Structural,
            ],
        },
        distance_metric: DistanceMetric::Cosine,
        random_seed: Some(42),
    };
    let engine = ClusteringEngine::new(config);
    let result = engine.cluster(&graph)?;
    print_clustering_result(&result);

    // Example 8: Different distance metrics comparison
    tracing::info!("\n8. Distance Metrics Comparison");
    tracing::info!("==============================");
    let distance_metrics = vec![
        (DistanceMetric::Euclidean, "Euclidean"),
        (DistanceMetric::Manhattan, "Manhattan"),
        (DistanceMetric::Cosine, "Cosine"),
        (DistanceMetric::Jaccard, "Jaccard"),
        (DistanceMetric::Hamming, "Hamming"),
    ];

    for (metric, name) in distance_metrics {
        let config = ClusteringConfig {
            algorithm: ClusteringAlgorithm::KMeans {
                k: 2,
                max_iterations: 20,
            },
            feature_strategy: FeatureStrategy::Structural,
            distance_metric: metric,
            random_seed: Some(42),
        };
        let engine = ClusteringEngine::new(config);
        let result = engine.cluster(&graph)?;
        tracing::info!("\n{} Distance:", name);
        tracing::info!("  Clusters: {}", result.clusters.len());
        tracing::info!("  Silhouette Score: {:.3}", result.metrics.silhouette_score);
        tracing::info!("  WCSS: {:.3}", result.metrics.wcss);
        tracing::info!("  BCSS: {:.3}", result.metrics.bcss);
    }

    tracing::info!("\n=== Clustering Example Complete ===");
    Ok(())
}

/// Create a sample graph with diverse nodes for clustering demonstration
fn create_sample_graph() -> Result<SimpleGraph, Box<dyn std::error::Error>> {
    let mut graph = SimpleGraph::new();

    // Create employees with different roles and departments
    let employees = vec![
        (
            "Alice",
            "Engineering",
            28,
            75000,
            vec!["Person", "Employee", "Developer"],
        ),
        (
            "Bob",
            "Engineering",
            32,
            85000,
            vec!["Person", "Employee", "SeniorDeveloper"],
        ),
        (
            "Charlie",
            "Engineering",
            25,
            65000,
            vec!["Person", "Employee", "JuniorDeveloper"],
        ),
        (
            "Diana",
            "Management",
            35,
            95000,
            vec!["Person", "Manager", "TeamLead"],
        ),
        (
            "Eve",
            "Management",
            40,
            110000,
            vec!["Person", "Manager", "Director"],
        ),
        (
            "Frank",
            "Sales",
            30,
            70000,
            vec!["Person", "Employee", "SalesRep"],
        ),
        (
            "Grace",
            "Sales",
            28,
            72000,
            vec!["Person", "Employee", "SalesRep"],
        ),
        (
            "Henry",
            "Marketing",
            26,
            68000,
            vec!["Person", "Employee", "MarketingSpecialist"],
        ),
        (
            "Ivy",
            "Marketing",
            29,
            75000,
            vec!["Person", "Employee", "MarketingManager"],
        ),
        (
            "Jack",
            "HR",
            33,
            80000,
            vec!["Person", "Employee", "HRSpecialist"],
        ),
    ];

    for (name, department, age, salary, labels) in employees {
        let labels: Vec<String> = labels.into_iter().map(|s| s.to_string()).collect();
        let node_id = graph.create_node(labels)?;

        // Add properties
        if let Some(node) = graph.get_node_mut(node_id)? {
            node.set_property("name".to_string(), PropertyValue::String(name.to_string()));
            node.set_property(
                "department".to_string(),
                PropertyValue::String(department.to_string()),
            );
            node.set_property("age".to_string(), PropertyValue::Int64(age));
            node.set_property("salary".to_string(), PropertyValue::Int64(salary));
            node.set_property(
                "experience_years".to_string(),
                PropertyValue::Int64(age - 22),
            ); // Assume started at 22
        }
    }

    // Create some companies
    let companies = vec![
        ("TechCorp", "Technology", vec!["Company", "TechCompany"]),
        ("FinanceInc", "Finance", vec!["Company", "FinanceCompany"]),
        ("RetailCo", "Retail", vec!["Company", "RetailCompany"]),
    ];

    for (name, industry, labels) in companies {
        let labels: Vec<String> = labels.into_iter().map(|s| s.to_string()).collect();
        let node_id = graph.create_node(labels)?;

        if let Some(node) = graph.get_node_mut(node_id)? {
            node.set_property("name".to_string(), PropertyValue::String(name.to_string()));
            node.set_property(
                "industry".to_string(),
                PropertyValue::String(industry.to_string()),
            );
            node.set_property(
                "employee_count".to_string(),
                PropertyValue::Int64(100 + (name.len() as i64 * 10)),
            );
        }
    }

    // Create some projects
    let projects = vec![
        ("ProjectAlpha", "Active", vec!["Project", "ActiveProject"]),
        (
            "ProjectBeta",
            "Completed",
            vec!["Project", "CompletedProject"],
        ),
        (
            "ProjectGamma",
            "Planning",
            vec!["Project", "PlanningProject"],
        ),
    ];

    for (name, status, labels) in projects {
        let labels: Vec<String> = labels.into_iter().map(|s| s.to_string()).collect();
        let node_id = graph.create_node(labels)?;

        if let Some(node) = graph.get_node_mut(node_id)? {
            node.set_property("name".to_string(), PropertyValue::String(name.to_string()));
            node.set_property(
                "status".to_string(),
                PropertyValue::String(status.to_string()),
            );
            node.set_property(
                "budget".to_string(),
                PropertyValue::Int64(50000 + (name.len() as i64 * 1000)),
            );
        }
    }

    // Add some relationships to create structure
    let all_nodes: Vec<_> = graph.get_all_nodes()?.into_iter().map(|n| n.id).collect();

    // Connect some employees to companies
    for i in 0..3 {
        if i < all_nodes.len() && i + 10 < all_nodes.len() {
            let _ = graph.create_edge(all_nodes[i], all_nodes[i + 10], "WORKS_FOR".to_string());
        }
    }

    // Connect some employees to projects
    for i in 3..6 {
        if i < all_nodes.len() && i + 7 < all_nodes.len() {
            let _ = graph.create_edge(all_nodes[i], all_nodes[i + 7], "WORKS_ON".to_string());
        }
    }

    // Connect some projects to companies
    for i in 10..13 {
        if i < all_nodes.len() && i + 3 < all_nodes.len() {
            let _ = graph.create_edge(all_nodes[i], all_nodes[i + 3], "SPONSORED_BY".to_string());
        }
    }

    Ok(graph)
}

/// Print clustering results in a formatted way
fn print_clustering_result(result: &nexus_core::ClusteringResult) {
    tracing::info!("  Algorithm: {:?}", result.algorithm);
    tracing::info!("  Clusters: {}", result.clusters.len());
    tracing::info!("  Iterations: {}", result.iterations);
    tracing::info!("  Converged: {}", result.converged);
    tracing::info!("  Quality Metrics:");
    tracing::info!(
        "    Silhouette Score: {:.3}",
        result.metrics.silhouette_score
    );
    tracing::info!("    WCSS: {:.3}", result.metrics.wcss);
    tracing::info!("    BCSS: {:.3}", result.metrics.bcss);
    tracing::info!(
        "    Calinski-Harabasz: {:.3}",
        result.metrics.calinski_harabasz
    );
    tracing::info!("    Davies-Bouldin: {:.3}", result.metrics.davies_bouldin);

    tracing::info!("  Cluster Details:");
    for (i, cluster) in result.clusters.iter().enumerate() {
        tracing::info!("    Cluster {}: {} nodes", i, cluster.size());
        if let Some(centroid) = &cluster.centroid {
            tracing::info!("      Centroid: {:?}", centroid);
        }
        if !cluster.metadata.is_empty() {
            tracing::info!("      Metadata: {:?}", cluster.metadata);
        }
    }
}
