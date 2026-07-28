use super::*;
use crate::storage::records::{NODE_RECORD_SIZE, NodeRecord};
use crate::testing::TestContext;
use serde_json::json;
use std::io::{Seek, SeekFrom, Write};
use std::sync::{Arc, RwLock};

#[test]
fn test_property_store_creation() {
    let ctx = TestContext::new();
    let store = PropertyStore::new(ctx.path().to_path_buf()).unwrap();
    assert_eq!(store.property_count(), 0);
}

/// #3: a `prop_ptr` landing in the last 12 bytes before EOF must make the
/// header readers return `None`/`Ok(None)` — not over-read past
/// `mmap.len()` and panic. The old guard only rejected `offset >= len`,
/// so any offset in `[len - 12, len)` passed and then `read_u64` walked
/// off the end of the mapping. This is reachable from `read_node` /
/// `repair_corrupt_node_prop_ptrs` on a corrupt on-disk pointer.
#[test]
fn header_read_near_eof_returns_none_not_panic() {
    let ctx = TestContext::new();
    let store = PropertyStore::new(ctx.path().to_path_buf()).unwrap();
    let len = store.mmap.len() as u64;
    assert!(len >= PROPERTY_ENTRY_HEADER_SIZE, "fixture too small");

    // Every offset whose 13-byte header would run past EOF must be
    // rejected cleanly, including the exact last byte (`len - 1`).
    for offset in (len - (PROPERTY_ENTRY_HEADER_SIZE - 1))..len {
        assert_eq!(
            store.get_entity_info_at_offset(offset),
            None,
            "get_entity_info_at_offset({offset}) (len {len}) must return None"
        );
        assert!(
            matches!(store.load_properties_at_offset(offset), Ok(None)),
            "load_properties_at_offset({offset}) (len {len}) must return Ok(None)"
        );
    }
}

#[test]
fn test_store_and_load_properties() {
    let ctx = TestContext::new();
    let mut store = PropertyStore::new(ctx.path().to_path_buf()).unwrap();

    let properties = json!({
        "name": "Alice",
        "age": 30,
        "active": true
    });

    let ptr = store
        .store_properties(1, EntityType::Node, properties.clone())
        .unwrap();
    // First property should be at offset 1 (not 0, because prop_ptr=0 means "no properties")
    assert!(
        ptr == 1,
        "First property should be at offset 1, got {}",
        ptr
    );

    let loaded = store.load_properties(1, EntityType::Node).unwrap().unwrap();
    assert_eq!(loaded, properties);
}

#[test]
fn test_update_properties() {
    let ctx = TestContext::new();
    let mut store = PropertyStore::new(ctx.path().to_path_buf()).unwrap();

    let initial_properties = json!({"name": "Alice"});
    let updated_properties = json!({"name": "Alice", "age": 30});

    store
        .store_properties(1, EntityType::Node, initial_properties)
        .unwrap();
    store
        .store_properties(1, EntityType::Node, updated_properties.clone())
        .unwrap();

    let loaded = store.load_properties(1, EntityType::Node).unwrap().unwrap();
    assert_eq!(loaded, updated_properties);
}

#[test]
fn test_delete_properties() {
    let ctx = TestContext::new();
    let mut store = PropertyStore::new(ctx.path().to_path_buf()).unwrap();

    let properties = json!({"name": "Alice"});
    store
        .store_properties(1, EntityType::Node, properties)
        .unwrap();

    assert!(
        store
            .load_properties(1, EntityType::Node)
            .unwrap()
            .is_some()
    );

    store.delete_properties(1, EntityType::Node).unwrap();
    assert!(
        store
            .load_properties(1, EntityType::Node)
            .unwrap()
            .is_none()
    );
}

/// phase0_fix-deleted-properties-resurrected-on-rebuild §1.1: a deleted
/// property must stay deleted after the store is dropped and reopened
/// (reopen drives `PropertyStore::new` -> `rebuild_index`, the same
/// path a server restart takes). Before the fix, `delete_properties`
/// only cleared the in-memory index, so the rebuild scan re-parsed the
/// still-intact on-disk bytes and resurrected the property.
#[test]
fn test_deleted_properties_do_not_resurrect_on_reopen() {
    let ctx = TestContext::new();
    let dir = ctx.path().to_path_buf();

    {
        let mut store = PropertyStore::new(dir.clone()).unwrap();
        store
            .store_properties(1, EntityType::Node, json!({"secret": "x"}))
            .unwrap();
        store.delete_properties(1, EntityType::Node).unwrap();
        store.flush().unwrap();
    }

    let reopened = PropertyStore::new(dir).unwrap();
    assert!(
        reopened
            .load_properties(1, EntityType::Node)
            .unwrap()
            .is_none(),
        "deleted property resurrected after reopen"
    );
}

