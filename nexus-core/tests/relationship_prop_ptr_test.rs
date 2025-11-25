//! Integration tests for relationship creation and prop_ptr preservation
//!
//! These tests verify that creating relationships does not corrupt the prop_ptr
//! of nodes, ensuring that node properties remain accessible after relationship creation.

use nexus_core::error::Result;
use nexus_core::storage::RecordStore;
use nexus_core::transaction::TransactionManager;
use tempfile::TempDir;

fn create_test_store() -> (RecordStore, TempDir, std::path::PathBuf) {
    let dir = TempDir::new().unwrap();
    let path = dir.path().to_path_buf();
    let store = RecordStore::new(&path).unwrap();
    (store, dir, path)
}

#[test]
fn test_create_relationship_preserves_source_node_prop_ptr() -> Result<()> {
    let (mut store, _dir, _path) = create_test_store();
    let mut tx_mgr = TransactionManager::new()?;
    let mut tx = tx_mgr.begin_write()?;

    // Create source node with properties
    let source_id = store.create_node_with_label_bits(
        &mut tx,
        1, // label_bits for label 0
        serde_json::json!({"name": "Alice", "age": 30}),
    )?;

    // Create target node with properties
    let target_id = store.create_node_with_label_bits(
        &mut tx,
        2, // label_bits for label 1
        serde_json::json!({"name": "Acme"}),
    )?;

    // Get original prop_ptr from source node
    let source_node_before = store.read_node(source_id)?;
    let original_prop_ptr = source_node_before.prop_ptr;
    assert_ne!(original_prop_ptr, 0, "Source node should have properties");

    // Create relationship
    let _rel_id = store.create_relationship(
        &mut tx,
        source_id,
        target_id,
        0, // type_id
        serde_json::json!({"since": 2020}),
    )?;

    tx_mgr.commit(&mut tx)?;

    // Verify source node prop_ptr was preserved
    let source_node_after = store.read_node(source_id)?;
    assert_eq!(
        source_node_after.prop_ptr, original_prop_ptr,
        "Source node prop_ptr should be preserved after creating relationship"
    );

    // Verify source node properties are still accessible
    let source_properties = store.load_node_properties(source_id)?;
    assert!(
        source_properties.is_some(),
        "Source node properties should be accessible"
    );
    let props = source_properties.unwrap();
    assert_eq!(props["name"], "Alice");
    assert_eq!(props["age"], 30);

    Ok(())
}

#[test]
fn test_create_relationship_preserves_target_node_prop_ptr() -> Result<()> {
    let (mut store, _dir, _path) = create_test_store();
    let mut tx_mgr = TransactionManager::new()?;
    let mut tx = tx_mgr.begin_write()?;

    // Create source node
    let source_id =
        store.create_node_with_label_bits(&mut tx, 1, serde_json::json!({"name": "Bob"}))?;

    // Create target node with properties
    let target_id = store.create_node_with_label_bits(
        &mut tx,
        2,
        serde_json::json!({"name": "TechCorp", "founded": 2010}),
    )?;

    // Get original prop_ptr from target node
    let target_node_before = store.read_node(target_id)?;
    let original_prop_ptr = target_node_before.prop_ptr;
    assert_ne!(original_prop_ptr, 0, "Target node should have properties");

    // Create relationship
    let _rel_id = store.create_relationship(
        &mut tx,
        source_id,
        target_id,
        0,
        serde_json::json!({"since": 2022}),
    )?;

    tx_mgr.commit(&mut tx)?;

    // Verify target node prop_ptr was preserved
    let target_node_after = store.read_node(target_id)?;
    assert_eq!(
        target_node_after.prop_ptr, original_prop_ptr,
        "Target node prop_ptr should be preserved after creating relationship"
    );

    // Verify target node properties are still accessible
    let target_properties = store.load_node_properties(target_id)?;
    assert!(
        target_properties.is_some(),
        "Target node properties should be accessible"
    );
    let props = target_properties.unwrap();
    assert_eq!(props["name"], "TechCorp");
    assert_eq!(props["founded"], 2010);

    Ok(())
}

