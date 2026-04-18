//! Migration tools for converting between storage formats
//!
//! This module provides utilities for migrating data between the legacy
//! RecordStore (mmap-based) format and the new GraphStorageEngine format.

use super::engine::GraphStorageEngine;
use crate::error::{Error, Result};
use crate::storage::{
    NodeRecord as LegacyNodeRecord, RecordStore, RelationshipRecord as LegacyRelRecord,
};
use std::path::Path;
use std::time::Instant;
use tracing;

/// Migration statistics
#[derive(Debug, Clone, Default)]
pub struct MigrationStats {
    /// Number of nodes migrated
    pub nodes_migrated: u64,
    /// Number of relationships migrated
    pub relationships_migrated: u64,
    /// Number of nodes skipped (deleted)
    pub nodes_skipped: u64,
    /// Number of relationships skipped (deleted)
    pub relationships_skipped: u64,
    /// Migration duration in milliseconds
    pub duration_ms: u64,
    /// Nodes per second
    pub nodes_per_sec: f64,
    /// Relationships per second
    pub rels_per_sec: f64,
}

/// Migration options
#[derive(Debug, Clone)]
pub struct MigrationOptions {
    /// Whether to migrate deleted records
    pub include_deleted: bool,
    /// Batch size for relationship migration
    pub batch_size: usize,
    /// Whether to verify data after migration
    pub verify: bool,
    /// Whether to print progress
    pub verbose: bool,
}

impl Default for MigrationOptions {
    fn default() -> Self {
        Self {
            include_deleted: false,
            batch_size: 10000,
            verify: true,
            verbose: true,
        }
    }
}

/// Migrate data from RecordStore to GraphStorageEngine
///
/// # Arguments
/// * `record_store` - The source RecordStore (legacy format)
/// * `output_path` - Path for the new GraphStorageEngine file
/// * `options` - Migration options
///
/// # Returns
/// Migration statistics on success
pub fn migrate_to_graph_engine<P: AsRef<Path>>(
    record_store: &RecordStore,
    output_path: P,
    options: &MigrationOptions,
) -> Result<MigrationStats> {
    let start = Instant::now();
    let mut stats = MigrationStats::default();

    if options.verbose {
        tracing::info!("Starting migration to GraphStorageEngine...");
    }

    // Create new graph storage engine
    let mut engine = GraphStorageEngine::create(output_path.as_ref())?;

    // Migrate nodes
    let node_stats = migrate_nodes(record_store, &mut engine, options)?;
    stats.nodes_migrated = node_stats.0;
    stats.nodes_skipped = node_stats.1;

    if options.verbose {
        tracing::info!(
            "Migrated {} nodes ({} skipped)",
            stats.nodes_migrated,
            stats.nodes_skipped
        );
    }

    // Migrate relationships
    let rel_stats = migrate_relationships(record_store, &mut engine, options)?;
    stats.relationships_migrated = rel_stats.0;
    stats.relationships_skipped = rel_stats.1;

    if options.verbose {
        tracing::info!(
            "Migrated {} relationships ({} skipped)",
            stats.relationships_migrated,
            stats.relationships_skipped
        );
    }

    // Flush to ensure all data is persisted
    engine.flush()?;

    // Calculate timing stats
    stats.duration_ms = start.elapsed().as_millis() as u64;
    let duration_secs = stats.duration_ms as f64 / 1000.0;
    stats.nodes_per_sec = if duration_secs > 0.0 {
        stats.nodes_migrated as f64 / duration_secs
    } else {
        0.0
    };
    stats.rels_per_sec = if duration_secs > 0.0 {
        stats.relationships_migrated as f64 / duration_secs
    } else {
        0.0
    };

    // Verify if requested
    if options.verify {
        verify_migration(record_store, &engine, &stats)?;
        if options.verbose {
            tracing::info!("Migration verification passed");
        }
    }

    if options.verbose {
        tracing::info!(
            "Migration completed in {}ms ({:.0} nodes/sec, {:.0} rels/sec)",
            stats.duration_ms,
            stats.nodes_per_sec,
            stats.rels_per_sec
        );
    }

    Ok(stats)
}

