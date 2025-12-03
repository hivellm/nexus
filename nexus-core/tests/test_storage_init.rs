use nexus_core::storage::RecordStore;
use nexus_core::testing::TestContext;

#[test]
fn test_storage_initial_node_count() {
    let ctx = TestContext::new();
    let store = RecordStore::new(ctx.path()).unwrap();

    tracing::info!("Initial node_count: {}", store.node_count());

    assert_eq!(
        store.node_count(),
        0,
        "Fresh storage should start with node_count = 0"
    );
}
