use nexus_core::storage::RecordStore;
use tempfile::TempDir;

#[test]
fn test_storage_initial_node_count() {
    let temp_dir = TempDir::new().unwrap();
    let store = RecordStore::new(temp_dir.path()).unwrap();

    println!("Initial node_count: {}", store.node_count());

    assert_eq!(
        store.node_count(),
        0,
        "Fresh storage should start with node_count = 0"
    );
}
