use nexus_core::storage::{RecordStore, RelationshipRecord};
use nexus_core::transaction::TransactionManager;
use tempfile::TempDir;
use tracing;

#[test]
fn test_relationship_linked_list_traversal() {
    // Setup
    let temp_dir = TempDir::new().unwrap();
    let mut store = RecordStore::new(temp_dir.path()).unwrap();
    let mut tx_mgr = TransactionManager::new().unwrap();

    // Create 3 nodes
    let mut tx = tx_mgr.begin_write().unwrap();
    let node1 = store
        .create_node(&mut tx, vec![], serde_json::json!({}))
        .unwrap();
    let node2 = store
        .create_node(&mut tx, vec![], serde_json::json!({}))
        .unwrap();
    let node3 = store
        .create_node(&mut tx, vec![], serde_json::json!({}))
        .unwrap();
    tx_mgr.commit(&mut tx).unwrap();
    store.flush().unwrap(); // Ensure persistence

    tracing::info!("Nodes created: {}, {}, {}", node1, node2, node3);

    // Create first relationship: node1 -> node2
    let mut tx1 = tx_mgr.begin_write().unwrap();
    let rel1 = store
        .create_relationship(&mut tx1, node1, node2, 1, serde_json::json!({}))
        .unwrap();
    tracing::info!("Rel 1 created: {} ({} -> {})", rel1, node1, node2);
    tx_mgr.commit(&mut tx1).unwrap();
    store.flush().unwrap();

    // Verify first relationship pointer on node1
    let n1_rec = store.read_node(node1).unwrap();
    let first_rel_ptr_1 = n1_rec.first_rel_ptr;
    tracing::info!("Node 1 first_rel_ptr after rel1: {}", first_rel_ptr_1);
    assert_eq!(first_rel_ptr_1, rel1 + 1);

    // Create second relationship: node1 -> node3
    let mut tx2 = tx_mgr.begin_write().unwrap();
    let rel2 = store
        .create_relationship(&mut tx2, node1, node3, 1, serde_json::json!({}))
        .unwrap();
    tracing::info!("Rel 2 created: {} ({} -> {})", rel2, node1, node3);
    tx_mgr.commit(&mut tx2).unwrap();
    store.flush().unwrap();

    // Verify first relationship pointer on node1 (should point to rel2 now)
    let n1_rec_2 = store.read_node(node1).unwrap();
    let first_rel_ptr_2 = n1_rec_2.first_rel_ptr;
    tracing::info!("Node 1 first_rel_ptr after rel2: {}", first_rel_ptr_2);
    assert_eq!(first_rel_ptr_2, rel2 + 1);

    // Verify rel2 points to rel1 via next_src_ptr
    let r2_rec = store.read_rel(rel2).unwrap();
    let next_src_ptr = r2_rec.next_src_ptr;
    tracing::info!("Rel 2 next_src_ptr: {}", next_src_ptr);
    assert_eq!(next_src_ptr, rel1 + 1, "Rel 2 should point to Rel 1");

    // Verify traversal
    let mut count = 0;
    let mut ptr = first_rel_ptr_2;

    tracing::info!("Starting traversal from ptr: {}", ptr);
    while ptr != 0 {
        let rel_id = ptr - 1;
        tracing::info!("Visiting rel_id: {}", rel_id);
        count += 1;

        let rel = store.read_rel(rel_id).unwrap();
        let src_id = rel.src_id;
        let next_src = rel.next_src_ptr;
        let next_dst = rel.next_dst_ptr;

        if src_id == node1 {
            ptr = next_src;
            tracing::info!("  Next ptr (src): {}", ptr);
        } else {
            ptr = next_dst;
            tracing::info!("  Next ptr (dst): {}", ptr);
        }

        if count > 10 {
            break;
        } // Safety break
    }

    assert_eq!(count, 2, "Should find exactly 2 relationships");
}
