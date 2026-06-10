//! Catalog module — label/type/key mappings.
//!
//! The catalog maintains bidirectional mappings between:
//! - Labels (node labels) ↔ LabelId
//! - Types (relationship types) ↔ TypeId
//! - Keys (property keys) ↔ KeyId
//!
//! # Architecture
//!
//! Backed by LMDB via [`heed`] with per-mapping databases plus sidecar
//! databases for metadata, statistics, constraints, UDFs, and
//! procedures. Tests share a single `nexus_test_catalogs_shared`
//! environment under `std::env::temp_dir()` so parallel runs stay below
//! the Windows TLS-slot ceiling that used to surface as `TlsFull`. In
//! production every `Catalog::new` call opens its own LMDB environment
//! at the caller-supplied path.
//!
//! A second, memory-map-backed backend (`mmap_catalog.rs`) lived next
//! to this module during the 2025 TlsFull investigation. It was never
//! wired into `Catalog::new` at runtime — the `NEXUS_USE_MMAP_CATALOG`
//! env var was read but both branches fell through to LMDB — so the
//! phase2 "deduplicate catalog backends" task deleted it. If a future
//! memory-mapped backend is required, implement it behind a fresh
//! trait rather than resurrecting the dead module.
//!
//! # Sub-module layout
//!
//! | Module | Contents |
//! |---|---|
//! | [`types`] | Primitive type aliases and value types (`LabelId`, `CatalogStats`, …) |
//! | [`store`] | `Catalog` struct, LMDB constructors, `Default` impl |
//! | [`mappings`] | Label/type/key name ↔ ID allocation and lookup |
//! | [`stats`] | Metadata and statistics read/write |
//! | [`extensions`] | UDF, procedure, property-index, external-id index |
//! | [`constraints`] | Uniqueness / existence constraint management |
//! | [`external_id`] | `ExternalId` value type |
//! | [`external_id_index`] | Forward+reverse LMDB external-id index |

// ── Existing sibling modules (untouched) ────────────────────────────────────
pub mod constraints;
pub mod external_id;
pub mod external_id_index;

// ── New split sub-modules ────────────────────────────────────────────────────
pub(crate) mod extensions;
pub(crate) mod mappings;
pub(crate) mod stats;
pub(crate) mod store;
pub(crate) mod types;

// ── Public re-exports — every path that was previously reachable via
//    `crate::catalog::*` is preserved here unchanged.
// ── types ────────────────────────────────────────────────────────────────────
pub use types::{CatalogMetadata, CatalogStats, KeyId, LabelId, TypeId};