/// Migrate nodes from RecordStore to GraphStorageEngine
fn migrate_nodes(
    record_store: &RecordStore,
    engine: &mut GraphStorageEngine,
    options: &MigrationOptions,
) -> Result<(u64, u64)> {
    let mut migrated = 0u64;
    let mut skipped = 0u64;

    // Get node count from record store
    let node_count = record_store.node_count();

    for node_id in 0..node_count {
        match record_store.read_node(node_id) {
            Ok(legacy_node) => {
                // Skip deleted nodes unless include_deleted is set
                if legacy_node.is_deleted() && !options.include_deleted {
                    skipped += 1;
                    continue;
                }

                // Get the primary label (first bit set in label_bits)
                let label_id = get_primary_label(&legacy_node);

                // Create node in new engine
                // Note: The new engine assigns IDs sequentially, so we need to
                // ensure the node_id matches
                let new_node_id = engine.create_node(label_id)?;

                // Verify ID match (important for relationship migration)
                if new_node_id != node_id {
                    return Err(Error::Storage(format!(
                        "Node ID mismatch during migration: expected {}, got {}",
                        node_id, new_node_id
                    )));
                }

                migrated += 1;
            }
            Err(_) => {
                // Node doesn't exist or error reading
                skipped += 1;
            }
        }
    }

    Ok((migrated, skipped))
}

/// Migrate relationships from RecordStore to GraphStorageEngine
fn migrate_relationships(
    record_store: &RecordStore,
    engine: &mut GraphStorageEngine,
    options: &MigrationOptions,
) -> Result<(u64, u64)> {
    let mut migrated = 0u64;
    let mut skipped = 0u64;

    // Get relationship count from record store
    let rel_count = record_store.relationship_count();

    for rel_id in 0..rel_count {
        match record_store.read_rel(rel_id) {
            Ok(legacy_rel) => {
                // Skip deleted relationships unless include_deleted is set
                if legacy_rel.is_deleted() && !options.include_deleted {
                    skipped += 1;
                    continue;
                }

                // Create relationship in new engine
                let _new_rel_id = engine.create_relationship(
                    legacy_rel.src_id,
                    legacy_rel.dst_id,
                    legacy_rel.type_id,
                )?;

                migrated += 1;

                // Progress logging for large migrations
                if options.verbose && migrated % 100000 == 0 {
                    tracing::info!("Migrated {} relationships...", migrated);
                }
            }
            Err(_) => {
                // Relationship doesn't exist or error reading
                skipped += 1;
            }
        }
    }

    Ok((migrated, skipped))
}

/// Verify migration by comparing counts and sampling data
fn verify_migration(
    record_store: &RecordStore,
    engine: &GraphStorageEngine,
    stats: &MigrationStats,
) -> Result<()> {
    let engine_stats = engine.stats();

    // Verify node count
    if engine_stats.node_count != stats.nodes_migrated {
        return Err(Error::Storage(format!(
            "Node count mismatch: migrated {} but engine has {}",
            stats.nodes_migrated, engine_stats.node_count
        )));
    }

    // Verify relationship count
    if engine_stats.relationship_count != stats.relationships_migrated {
        return Err(Error::Storage(format!(
            "Relationship count mismatch: migrated {} but engine has {}",
            stats.relationships_migrated, engine_stats.relationship_count
        )));
    }

    // Sample verification: check first and last nodes
    if stats.nodes_migrated > 0 {
        // Verify first node
        engine.read_node(0)?;

        // Verify last node
        if stats.nodes_migrated > 1 {
            engine.read_node(stats.nodes_migrated - 1)?;
        }
    }

    Ok(())
}

/// Get the primary label from a legacy node record
fn get_primary_label(node: &LegacyNodeRecord) -> u32 {
    // Return the first set bit (lowest label ID)
    for i in 0..64 {
        if node.has_label(i) {
            return i;
        }
    }
    0 // Default label if none set
}

