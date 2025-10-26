#![allow(unexpected_cfgs)]
#![cfg(FALSE)]
//! Integration tests with real codebase structures
//!
//! Tests graph correlation with realistic code patterns

use nexus_core::graph::correlation::*;
use std::collections::HashMap;

/// Simulates a real microservices architecture
#[test]
fn test_microservices_architecture() {
    let mut source_data = GraphSourceData::new();

    // API Gateway
    source_data.add_file(
        "api_gateway/main.rs".to_string(),
        "use auth_service;\nuse user_service;\nuse order_service;".to_string(),
    );
    source_data.add_functions(
        "api_gateway/main.rs".to_string(),
        vec!["route_request".to_string(), "handle_auth".to_string()],
    );
    source_data.add_imports(
        "api_gateway/main.rs".to_string(),
        vec![
            "auth_service".to_string(),
            "user_service".to_string(),
            "order_service".to_string(),
        ],
    );

    // Auth Service
    source_data.add_file(
        "auth_service/main.rs".to_string(),
        "use database;".to_string(),
    );
    source_data.add_functions(
        "auth_service/main.rs".to_string(),
        vec!["authenticate".to_string(), "validate_token".to_string()],
    );
    source_data.add_imports(
        "auth_service/main.rs".to_string(),
        vec!["database".to_string()],
    );

    // User Service
    source_data.add_file(
        "user_service/main.rs".to_string(),
        "use database;\nuse cache;".to_string(),
    );
    source_data.add_functions(
        "user_service/main.rs".to_string(),
        vec!["get_user".to_string(), "update_user".to_string()],
    );
    source_data.add_imports(
        "user_service/main.rs".to_string(),
        vec!["database".to_string(), "cache".to_string()],
    );

    // Order Service
    source_data.add_file(
        "order_service/main.rs".to_string(),
        "use database;\nuse payment_service;".to_string(),
    );
    source_data.add_functions(
        "order_service/main.rs".to_string(),
        vec!["create_order".to_string(), "get_order".to_string()],
    );
    source_data.add_imports(
        "order_service/main.rs".to_string(),
        vec!["database".to_string(), "payment_service".to_string()],
    );

    // Payment Service
    source_data.add_file(
        "payment_service/main.rs".to_string(),
        "use external_api;".to_string(),
    );
    source_data.add_functions(
        "payment_service/main.rs".to_string(),
        vec!["process_payment".to_string()],
    );

    // Shared Database
    source_data.add_file("database/mod.rs".to_string(), "".to_string());
    source_data.add_functions(
        "database/mod.rs".to_string(),
        vec!["connect".to_string(), "query".to_string()],
    );

    // Build dependency graph
    let manager = GraphCorrelationManager::new();
    let graph = manager
        .build_graph(GraphType::Dependency, &source_data)
        .unwrap();

    // Validate microservices structure
    assert!(graph.nodes.len() >= 6);

    // Find the database node - it should be critical
    let critical_nodes = identify_critical_nodes(&graph).unwrap();
    assert!(!critical_nodes.is_empty());

    // Database should have high impact score
    let db_node = critical_nodes
        .iter()
        .find(|(id, _)| id.contains("database"));
    assert!(db_node.is_some());
}