#[test]
fn test_create_multiple_relationships_preserves_prop_ptr() -> Result<()> {
    let (mut store, _dir, _path) = create_test_store();
    let mut tx_mgr = TransactionManager::new()?;

    // Create source node with properties
    let mut tx = tx_mgr.begin_write()?;
    let source_id = store.create_node_with_label_bits(
        &mut tx,
        1,
        serde_json::json!({"name": "Alice", "age": 30}),
    )?;

    // Create target nodes
    let target1_id =
        store.create_node_with_label_bits(&mut tx, 2, serde_json::json!({"name": "Acme"}))?;

    let target2_id =
        store.create_node_with_label_bits(&mut tx, 2, serde_json::json!({"name": "TechCorp"}))?;

    tx_mgr.commit(&mut tx)?;

    // Get original prop_ptr
    let source_node_before = store.read_node(source_id)?;
    let original_prop_ptr = source_node_before.prop_ptr;

    // Create first relationship
    let mut tx = tx_mgr.begin_write()?;
    let _rel1_id = store.create_relationship(
        &mut tx,
        source_id,
        target1_id,
        0,
        serde_json::json!({"since": 2020}),
    )?;
    tx_mgr.commit(&mut tx)?;

    // Verify prop_ptr after first relationship
    let source_node_after_rel1 = store.read_node(source_id)?;
    assert_eq!(
        source_node_after_rel1.prop_ptr, original_prop_ptr,
        "prop_ptr should be preserved after first relationship"
    );

    // Create second relationship
    let mut tx = tx_mgr.begin_write()?;
    let _rel2_id = store.create_relationship(
        &mut tx,
        source_id,
        target2_id,
        0,
        serde_json::json!({"since": 2022}),
    )?;
    tx_mgr.commit(&mut tx)?;

    // Verify prop_ptr after second relationship
    let source_node_after_rel2 = store.read_node(source_id)?;
    assert_eq!(
        source_node_after_rel2.prop_ptr, original_prop_ptr,
        "prop_ptr should be preserved after second relationship"
    );

    // Verify properties are still accessible
    let properties = store.load_node_properties(source_id)?;
    assert!(properties.is_some());
    let props = properties.unwrap();
    assert_eq!(props["name"], "Alice");
    assert_eq!(props["age"], 30);

    Ok(())
}

#[test]
fn test_prop_ptr_validation_on_write() -> Result<()> {
    let (mut store, _dir, _path) = create_test_store();
    let mut tx_mgr = TransactionManager::new()?;
    let mut tx = tx_mgr.begin_write()?;

    // Create a node
    let node_id =
        store.create_node_with_label_bits(&mut tx, 1, serde_json::json!({"name": "Test"}))?;

    tx_mgr.commit(&mut tx)?;

    // Get the correct prop_ptr
    let node = store.read_node(node_id)?;
    let correct_prop_ptr = node.prop_ptr;

    // Try to create a relationship to get a relationship prop_ptr
    let target_id =
        store.create_node_with_label_bits(&mut tx_mgr.begin_write()?, 2, serde_json::json!({}))?;

    let mut tx = tx_mgr.begin_write()?;
    let rel_id = store.create_relationship(
        &mut tx,
        node_id,
        target_id,
        0,
        serde_json::json!({"since": 2020}),
    )?;
    tx_mgr.commit(&mut tx)?;

    // Get relationship prop_ptr
    let rel = store.read_rel(rel_id)?;
    let rel_prop_ptr = rel.prop_ptr;

    // Try to write node with relationship prop_ptr (should fail validation)
    let mut corrupted_node = store.read_node(node_id)?;
    corrupted_node.prop_ptr = rel_prop_ptr;

    // This should fail the validation in write_node
    let result = store.write_node(node_id, &corrupted_node);
    assert!(
        result.is_err(),
        "write_node should reject corrupted prop_ptr"
    );

    // Verify original prop_ptr is still valid
    let node_after = store.read_node(node_id)?;
    assert_eq!(node_after.prop_ptr, correct_prop_ptr);

    Ok(())
}