// ── store ────────────────────────────────────────────────────────────────────
pub use store::{CATALOG_MMAP_INITIAL_SIZE, Catalog};

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testing::TestContext;

    fn create_test_catalog() -> (Catalog, TestContext) {
        let ctx = TestContext::new();
        // Use shared catalog for most tests to avoid TlsFull.
        let catalog = Catalog::with_map_size(ctx.path(), CATALOG_MMAP_INITIAL_SIZE).unwrap();
        (catalog, ctx)
    }

    /// Create an isolated catalog for tests that need data isolation.
    /// WARNING: Use sparingly — each call creates a new LMDB environment.
    fn create_isolated_test_catalog() -> (Catalog, TestContext) {
        let ctx = TestContext::new();
        let catalog = Catalog::with_isolated_path(ctx.path(), CATALOG_MMAP_INITIAL_SIZE).unwrap();
        (catalog, ctx)
    }

    #[test]
    fn test_catalog_creation() {
        let (catalog, _dir) = create_isolated_test_catalog();
        let metadata = catalog.get_metadata().unwrap();
        assert_eq!(metadata.version, 1);
        assert_eq!(metadata.page_size, 8192);
    }

    #[test]
    fn test_label_creation() {
        let (catalog, _dir) = create_isolated_test_catalog();

        let person_id = catalog.get_or_create_label("Person").unwrap();
        let company_id = catalog.get_or_create_label("Company").unwrap();

        assert_ne!(person_id, company_id);

        // Get same label again should return same ID.
        let person_id_2 = catalog.get_or_create_label("Person").unwrap();
        assert_eq!(person_id, person_id_2);
    }

    #[test]
    fn test_label_name_lookup() {
        let (catalog, _dir) = create_isolated_test_catalog();

        let id = catalog.get_or_create_label("Person").unwrap();
        let name = catalog.get_label_name(id).unwrap();

        assert_eq!(name, Some("Person".to_string()));
    }

    #[test]
    fn test_type_creation() {
        let (catalog, _dir) = create_isolated_test_catalog();

        let knows_id = catalog.get_or_create_type("KNOWS").unwrap();
        let works_at_id = catalog.get_or_create_type("WORKS_AT").unwrap();

        assert_ne!(knows_id, works_at_id);

        let knows_id_2 = catalog.get_or_create_type("KNOWS").unwrap();
        assert_eq!(knows_id, knows_id_2);
    }

    #[test]
    fn test_key_creation() {
        let (catalog, _dir) = create_isolated_test_catalog();

        let name_id = catalog.get_or_create_key("name").unwrap();
        let age_id = catalog.get_or_create_key("age").unwrap();

        assert_ne!(name_id, age_id);

        let name_id_2 = catalog.get_or_create_key("name").unwrap();
        assert_eq!(name_id, name_id_2);
    }

    #[test]
    fn test_statistics_update() {
        // Use isolated catalog for statistics tests.
        let (catalog, _dir) = create_isolated_test_catalog();

        let person_id = catalog.get_or_create_label("TestStatPerson").unwrap();

        catalog.increment_node_count(person_id).unwrap();
        catalog.increment_node_count(person_id).unwrap();

        let stats = catalog.get_statistics().unwrap();
        assert_eq!(stats.node_counts.get(&person_id), Some(&2));

        catalog.decrement_node_count(person_id).unwrap();
        let stats = catalog.get_statistics().unwrap();
        assert_eq!(stats.node_counts.get(&person_id), Some(&1));
    }

    #[test]
    fn test_persistence() {
        let ctx = TestContext::new();
        let path = ctx.path().to_path_buf();

        // Create catalog and add data using isolated path.
        {
            let catalog = Catalog::with_isolated_path(&path, CATALOG_MMAP_INITIAL_SIZE).unwrap();
            catalog.get_or_create_label("Person").unwrap();
            catalog.get_or_create_type("KNOWS").unwrap();
            catalog.sync().unwrap();
        }

        // Reopen and verify data persisted.
        {
            let catalog = Catalog::with_isolated_path(&path, CATALOG_MMAP_INITIAL_SIZE).unwrap();
            let person_id = catalog.get_or_create_label("Person").unwrap();
            let knows_id = catalog.get_or_create_type("KNOWS").unwrap();

            assert_eq!(person_id, 0);
            assert_eq!(knows_id, 0);
        }
    }

    #[test]
    fn test_type_name_lookup() {
        let (catalog, _dir) = create_isolated_test_catalog();

        let id = catalog.get_or_create_type("KNOWS").unwrap();
        let name = catalog.get_type_name(id).unwrap();

        assert_eq!(name, Some("KNOWS".to_string()));
    }

    #[test]
    fn test_key_name_lookup() {
        let (catalog, _dir) = create_isolated_test_catalog();

        let id = catalog.get_or_create_key("name").unwrap();
        let name = catalog.get_key_name(id).unwrap();

        assert_eq!(name, Some("name".to_string()));
    }

    #[test]
    fn test_nonexistent_label_lookup() {
        let (catalog, _dir) = create_isolated_test_catalog();

        let name = catalog.get_label_name(999).unwrap();
        assert_eq!(name, None);
    }

    #[test]
    fn test_nonexistent_type_lookup() {
        let (catalog, _dir) = create_isolated_test_catalog();

        let name = catalog.get_type_name(999).unwrap();
        assert_eq!(name, None);
    }

    #[test]
    fn test_nonexistent_key_lookup() {
        let (catalog, _dir) = create_isolated_test_catalog();

        let name = catalog.get_key_name(999).unwrap();
        assert_eq!(name, None);
    }

    #[test]
    fn test_metadata_update() {
        let (catalog, _dir) = create_isolated_test_catalog();

        let mut metadata = catalog.get_metadata().unwrap();
        assert_eq!(metadata.epoch, 0);

        metadata.epoch = 100;
        catalog.update_metadata(&metadata).unwrap();

        let updated = catalog.get_metadata().unwrap();
        assert_eq!(updated.epoch, 100);
    }

    #[test]
    fn test_rel_count_tracking() {
        // Use isolated catalog for statistics tests.
        let (catalog, _dir) = create_isolated_test_catalog();

        let type_id = catalog.get_or_create_type("TestRelKnows").unwrap();

        catalog.increment_rel_count(type_id).unwrap();
        catalog.increment_rel_count(type_id).unwrap();
        catalog.increment_rel_count(type_id).unwrap();

        let stats = catalog.get_statistics().unwrap();
        assert_eq!(stats.rel_counts.get(&type_id), Some(&3));

        catalog.decrement_rel_count(type_id).unwrap();
        let stats = catalog.get_statistics().unwrap();
        assert_eq!(stats.rel_counts.get(&type_id), Some(&2));
    }

    #[test]
    fn test_decrement_nonexistent_count() {
        let (catalog, _dir) = create_isolated_test_catalog();

        // Decrementing non-existent count should not panic.
        catalog.decrement_node_count(999).unwrap();
        catalog.decrement_rel_count(999).unwrap();

        let stats = catalog.get_statistics().unwrap();
        assert_eq!(stats.node_counts.get(&999), None);
    }

    #[test]
    fn test_decrement_to_zero() {
        let (catalog, _dir) = create_isolated_test_catalog();

        let label_id = catalog.get_or_create_label("Person").unwrap();

        catalog.increment_node_count(label_id).unwrap();

        let stats = catalog.get_statistics().unwrap();
        assert_eq!(stats.node_counts.get(&label_id), Some(&1));

        catalog.decrement_node_count(label_id).unwrap();

        let stats = catalog.get_statistics().unwrap();
        assert_eq!(stats.node_counts.get(&label_id), Some(&0));
    }

    #[test]
    fn test_multiple_labels_and_types() {
        // Use isolated catalog to ensure clean state.
        let (catalog, _dir) = create_isolated_test_catalog();

        // Create multiple labels.
        let labels = vec!["Person", "Company", "Product", "Location"];
        for label in &labels {
            catalog.get_or_create_label(label).unwrap();
        }

        // Create multiple types.
        let types = vec!["KNOWS", "WORKS_AT", "BOUGHT", "LOCATED_IN"];
        for type_name in &types {
            catalog.get_or_create_type(type_name).unwrap();
        }

        // Verify all can be looked up.
        for label in &labels {
            let id = catalog.get_or_create_label(label).unwrap();
            let name = catalog.get_label_name(id).unwrap();
            assert_eq!(name.as_deref(), Some(*label));
        }

        for type_name in &types {
            let id = catalog.get_or_create_type(type_name).unwrap();
            let name = catalog.get_type_name(id).unwrap();
            assert_eq!(name.as_deref(), Some(*type_name));
        }
    }

    #[test]
    fn test_sync_operation() {
        let (catalog, _dir) = create_isolated_test_catalog();

        catalog.get_or_create_label("Person").unwrap();
        catalog.sync().unwrap();

        // Should not fail.
        catalog.sync().unwrap();
    }

    #[test]
    fn test_statistics_initialization() {
        // Use isolated catalog for statistics tests.
        let (catalog, _dir) = create_isolated_test_catalog();

        let stats = catalog.get_statistics().unwrap();
        assert_eq!(stats.label_count, 0);
        assert_eq!(stats.type_count, 0);
        assert_eq!(stats.key_count, 0);
        assert!(stats.node_counts.is_empty());
        assert!(stats.rel_counts.is_empty());
    }

    #[test]
    fn test_metadata_initialization() {
        let (catalog, _dir) = create_isolated_test_catalog();

        let metadata = catalog.get_metadata().unwrap();
        assert_eq!(metadata.version, 1);
        assert_eq!(metadata.epoch, 0);
        assert_eq!(metadata.page_size, 8192);
    }

    #[test]
    fn test_reopen_with_existing_data() {
        let ctx = TestContext::new();
        let path = ctx.path().to_path_buf();

        // Create catalog with data using isolated path.
        {
            let catalog = Catalog::with_isolated_path(&path, CATALOG_MMAP_INITIAL_SIZE).unwrap();
            catalog.get_or_create_label("Person").unwrap();
            catalog.get_or_create_label("Company").unwrap();
            catalog.get_or_create_type("KNOWS").unwrap();
            catalog.get_or_create_key("name").unwrap();

            let person_id = catalog.get_or_create_label("Person").unwrap();
            catalog.increment_node_count(person_id).unwrap();

            catalog.sync().unwrap();
        }

        // Reopen and verify counters are correct.
        {
            let catalog = Catalog::with_isolated_path(&path, CATALOG_MMAP_INITIAL_SIZE).unwrap();

            // Should allocate next IDs correctly.
            let location_id = catalog.get_or_create_label("Location").unwrap();
            assert_eq!(location_id, 2); // After Person(0) and Company(1)

            let works_at_id = catalog.get_or_create_type("WORKS_AT").unwrap();
            assert_eq!(works_at_id, 1); // After KNOWS(0)

            let age_id = catalog.get_or_create_key("age").unwrap();
            assert_eq!(age_id, 1); // After name(0)
        }
    }

    #[test]
    fn test_mixed_operations() {
        let (catalog, _dir) = create_isolated_test_catalog();

        // Mix labels, types, and keys.
        let p1 = catalog.get_or_create_label("Person").unwrap();
        let k1 = catalog.get_or_create_type("KNOWS").unwrap();
        let n1 = catalog.get_or_create_key("name").unwrap();
        let p2 = catalog.get_or_create_label("Company").unwrap();
        let k2 = catalog.get_or_create_type("WORKS_AT").unwrap();
        let n2 = catalog.get_or_create_key("age").unwrap();

        // Verify all unique.
        assert_ne!(p1, p2);
        assert_ne!(k1, k2);
        assert_ne!(n1, n2);

        // Verify lookups work.
        assert_eq!(
            catalog.get_label_name(p1).unwrap(),
            Some("Person".to_string())
        );
        assert_eq!(
            catalog.get_type_name(k1).unwrap(),
            Some("KNOWS".to_string())
        );
        assert_eq!(catalog.get_key_name(n1).unwrap(), Some("name".to_string()));
    }

    #[test]
    fn test_saturating_decrement() {
        // Use isolated catalog for statistics tests.
        let (catalog, _dir) = create_isolated_test_catalog();

        let label_id = catalog.get_or_create_label("TestSatPerson").unwrap();

        // Increment once.
        catalog.increment_node_count(label_id).unwrap();

        // Decrement twice (should saturate at 0, not underflow).
        catalog.decrement_node_count(label_id).unwrap();
        catalog.decrement_node_count(label_id).unwrap();

        let stats = catalog.get_statistics().unwrap();
        assert_eq!(stats.node_counts.get(&label_id), Some(&0));
    }

    #[test]
    fn test_multiple_increments() {
        let (catalog, _dir) = create_isolated_test_catalog();

        let label_id = catalog.get_or_create_label("Person").unwrap();
        let type_id = catalog.get_or_create_type("KNOWS").unwrap();

        // Multiple increments.
        for _ in 0..100 {
            catalog.increment_node_count(label_id).unwrap();
            catalog.increment_rel_count(type_id).unwrap();
        }

        let stats = catalog.get_statistics().unwrap();
        assert_eq!(stats.node_counts.get(&label_id), Some(&100));
        assert_eq!(stats.rel_counts.get(&type_id), Some(&100));
    }

    #[test]
    fn test_get_labels_from_bitmap() {
        let (catalog, _dir) = create_isolated_test_catalog();

        // Create some labels.
        let person_id = catalog.get_or_create_label("Person").unwrap();
        let company_id = catalog.get_or_create_label("Company").unwrap();

        // Create a bitmap with both labels.
        let bitmap = (1u64 << person_id) | (1u64 << company_id);

        // Test conversion.
        let labels = catalog.get_labels_from_bitmap(bitmap).unwrap();
        assert_eq!(labels.len(), 2);
        assert!(labels.contains(&"Person".to_string()));
        assert!(labels.contains(&"Company".to_string()));
    }

    #[test]
    fn test_get_labels_from_empty_bitmap() {
        let (catalog, _dir) = create_isolated_test_catalog();

        // Test with empty bitmap.
        let labels = catalog.get_labels_from_bitmap(0).unwrap();
        assert_eq!(labels.len(), 0);
    }

    #[test]
    fn test_get_label_id() {
        let (catalog, _dir) = create_isolated_test_catalog();

        // Create a label.
        let person_id = catalog.get_or_create_label("Person").unwrap();

        // Test getting the ID.
        let retrieved_id = catalog.get_label_id("Person").unwrap();
        assert_eq!(retrieved_id, person_id);
    }

    #[test]
    fn test_get_label_id_nonexistent() {
        let (catalog, _dir) = create_isolated_test_catalog();

        // Test getting ID for non-existent label.
        let result = catalog.get_label_id("Nonexistent");
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("not found"));
    }

    #[test]
    fn test_get_label_id_by_id() {
        let (catalog, _dir) = create_isolated_test_catalog();

        // Test the identity function.
        let test_id = 5;
        let result = catalog.get_label_id_by_id(test_id).unwrap();
        assert_eq!(result, test_id);
    }

    #[test]
    fn test_udf_storage() {
        let (catalog, _dir) = create_isolated_test_catalog();

        let signature = crate::udf::UdfSignature {
            name: "test_udf".to_string(),
            parameters: vec![],
            return_type: crate::udf::UdfReturnType::Integer,
            description: Some("Test UDF".to_string()),
        };

        // Store UDF.
        catalog.store_udf(&signature).unwrap();

        // Retrieve UDF.
        let retrieved = catalog.get_udf("test_udf").unwrap();
        assert!(retrieved.is_some());
        let retrieved_sig = retrieved.unwrap();
        assert_eq!(retrieved_sig.name, "test_udf");
        assert_eq!(
            retrieved_sig.return_type,
            crate::udf::UdfReturnType::Integer
        );

        // List UDFs.
        let udfs = catalog.list_udfs().unwrap();
        assert_eq!(udfs.len(), 1);
        assert_eq!(udfs[0], "test_udf");

        // Remove UDF.
        catalog.remove_udf("test_udf").unwrap();
        let retrieved_after = catalog.get_udf("test_udf").unwrap();
        assert!(retrieved_after.is_none());
    }

    #[test]
    fn test_procedure_storage() {
        let (catalog, _dir) = create_isolated_test_catalog();

        let signature = crate::graph::procedures::ProcedureSignature {
            name: "custom.test".to_string(),
            parameters: vec![crate::graph::procedures::ProcedureParameter {
                name: "param1".to_string(),
                param_type: crate::graph::procedures::ParameterType::Integer,
                required: true,
                default: None,
            }],
            output_columns: vec!["result".to_string()],
            description: Some("Test procedure".to_string()),
        };

        // Store procedure.
        catalog.store_procedure(&signature).unwrap();

        // Retrieve procedure.
        let retrieved = catalog.get_procedure("custom.test").unwrap();
        assert!(retrieved.is_some());
        let retrieved_sig = retrieved.unwrap();
        assert_eq!(retrieved_sig.name, "custom.test");
        assert_eq!(retrieved_sig.parameters.len(), 1);
        assert_eq!(retrieved_sig.output_columns.len(), 1);

        // List procedures.
        let procedures = catalog.list_procedures().unwrap();
        assert_eq!(procedures.len(), 1);
        assert_eq!(procedures[0], "custom.test");

        // Remove procedure.
        catalog.remove_procedure("custom.test").unwrap();
        let retrieved_after = catalog.get_procedure("custom.test").unwrap();
        assert!(retrieved_after.is_none());
    }

    // ── External-id index integration tests ──────────────────────────────────

    #[test]
    fn test_external_id_insert_and_lookup() {
        use crate::catalog::external_id::{ExternalId, HashKind};
        let (catalog, _dir) = create_isolated_test_catalog();

        let ext = ExternalId::try_hash(HashKind::Sha256, vec![0xABu8; 32]).unwrap();
        let internal_id = 42u64;

        // Insert.
        let mut wtxn = catalog.env.write_txn().unwrap();
        let conflict = catalog
            .external_id_index()
            .put_if_absent(&mut wtxn, &ext, internal_id)
            .unwrap();
        wtxn.commit().unwrap();

        assert_eq!(conflict, None, "first insert must not conflict");

        // Forward lookup.
        let rtxn = catalog.env.read_txn().unwrap();
        let found = catalog
            .external_id_index()
            .get_internal(&rtxn, &ext)
            .unwrap();
        assert_eq!(found, Some(internal_id));

        // Reverse lookup.
        let found_ext = catalog
            .external_id_index()
            .get_external(&rtxn, internal_id)
            .unwrap();
        assert_eq!(found_ext, Some(ext));
    }

    #[test]
    fn test_external_id_duplicate_rejection() {
        use crate::catalog::external_id::ExternalId;
        let (catalog, _dir) = create_isolated_test_catalog();

        let ext = ExternalId::try_uuid([0x55u8; 16]).unwrap();

        let mut wtxn = catalog.env.write_txn().unwrap();
        catalog
            .external_id_index()
            .put_if_absent(&mut wtxn, &ext, 10)
            .unwrap();
        wtxn.commit().unwrap();

        // Second insert of same external id — must return existing.
        let mut wtxn2 = catalog.env.write_txn().unwrap();
        let conflict = catalog
            .external_id_index()
            .put_if_absent(&mut wtxn2, &ext, 20)
            .unwrap();
        wtxn2.commit().unwrap();

        assert_eq!(conflict, Some(10));
    }

    #[test]
    fn test_external_id_delete_both_maps() {
        use crate::catalog::external_id::ExternalId;
        let (catalog, _dir) = create_isolated_test_catalog();

        let ext = ExternalId::try_str("del-me".to_string()).unwrap();
        let internal_id = 7u64;

        let mut wtxn = catalog.env.write_txn().unwrap();
        catalog
            .external_id_index()
            .put_if_absent(&mut wtxn, &ext, internal_id)
            .unwrap();
        wtxn.commit().unwrap();

        let mut wtxn2 = catalog.env.write_txn().unwrap();
        let deleted = catalog
            .external_id_index()
            .delete(&mut wtxn2, internal_id)
            .unwrap();
        wtxn2.commit().unwrap();

        assert!(deleted);

        let rtxn = catalog.env.read_txn().unwrap();
        assert_eq!(
            catalog
                .external_id_index()
                .get_internal(&rtxn, &ext)
                .unwrap(),
            None
        );
        assert_eq!(
            catalog
                .external_id_index()
                .get_external(&rtxn, internal_id)
                .unwrap(),
            None
        );
    }

    #[test]
    fn test_external_id_reopen_and_reload() {
        use crate::catalog::external_id::ExternalId;
        let ctx = TestContext::new();
        let path = ctx.path().to_path_buf();

        let ext = ExternalId::try_str("persist-test".to_string()).unwrap();
        let internal_id = 99u64;

        // Write data and close.
        {
            let catalog = Catalog::with_isolated_path(&path, CATALOG_MMAP_INITIAL_SIZE).unwrap();
            let mut wtxn = catalog.env.write_txn().unwrap();
            catalog
                .external_id_index()
                .put_if_absent(&mut wtxn, &ext, internal_id)
                .unwrap();
            wtxn.commit().unwrap();
            catalog.sync().unwrap();
        }

        // Reopen and verify.
        {
            let catalog = Catalog::with_isolated_path(&path, CATALOG_MMAP_INITIAL_SIZE).unwrap();
            let rtxn = catalog.env.read_txn().unwrap();
            assert_eq!(
                catalog
                    .external_id_index()
                    .get_internal(&rtxn, &ext)
                    .unwrap(),
                Some(internal_id)
            );
            assert_eq!(
                catalog
                    .external_id_index()
                    .get_external(&rtxn, internal_id)
                    .unwrap(),
                Some(ext)
            );
        }
    }

    #[test]
    fn test_external_id_forward_reverse_consistency() {
        use crate::catalog::external_id::ExternalId;
        let (catalog, _dir) = create_isolated_test_catalog();

        let mut wtxn = catalog.env.write_txn().unwrap();
        for i in 0u64..5 {
            let ext = ExternalId::try_str(format!("node-{i}")).unwrap();
            catalog
                .external_id_index()
                .put_if_absent(&mut wtxn, &ext, i * 10)
                .unwrap();
        }
        wtxn.commit().unwrap();

        // Integrity check must pass.
        catalog.verify_external_ids().unwrap();
    }
}