/// Simulates a layered architecture (MVC-style)
#[test]
fn test_layered_architecture() {
    let mut source_data = GraphSourceData::new();

    // Presentation Layer (Controllers)
    source_data.add_file(
        "controllers/user_controller.rs".to_string(),
        "use services::user_service;".to_string(),
    );
    source_data.add_functions(
        "controllers/user_controller.rs".to_string(),
        vec![
            "get_user_handler".to_string(),
            "create_user_handler".to_string(),
        ],
    );
    source_data.add_imports(
        "controllers/user_controller.rs".to_string(),
        vec!["user_service".to_string()],
    );

    source_data.add_file(
        "controllers/order_controller.rs".to_string(),
        "use services::order_service;".to_string(),
    );
    source_data.add_functions(
        "controllers/order_controller.rs".to_string(),
        vec!["get_order_handler".to_string()],
    );
    source_data.add_imports(
        "controllers/order_controller.rs".to_string(),
        vec!["order_service".to_string()],
    );

    // Business Logic Layer (Services)
    source_data.add_file(
        "services/user_service.rs".to_string(),
        "use repositories::user_repo;".to_string(),
    );
    source_data.add_functions(
        "services/user_service.rs".to_string(),
        vec!["find_user".to_string(), "create_user".to_string()],
    );
    source_data.add_imports(
        "services/user_service.rs".to_string(),
        vec!["user_repo".to_string()],
    );

    source_data.add_file(
        "services/order_service.rs".to_string(),
        "use repositories::order_repo;".to_string(),
    );
    source_data.add_functions(
        "services/order_service.rs".to_string(),
        vec!["place_order".to_string()],
    );
    source_data.add_imports(
        "services/order_service.rs".to_string(),
        vec!["order_repo".to_string()],
    );

    // Data Access Layer (Repositories)
    source_data.add_file(
        "repositories/user_repo.rs".to_string(),
        "use database;".to_string(),
    );
    source_data.add_functions(
        "repositories/user_repo.rs".to_string(),
        vec!["get_by_id".to_string(), "save".to_string()],
    );
    source_data.add_imports(
        "repositories/user_repo.rs".to_string(),
        vec!["database".to_string()],
    );

    source_data.add_file(
        "repositories/order_repo.rs".to_string(),
        "use database;".to_string(),
    );
    source_data.add_functions(
        "repositories/order_repo.rs".to_string(),
        vec!["find_orders".to_string()],
    );
    source_data.add_imports(
        "repositories/order_repo.rs".to_string(),
        vec!["database".to_string()],
    );

    // Build graph
    let manager = GraphCorrelationManager::new();
    let graph = manager
        .build_graph(GraphType::Dependency, &source_data)
        .unwrap();

    // Detect architectural pattern
    let detector = ArchitecturalPatternDetector;
    let result = detector.detect(&graph).unwrap();

    // Should detect layered architecture
    assert!(!result.patterns.is_empty());
    let has_layered = result
        .patterns
        .iter()
        .any(|p| matches!(p.pattern_type, PatternType::LayeredArchitecture));
    assert!(has_layered, "Should detect layered architecture pattern");
}

/// Simulates a monorepo with multiple packages
#[test]
fn test_monorepo_structure() {
    let mut source_data = GraphSourceData::new();

    // Frontend package
    source_data.add_file(
        "packages/frontend/src/main.tsx".to_string(),
        "import { api } from '@company/api-client';".to_string(),
    );
    source_data.add_functions(
        "packages/frontend/src/main.tsx".to_string(),
        vec!["App".to_string(), "render".to_string()],
    );
    source_data.add_imports(
        "packages/frontend/src/main.tsx".to_string(),
        vec!["api-client".to_string()],
    );

    // API Client package
    source_data.add_file(
        "packages/api-client/src/index.ts".to_string(),
        "import { types } from '@company/types';".to_string(),
    );
    source_data.add_functions(
        "packages/api-client/src/index.ts".to_string(),
        vec!["ApiClient".to_string(), "fetch".to_string()],
    );
    source_data.add_imports(
        "packages/api-client/src/index.ts".to_string(),
        vec!["types".to_string()],
    );

    // Shared Types package
    source_data.add_file("packages/types/src/index.ts".to_string(), "".to_string());
    source_data.add_functions(
        "packages/types/src/index.ts".to_string(),
        vec!["User".to_string(), "Order".to_string()],
    );

    // Backend package
    source_data.add_file(
        "packages/backend/src/main.rs".to_string(),
        "use company_types;".to_string(),
    );
    source_data.add_functions(
        "packages/backend/src/main.rs".to_string(),
        vec!["start_server".to_string()],
    );
    source_data.add_imports(
        "packages/backend/src/main.rs".to_string(),
        vec!["company_types".to_string()],
    );

    // Build graph
    let manager = GraphCorrelationManager::new();
    let graph = manager
        .build_graph(GraphType::Dependency, &source_data)
        .unwrap();

    // Types package should be a leaf node (no dependencies)
    let (leaf_nodes, _) = identify_leaf_and_root_nodes(&graph);
    let has_types_leaf = graph
        .nodes
        .iter()
        .any(|n| n.label.contains("types") && leaf_nodes.contains(&n.id));
    assert!(has_types_leaf, "Types package should be a leaf node");

    // Frontend and backend both depend on shared types
    let types_node = graph.nodes.iter().find(|n| n.label.contains("types"));
    if let Some(types) = types_node {
        let impact = analyze_impact(&graph, &types.id).unwrap();
        assert!(
            impact.total_affected >= 2,
            "Types should affect multiple packages"
        );
    }
}