/// Export data from GraphStorageEngine to RecordStore format
///
/// This provides a fallback path if migration causes issues.
pub fn export_to_record_store<P: AsRef<Path>>(
    engine: &GraphStorageEngine,
    output_path: P,
    options: &MigrationOptions,
) -> Result<MigrationStats> {
    let start = Instant::now();
    let mut stats = MigrationStats::default();

    if options.verbose {
        tracing::info!("Starting export to RecordStore format...");
    }

    // Create new record store (mutable for writes)
    let mut record_store = RecordStore::new(output_path.as_ref())?;

    let engine_stats = engine.stats();

    // Export nodes
    for node_id in 0..engine_stats.node_count {
        match engine.read_node(node_id) {
            Ok(node) => {
                let mut legacy_node = LegacyNodeRecord::new();
                legacy_node.add_label(node.label_id);
                record_store.write_node(node_id, &legacy_node)?;
                stats.nodes_migrated += 1;
            }
            Err(_) => {
                stats.nodes_skipped += 1;
            }
        }
    }

    if options.verbose {
        tracing::info!("Exported {} nodes", stats.nodes_migrated);
    }

    // Export relationships by type
    for type_id in engine.get_relationship_type_ids() {
        for rel_id in 0..engine_stats.relationship_count {
            if let Ok(rel) = engine.read_relationship(type_id, rel_id) {
                let legacy_rel = LegacyRelRecord::new(rel.from_node, rel.to_node, rel.type_id);
                record_store.write_rel(stats.relationships_migrated, &legacy_rel)?;
                stats.relationships_migrated += 1;
            }
        }
    }

    if options.verbose {
        tracing::info!("Exported {} relationships", stats.relationships_migrated);
    }

    // Calculate timing
    stats.duration_ms = start.elapsed().as_millis() as u64;
    let duration_secs = stats.duration_ms as f64 / 1000.0;
    stats.nodes_per_sec = if duration_secs > 0.0 {
        stats.nodes_migrated as f64 / duration_secs
    } else {
        0.0
    };
    stats.rels_per_sec = if duration_secs > 0.0 {
        stats.relationships_migrated as f64 / duration_secs
    } else {
        0.0
    };

    Ok(stats)
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn test_migration_empty_store() {
        let temp_dir = TempDir::new().unwrap();
        let record_store = RecordStore::new(temp_dir.path().join("source")).unwrap();
        let output_path = temp_dir.path().join("output.graph");

        let options = MigrationOptions {
            verbose: false,
            verify: true,
            ..Default::default()
        };

        let stats = migrate_to_graph_engine(&record_store, &output_path, &options).unwrap();

        assert_eq!(stats.nodes_migrated, 0);
        assert_eq!(stats.relationships_migrated, 0);
    }

    #[test]
    fn test_migration_with_nodes() {
        let temp_dir = TempDir::new().unwrap();
        let mut record_store = RecordStore::new(temp_dir.path().join("source")).unwrap();

        // Create nodes using allocate_node_id to properly track count
        for i in 0..10 {
            let node_id = record_store.allocate_node_id();
            let mut node = LegacyNodeRecord::new();
            node.add_label((i % 3) as u32);
            record_store.write_node(node_id, &node).unwrap();
        }

        let output_path = temp_dir.path().join("output.graph");

        let options = MigrationOptions {
            verbose: false,
            verify: true,
            ..Default::default()
        };

        let stats = migrate_to_graph_engine(&record_store, &output_path, &options).unwrap();

        assert_eq!(stats.nodes_migrated, 10);
    }

    #[test]
    fn test_migration_with_relationships() {
        let temp_dir = TempDir::new().unwrap();
        let mut record_store = RecordStore::new(temp_dir.path().join("source")).unwrap();

        // Create nodes using allocate_node_id to properly track count
        for _ in 0..5 {
            let node_id = record_store.allocate_node_id();
            let mut node = LegacyNodeRecord::new();
            node.add_label(1);
            record_store.write_node(node_id, &node).unwrap();
        }

        // Create relationships using allocate_rel_id to properly track count
        for i in 0..4u64 {
            let rel_id = record_store.allocate_rel_id();
            let rel = LegacyRelRecord::new(i, i + 1, 10);
            record_store.write_rel(rel_id, &rel).unwrap();
        }

        let output_path = temp_dir.path().join("output.graph");

        let options = MigrationOptions {
            verbose: false,
            verify: true,
            ..Default::default()
        };

        let stats = migrate_to_graph_engine(&record_store, &output_path, &options).unwrap();

        assert_eq!(stats.nodes_migrated, 5);
        assert_eq!(stats.relationships_migrated, 4);
    }

    #[test]
    fn test_migration_options() {
        let options = MigrationOptions::default();
        assert!(!options.include_deleted);
        assert_eq!(options.batch_size, 10000);
        assert!(options.verify);
        assert!(options.verbose);
    }
}
