//! Engine construction knobs and top-level graph metrics.
//!
//! [`EngineConfig`] holds the runtime-tunable parameters the engine
//! accepts at construction time; call `Engine::with_data_dir_and_config`
//! to supply a non-default value. [`GraphStatistics`] is the summary
//! produced by `Engine::get_graph_statistics` — a cross-cutting read
//! of catalog + storage state that does not belong in either subsystem.

use std::collections::HashMap;

/// Graph statistics for analysis and monitoring
#[derive(Debug, Clone, Default)]
pub struct GraphStatistics {
    /// Total number of nodes
    pub node_count: u64,
    /// Total number of relationships
    pub relationship_count: u64,
    /// Count of nodes per label
    pub label_counts: HashMap<String, u64>,
    /// Count of relationships per type
    pub relationship_type_counts: HashMap<String, u64>,
}

/// Tunable construction parameters for [`crate::Engine`].
///
/// Holds the runtime-configurable knobs that used to be hardcoded
/// inside `Engine::with_data_dir`. Call sites that need to honour a
/// loaded YAML config should populate this explicitly via
/// `Engine::with_data_dir_and_config`; `Engine::with_data_dir` stays
/// as a thin wrapper that picks up [`EngineConfig::default`].
#[derive(Debug, Clone)]
pub struct EngineConfig {
    /// Page cache capacity in 8 KB pages. Historical default was 1024
    /// (8 MB), which is tiny for any real workload but safe on cold
    /// start.
    pub page_cache_capacity: usize,
}

impl Default for EngineConfig {
    fn default() -> Self {
        Self {
            page_cache_capacity: 1024,
        }
    }
}