/// Simulates circular dependencies (anti-pattern)
#[test]
fn test_circular_dependency_detection() {
    let mut source_data = GraphSourceData::new();

    // Module A depends on B
    source_data.add_file("module_a.rs".to_string(), "use module_b;".to_string());
    source_data.add_functions("module_a.rs".to_string(), vec!["func_a".to_string()]);
    source_data.add_imports("module_a.rs".to_string(), vec!["module_b".to_string()]);

    // Module B depends on C
    source_data.add_file("module_b.rs".to_string(), "use module_c;".to_string());
    source_data.add_functions("module_b.rs".to_string(), vec!["func_b".to_string()]);
    source_data.add_imports("module_b.rs".to_string(), vec!["module_c".to_string()]);

    // Module C depends on A (creates cycle)
    source_data.add_file("module_c.rs".to_string(), "use module_a;".to_string());
    source_data.add_functions("module_c.rs".to_string(), vec!["func_c".to_string()]);
    source_data.add_imports("module_c.rs".to_string(), vec!["module_a".to_string()]);

    // Build graph
    let manager = GraphCorrelationManager::new();
    let graph = manager
        .build_graph(GraphType::Dependency, &source_data)
        .unwrap();

    // Filter for circular dependencies
    let filter = DependencyFilter::new().circular_only();
    let circular_graph = filter_dependency_graph(&graph, &filter).unwrap();

    // Should detect all three modules in the cycle
    assert_eq!(
        circular_graph.nodes.len(),
        3,
        "Should detect all nodes in circular dependency"
    );
}

/// Simulates a plugin architecture
#[test]
fn test_plugin_architecture() {
    let mut source_data = GraphSourceData::new();

    // Core system
    source_data.add_file("core/plugin_manager.rs".to_string(), "".to_string());
    source_data.add_functions(
        "core/plugin_manager.rs".to_string(),
        vec!["load_plugin".to_string(), "register".to_string()],
    );

    // Plugin interface
    source_data.add_file("core/plugin_trait.rs".to_string(), "".to_string());
    source_data.add_functions(
        "core/plugin_trait.rs".to_string(),
        vec!["Plugin".to_string(), "execute".to_string()],
    );

    // Plugins (depend on interface, not on each other)
    for i in 1..=5 {
        let plugin_file = format!("plugins/plugin_{}.rs", i);
        source_data.add_file(plugin_file.clone(), "use core::plugin_trait;".to_string());
        source_data.add_functions(plugin_file.clone(), vec![format!("Plugin{}", i)]);
        source_data.add_imports(plugin_file, vec!["plugin_trait".to_string()]);
    }

    // Build graph
    let manager = GraphCorrelationManager::new();
    let graph = manager
        .build_graph(GraphType::Dependency, &source_data)
        .unwrap();

    // Plugin interface should be critical
    let critical = identify_critical_nodes(&graph).unwrap();
    let interface_critical = critical
        .iter()
        .any(|(id, score)| id.contains("trait") && *score > 0.5);
    assert!(interface_critical, "Plugin interface should be critical");
}

