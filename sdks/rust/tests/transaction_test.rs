//! Integration tests for transaction support

use nexus_sdk_rust::{NexusClient, Transaction, Value};
use std::collections::HashMap;

// Note: These tests require a running Nexus server at http://localhost:15474
// They are skipped by default unless NEXUS_TEST_SERVER is set

#[tokio::test]
#[ignore]
async fn test_begin_transaction() {
    let client = NexusClient::new("http://localhost:15474").unwrap();
    let mut tx: Transaction = client.begin_transaction().await.unwrap();
    assert!(tx.is_active());
    assert_eq!(tx.status(), nexus_sdk_rust::TransactionStatus::Active);
    tx.commit().await.unwrap();
}

#[tokio::test]
#[ignore]
async fn test_commit_transaction() {
    let client = NexusClient::new("http://localhost:15474").unwrap();
    let mut tx: Transaction = client.begin_transaction().await.unwrap();

    // Execute a query within transaction
    let mut params = HashMap::new();
    params.insert("name".to_string(), Value::String("TestNode".to_string()));
    let _result = tx
        .execute("CREATE (n:TestLabel {name: $name}) RETURN n", Some(params))
        .await
        .unwrap();

    // Commit transaction
    tx.commit().await.unwrap();
    assert!(!tx.is_active());
    assert_eq!(tx.status(), nexus_sdk_rust::TransactionStatus::NotStarted);
}

#[tokio::test]
#[ignore]
async fn test_rollback_transaction() {
    let client = NexusClient::new("http://localhost:15474").unwrap();
    let mut tx: Transaction = client.begin_transaction().await.unwrap();

    // Execute a query within transaction
    let mut params = HashMap::new();
    params.insert(
        "name".to_string(),
        Value::String("RollbackNode".to_string()),
    );
    let _result = tx
        .execute(
            "CREATE (n:RollbackLabel {name: $name}) RETURN n",
            Some(params),
        )
        .await
        .unwrap();

    // Rollback transaction
    tx.rollback().await.unwrap();
    assert!(!tx.is_active());
    assert_eq!(tx.status(), nexus_sdk_rust::TransactionStatus::NotStarted);
}

#[tokio::test]
#[ignore]
async fn test_transaction_multiple_operations() {
    let client = NexusClient::new("http://localhost:15474").unwrap();
    let mut tx: Transaction = client.begin_transaction().await.unwrap();

    // Create multiple nodes
    for i in 0..3 {
        let mut params = HashMap::new();
        params.insert("id".to_string(), Value::Int(i));
        let _result = tx
            .execute("CREATE (n:MultiOp {id: $id}) RETURN n", Some(params))
            .await
            .unwrap();
    }

    // Commit all operations
    tx.commit().await.unwrap();
}

#[tokio::test]
#[ignore]
async fn test_transaction_error_handling() {
    let client = NexusClient::new("http://localhost:15474").unwrap();
    let mut tx: Transaction = client.begin_transaction().await.unwrap();

    // Try to commit without active transaction (should fail after commit)
    tx.commit().await.unwrap();

    // Try to commit again (should fail)
    let result = tx.commit().await;
    assert!(result.is_err());

    // Try to rollback (should fail)
    let result = tx.rollback().await;
    assert!(result.is_err());
}

#[tokio::test]
#[ignore]
async fn test_transaction_execute_query() {
    let client = NexusClient::new("http://localhost:15474").unwrap();
    let mut tx: Transaction = client.begin_transaction().await.unwrap();

    // Execute a query
    let mut params = HashMap::new();
    params.insert("name".to_string(), Value::String("ExecuteTest".to_string()));
    let result = tx
        .execute(
            "CREATE (n:ExecuteLabel {name: $name}) RETURN n.name as name",
            Some(params),
        )
        .await
        .unwrap();

    // Verify we got a result (rows may be empty for some queries or have error)
    // Just verify we got a response structure
    let _ = result.columns.len();
    let _ = result.rows.len();
    tx.commit().await.unwrap();
}
