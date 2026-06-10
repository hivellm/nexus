//! Graph Correlation Analysis - Code relationship visualization & LLM assistance
//!
//! This module provides functionality to automatically build correlation graphs
//! between files, function calls, and libraries to help LLMs understand processing flow.
//!
//! # Graph Types
//!
//! - **Call Graph**: Function call relationships and execution flow
//! - **Dependency Graph**: Module/library dependencies and imports
//! - **Data Flow Graph**: Data transformation and variable usage
//! - **Component Graph**: High-level architectural components
//!
//! # Features
//!
//! - Automatic graph generation from vectorizer data
//! - Pattern recognition (pipelines, event-driven, architectural)
//! - LLM assistance with graph context
//! - Interactive visualization support
//! - Real-time graph updates
//! - Multiple export formats (JSON, GraphML, GEXF)

use crate::vectorizer_cache::{QueryMetadata, VectorizerCache};
use crate::{Error, Result};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

// ---------------------------------------------------------------------------
// Submodule declarations
// ---------------------------------------------------------------------------

/// Hierarchical call graph layout algorithms
pub mod hierarchical_layout;

/// Graph visualization and rendering
pub mod visualization;

/// Call graph filtering and search functionality
pub mod call_graph_filtering;

/// Pattern recognition for architectural and design patterns
pub mod pattern_recognition;

/// Graph export to multiple formats
pub mod graph_export;

/// Graph statistics and metrics
pub mod graph_statistics;

/// Graph comparison and diff
pub mod graph_diff;

/// Performance optimization utilities
pub mod performance;

/// Dependency graph filtering
pub mod dependency_filter;

/// Dependency impact analysis
pub mod impact_analysis;

/// Enhanced vectorizer query caching
pub mod vectorizer_cache;

/// Version constraint analysis for dependencies
pub mod version_constraints;

/// Data flow analysis and tracking
pub mod data_flow;

/// Component analysis for object-oriented code
pub mod component;

/// Query execution against the vectorizer MCP
pub mod query_executor;

/// Vectorizer-driven graph extraction
pub mod vectorizer_extractor;

/// Core graph types: enums, node/edge structs, CorrelationGraph, statistics
pub mod graph_types;

/// Graph builder trait and concrete implementations
pub mod graph_builder;

/// Collection query types: traits, query structs, QueryResult, QueryBuilder
pub mod collection_query;

#[cfg(test)]
mod tests;

// ---------------------------------------------------------------------------
// Re-exports from new submodules (all previously-reachable paths preserved)
// ---------------------------------------------------------------------------

pub use graph_types::{
    CorrelationGraph, EdgeType, GraphEdge, GraphNode, GraphStatistics, GraphType, NodeType,
    RecursionType, RecursiveCallConfig, RecursiveCallInfo, RecursiveCallStatistics,
};

pub use graph_builder::{
    CallGraphBuilder, ComponentGraphBuilder, DataFlowGraphBuilder, DefaultGraphBuilder,
    DependencyGraphBuilder, GraphBuilder, GraphBuilderCapabilities, GraphBuilderConfig,
    GraphCorrelationManager, GraphSourceData,
};

pub use collection_query::{
    CollectionQuery, CollectionQueryEnum, HybridQuery, MetadataQuery, QueryBuilder, QueryResult,
    QueryType, SemanticQuery, SortOrder,
};

// ---------------------------------------------------------------------------
// Re-exports from existing submodules (unchanged)
// ---------------------------------------------------------------------------

pub use component::{
    ClassInfo, ComponentAnalyzer, ComponentCouplingAnalyzer, ComponentCouplingMetrics,
    ComponentRelationship, ComponentRelationshipInfo, ComponentStatistics,
    ComponentVisualizationConfig, FieldInfo, InterfaceInfo, MethodInfo, OOHierarchyLayout,
    ParameterInfo, PropertyInfo, apply_component_visualization, apply_oop_hierarchy_layout,
};
pub use data_flow::{
    DataFlowAnalyzer, DataFlowEdge, DataFlowVisualizationConfig, DataTransformation,
    FlowBasedLayout, FlowType, TransformationType, TypePropagator, UsageType, VariableTracker,
    VariableUsage, VariableUsageSite, apply_data_flow_visualization, apply_flow_layout,
    visualize_data_flow,
};
pub use dependency_filter::{
    DependencyFilter, calculate_node_depths, filter_dependency_graph, get_direct_dependencies,
    get_transitive_dependencies, identify_leaf_and_root_nodes,
};
pub use graph_diff::{
    EdgeDiff, GraphDiff, NodeDiff, apply_diff, calculate_structural_similarity, compare_graphs,
};
pub use graph_export::{ExportFormat, export_graph};
pub use graph_statistics::calculate_statistics;
pub use impact_analysis::{
    ChangeImpactResult, ChangeType, ImpactAnalysis, ImpactSeverity, analyze_batch_impact,
    analyze_change_impact, analyze_impact, calculate_propagation_distance, identify_critical_nodes,
};
pub use pattern_recognition::{
    ArchitecturalPatternDetector, DesignPatternDetector, DetectedPattern,
    EventDrivenPatternDetector, PatternDetectionResult, PatternDetector, PatternDifficulty,
    PatternMaturity, PatternOverlayConfig, PatternQualityMetrics, PatternRecommendation,
    PatternRecommendationEngine, PatternStatistics, PatternType, PipelinePatternDetector,
    apply_pattern_overlays, calculate_pattern_quality_metrics,
};
pub use performance::{
    GraphCache, PerformanceMetrics, PerformanceProfiler, PerformanceSummary, calculate_complexity,
    optimize_graph,
};
pub use vectorizer_cache::{CacheKeyBuilder, CacheStatistics, VectorizerQueryCache};
pub use version_constraints::{
    ConflictSeverity, DependencyVersion, VersionCompatibility, VersionConflict, VersionConstraint,
    analyze_version_constraints,
};
pub use visualization::{
    CacheStats, EdgeInteractionData, EdgeLineStyle, EdgeStyle, GraphRenderer, InteractionData,
    LayoutAlgorithm, NodeInteractionData, NodeShape, NodeStyle, SvgRenderer, VisualizationCache,
    VisualizationConfig, apply_layout, create_svg_renderer, generate_interaction_data,
    render_graph_to_svg,
};

pub use query_executor::QueryExecutor;
pub use vectorizer_extractor::{
    VectorizerCollections, VectorizerExtractorConfig, VectorizerGraphExtractor,
};

// Re-export call graph filtering types for convenience
pub use call_graph_filtering::{
    CallGraphFilter, CallGraphFiltering, CallGraphPath, CallGraphSearch, CallGraphSearchResult,
    EdgeFilter, NodeFilter, PathSearch,
};