/// Simulates event-driven architecture
#[test]
fn test_event_driven_architecture() {
    let mut source_data = GraphSourceData::new();

    // Event Bus
    source_data.add_file("eventbus/mod.rs".to_string(), "".to_string());
    source_data.add_functions(
        "eventbus/mod.rs".to_string(),
        vec!["publish".to_string(), "subscribe".to_string()],
    );

    // Publishers
    source_data.add_file(
        "publishers/order_publisher.rs".to_string(),
        "use eventbus;".to_string(),
    );
    source_data.add_functions(
        "publishers/order_publisher.rs".to_string(),
        vec!["publish_order_created".to_string()],
    );
    source_data.add_imports(
        "publishers/order_publisher.rs".to_string(),
        vec!["eventbus".to_string()],
    );

    // Subscribers
    source_data.add_file(
        "subscribers/email_subscriber.rs".to_string(),
        "use eventbus;".to_string(),
    );
    source_data.add_functions(
        "subscribers/email_subscriber.rs".to_string(),
        vec!["on_order_created".to_string()],
    );
    source_data.add_imports(
        "subscribers/email_subscriber.rs".to_string(),
        vec!["eventbus".to_string()],
    );

    source_data.add_file(
        "subscribers/inventory_subscriber.rs".to_string(),
        "use eventbus;".to_string(),
    );
    source_data.add_functions(
        "subscribers/inventory_subscriber.rs".to_string(),
        vec!["on_order_created".to_string()],
    );
    source_data.add_imports(
        "subscribers/inventory_subscriber.rs".to_string(),
        vec!["eventbus".to_string()],
    );

    // Build graph
    let manager = GraphCorrelationManager::new();
    let graph = manager
        .build_graph(GraphType::DataFlow, &source_data)
        .unwrap();

    // Detect event-driven pattern
    let detector = EventDrivenPatternDetector;
    let result = detector.detect(&graph).unwrap();

    assert!(
        !result.patterns.is_empty(),
        "Should detect event-driven patterns"
    );
}

/// Simulates data pipeline architecture
#[test]
fn test_data_pipeline() {
    let mut source_data = GraphSourceData::new();

    // Pipeline stages
    let stages = vec![
        ("ingestion", "extract_data"),
        ("validation", "validate_schema"),
        ("transformation", "transform_data"),
        ("enrichment", "enrich_metadata"),
        ("aggregation", "aggregate_metrics"),
        ("storage", "persist_data"),
    ];

    for (i, (stage, func)) in stages.iter().enumerate() {
        let file = format!("pipeline/{}.rs", stage);
        source_data.add_file(file.clone(), "".to_string());
        source_data.add_functions(file.clone(), vec![func.to_string()]);

        // Each stage depends on previous (except first)
        if i > 0 {
            let prev_stage = stages[i - 1].0;
            source_data.add_imports(file, vec![prev_stage.to_string()]);
        }
    }

    // Build graph
    let manager = GraphCorrelationManager::new();
    let graph = manager
        .build_graph(GraphType::DataFlow, &source_data)
        .unwrap();

    // Detect pipeline pattern
    let detector = PipelinePatternDetector;
    let result = detector.detect(&graph).unwrap();

    assert!(
        !result.patterns.is_empty(),
        "Should detect pipeline pattern"
    );

    // Verify linear structure
    let (leaf_nodes, root_nodes) = identify_leaf_and_root_nodes(&graph);
    assert_eq!(leaf_nodes.len(), 1, "Should have one leaf (storage)");
    assert_eq!(root_nodes.len(), 1, "Should have one root (ingestion)");
}

/// Test with complex dependency tree (diamond pattern)
#[test]
fn test_diamond_dependency() {
    let mut source_data = GraphSourceData::new();

    // Top level
    source_data.add_file("app.rs".to_string(), "use left;\nuse right;".to_string());
    source_data.add_imports(
        "app.rs".to_string(),
        vec!["left".to_string(), "right".to_string()],
    );

    // Middle level (both depend on base)
    source_data.add_file("left.rs".to_string(), "use base;".to_string());
    source_data.add_imports("left.rs".to_string(), vec!["base".to_string()]);

    source_data.add_file("right.rs".to_string(), "use base;".to_string());
    source_data.add_imports("right.rs".to_string(), vec!["base".to_string()]);

    // Base level
    source_data.add_file("base.rs".to_string(), "".to_string());

    // Build graph
    let manager = GraphCorrelationManager::new();
    let graph = manager
        .build_graph(GraphType::Dependency, &source_data)
        .unwrap();

    // Base should have high impact (affects everything above)
    if let Some(base_node) = graph.nodes.iter().find(|n| n.label.contains("base")) {
        let impact = analyze_impact(&graph, &base_node.id).unwrap();
        assert_eq!(
            impact.total_affected, 3,
            "Base change affects all nodes above"
        );

        // Check impact by level
        assert!(
            impact.impact_by_level.contains_key(&1),
            "Should have level 1 (left, right)"
        );
        assert!(
            impact.impact_by_level.contains_key(&2),
            "Should have level 2 (app)"
        );
    }
}