/// phase0_fix-deleted-properties-resurrected-on-rebuild §4.2: a
/// deleted entity must stay deleted even when live neighbours are
/// interleaved with it on disk, and the scanner must still stride
/// correctly past the dead entry to find them.
#[test]
fn test_deleted_properties_do_not_resurrect_among_live_neighbours() {
    let ctx = TestContext::new();
    let dir = ctx.path().to_path_buf();

    {
        let mut store = PropertyStore::new(dir.clone()).unwrap();
        store
            .store_properties(1, EntityType::Node, json!({"secret": "x"}))
            .unwrap();
        store
            .store_properties(2, EntityType::Node, json!({"name": "Bob"}))
            .unwrap();
        store.delete_properties(1, EntityType::Node).unwrap();
        store.flush().unwrap();
    }

    let reopened = PropertyStore::new(dir).unwrap();
    assert!(
        reopened
            .load_properties(1, EntityType::Node)
            .unwrap()
            .is_none(),
        "deleted property resurrected after reopen"
    );
    assert_eq!(
        reopened
            .load_properties(2, EntityType::Node)
            .unwrap()
            .unwrap(),
        json!({"name": "Bob"}),
        "live neighbour was lost or corrupted by the tombstone scan"
    );
}

#[test]
fn test_relationship_properties() {
    let ctx = TestContext::new();
    let mut store = PropertyStore::new(ctx.path().to_path_buf()).unwrap();

    let properties = json!({"weight": 0.8, "type": "friends"});
    store
        .store_properties(1, EntityType::Relationship, properties.clone())
        .unwrap();

    let loaded = store
        .load_properties(1, EntityType::Relationship)
        .unwrap()
        .unwrap();
    assert_eq!(loaded, properties);
}

#[test]
fn test_large_property_data() {
    let ctx = TestContext::new();
    let mut store = PropertyStore::new(ctx.path().to_path_buf()).unwrap();

    // Create a large JSON object
    let mut large_data = serde_json::Map::new();
    for i in 0..1000 {
        large_data.insert(
            format!("key_{}", i),
            serde_json::Value::String(format!("value_{}", i)),
        );
    }
    let properties = serde_json::Value::Object(large_data);

    let _ptr = store
        .store_properties(1, EntityType::Node, properties.clone())
        .unwrap();

    let loaded = store.load_properties(1, EntityType::Node).unwrap().unwrap();
    assert_eq!(loaded, properties);
}

#[test]
fn test_concurrent_property_access() {
    let ctx = TestContext::new();
    let store = Arc::new(RwLock::new(
        PropertyStore::new(ctx.path().to_path_buf()).unwrap(),
    ));

    let handles: Vec<_> = (0..10)
        .map(|i| {
            let store = Arc::clone(&store);
            std::thread::spawn(move || {
                let properties = json!({"thread_id": i, "data": format!("thread_{}", i)});
                store
                    .write()
                    .unwrap()
                    .store_properties(i as u64, EntityType::Node, properties)
                    .unwrap();
            })
        })
        .collect();

    for handle in handles {
        handle.join().unwrap();
    }

    // Verify all properties were stored
    for i in 0..10 {
        let loaded = store
            .read()
            .unwrap()
            .load_properties(i as u64, EntityType::Node)
            .unwrap()
            .unwrap();
        assert_eq!(loaded["thread_id"], i);
    }
}

#[test]
fn test_property_store_capacity_expansion() {
    let ctx = TestContext::new();
    let mut store = PropertyStore::new(ctx.path().to_path_buf()).unwrap();

    // Store many properties to trigger capacity expansion
    for i in 0..100 {
        let properties = json!({
            "id": i,
            "data": format!("property_data_{}", i),
            "metadata": {
                "created_at": "2024-01-01T00:00:00Z",
                "updated_at": "2024-01-01T00:00:00Z"
            }
        });
        store
            .store_properties(i, EntityType::Node, properties)
            .unwrap();
    }

    // Verify all properties can be loaded
    for i in 0..100 {
        let loaded = store.load_properties(i, EntityType::Node).unwrap().unwrap();
        assert_eq!(loaded["id"], i);
    }
}

#[test]
fn test_property_store_health_check() {
    let ctx = TestContext::new();
    let store = PropertyStore::new(ctx.path().to_path_buf()).unwrap();

    // Health check should pass for valid store
    store.health_check().unwrap();

    // Test property count
    assert_eq!(store.property_count(), 0);
}

#[test]
fn test_property_store_error_handling() {
    let ctx = TestContext::new();
    let mut store = PropertyStore::new(ctx.path().to_path_buf()).unwrap();

    // Test loading non-existent property
    let result = store.load_properties(999, EntityType::Node).unwrap();
    assert!(result.is_none());

    // Test deleting non-existent property (should not error)
    store.delete_properties(999, EntityType::Node).unwrap();
}

#[test]
fn test_property_store_serialization_types() {
    let ctx = TestContext::new();
    let mut store = PropertyStore::new(ctx.path().to_path_buf()).unwrap();

    // Test different JSON value types
    let test_cases = vec![
        ("string", json!("hello world")),
        ("number", json!(42)),
        ("float", json!(std::f64::consts::PI)),
        ("boolean", json!(true)),
        ("null", json!(null)),
        ("array", json!([1, 2, 3, "four"])),
        ("object", json!({"nested": {"key": "value"}})),
    ];

    for (name, value) in test_cases {
        store
            .store_properties(1, EntityType::Node, value.clone())
            .unwrap();

        let loaded = store.load_properties(1, EntityType::Node).unwrap().unwrap();
        assert_eq!(loaded, value, "Failed for test case: {}", name);
    }
}

