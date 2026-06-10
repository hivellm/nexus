//! Data Flow Analysis Module
//!
//! Provides advanced data flow analysis capabilities:
//! - Variable usage tracking
//! - Data transformation analysis
//! - Data type propagation
//! - Flow-based layout algorithms
//! - Flow optimization suggestions
//! - Data flow statistics

use crate::graph::correlation::visualization::VisualizationConfig;

use crate::Result;
use crate::graph::correlation::{CorrelationGraph, EdgeType, GraphEdge, GraphNode, NodeType};
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};

mod analyzer;
pub mod layout;
mod optimization;
mod statistics;
mod tracker;
mod types;

pub use analyzer::DataFlowAnalyzer;
pub use layout::{
    DataFlowVisualizationConfig, FlowBasedLayout, apply_data_flow_visualization, apply_flow_layout,
    visualize_data_flow,
};
pub use optimization::{
    FlowOptimizationAnalyzer, FlowOptimizationSuggestion, OptimizationEffort, OptimizationImpact,
    OptimizationPriority,
};
pub use statistics::DataFlowStatistics;
pub use tracker::{UsageType, VariableTracker, VariableUsage, VariableUsageSite};
pub use types::{DataFlowEdge, DataTransformation, FlowType, TransformationType, TypePropagator};

#[cfg(test)]
mod tests;