#[test]
fn test_property_store_mixed_entity_types() {
    let ctx = TestContext::new();
    let mut store = PropertyStore::new(ctx.path().to_path_buf()).unwrap();

    // Store properties for both node and relationship with same ID
    let node_props = json!({"type": "user", "name": "Alice"});
    let rel_props = json!({"weight": 0.8, "type": "friends"});

    store
        .store_properties(1, EntityType::Node, node_props.clone())
        .unwrap();
    store
        .store_properties(1, EntityType::Relationship, rel_props.clone())
        .unwrap();

    // Verify both can be loaded independently
    let loaded_node = store.load_properties(1, EntityType::Node).unwrap().unwrap();
    let loaded_rel = store
        .load_properties(1, EntityType::Relationship)
        .unwrap()
        .unwrap();

    assert_eq!(loaded_node, node_props);
    assert_eq!(loaded_rel, rel_props);
}

/// Edge case: re-adding properties for an entity after they were
/// deleted must not resurrect the OLD (tombstoned) value, and must
/// survive a reopen with the NEW value.
#[test]
fn test_store_after_delete_reuses_entity_with_new_value() {
    let ctx = TestContext::new();
    let dir = ctx.path().to_path_buf();

    {
        let mut store = PropertyStore::new(dir.clone()).unwrap();
        store
            .store_properties(1, EntityType::Node, json!({"name": "Alice"}))
            .unwrap();
        store.delete_properties(1, EntityType::Node).unwrap();
        store
            .store_properties(1, EntityType::Node, json!({"name": "Bob"}))
            .unwrap();
        store.flush().unwrap();
    }

    let reopened = PropertyStore::new(dir).unwrap();
    assert_eq!(
        reopened
            .load_properties(1, EntityType::Node)
            .unwrap()
            .unwrap(),
        json!({"name": "Bob"}),
        "re-added property was lost, or the stale deleted value resurrected"
    );
}

/// Edge case: deleting properties for an entity that was never stored
/// must not error and must not tombstone unrelated bytes.
#[test]
fn test_delete_properties_for_nonexistent_entity_is_a_noop() {
    let ctx = TestContext::new();
    let dir = ctx.path().to_path_buf();

    let mut store = PropertyStore::new(dir).unwrap();
    store
        .store_properties(1, EntityType::Node, json!({"name": "Alice"}))
        .unwrap();

    // Deleting an entity that was never stored must succeed silently.
    store.delete_properties(999, EntityType::Node).unwrap();

    // The unrelated, still-live entity must be unaffected.
    assert_eq!(
        store.load_properties(1, EntityType::Node).unwrap().unwrap(),
        json!({"name": "Alice"})
    );
}

/// phase0_fix-deleted-properties-resurrected-on-rebuild §2.2 back-compat:
/// a pre-fix store may hold a deleted entity that was never tombstoned
/// (only its owning node record was marked deleted). Reopening must
/// reconcile against the record store and not resurrect it, while a
/// live neighbour with no record store entry (no reconciliation data)
/// is trusted as before.
#[test]
fn test_back_compat_reconciles_untombstoned_deleted_entity_against_record_store() {
    let ctx = TestContext::new();
    let dir = ctx.path().to_path_buf();

    {
        let mut store = PropertyStore::new(dir.clone()).unwrap();
        store
            .store_properties(1, EntityType::Node, json!({"secret": "x"}))
            .unwrap();
        store
            .store_properties(2, EntityType::Node, json!({"name": "Bob"}))
            .unwrap();
        // Simulate the pre-fix delete path: entity 1's property blob is
        // left fully parseable on disk (no tombstone).
        store.flush().unwrap();
    }

    // Simulate node 1 having been deleted at the record-store level
    // (the authoritative signal a pre-fix property store had no way to
    // record itself). Node 2's slot is present but not deleted, to
    // confirm reconciliation only drops the deleted entity.
    let nodes_path = dir.join("nodes.store");
    let mut deleted_record = NodeRecord::new();
    deleted_record.mark_deleted();
    let mut file = OpenOptions::new()
        .create(true)
        .write(true)
        .truncate(true)
        .open(&nodes_path)
        .unwrap();
    file.set_len(3 * NODE_RECORD_SIZE as u64).unwrap();
    file.seek(SeekFrom::Start(NODE_RECORD_SIZE as u64)).unwrap();
    file.write_all(bytemuck::bytes_of(&deleted_record)).unwrap();
    file.sync_all().unwrap();
    drop(file);

    let reopened = PropertyStore::new(dir).unwrap();
    assert!(
        reopened
            .load_properties(1, EntityType::Node)
            .unwrap()
            .is_none(),
        "pre-fix, un-tombstoned deleted entity resurrected after reopen"
    );
    assert_eq!(
        reopened
            .load_properties(2, EntityType::Node)
            .unwrap()
            .unwrap(),
        json!({"name": "Bob"}),
        "live neighbour was wrongly reconciled away as deleted"
    );
}
