//! Tests for multi-database support

use nexus_sdk::{NexusClient, NexusError, Value};
use std::collections::HashMap;

#[tokio::test]
async fn test_list_databases() -> Result<(), NexusError> {
    let client = NexusClient::new("http://localhost:15474")?;

    let databases = client.list_databases().await?;

    assert!(!databases.databases.is_empty());
    assert!(!databases.default_database.is_empty());
    // Check that default database exists in the list
    assert!(
        databases
            .databases
            .iter()
            .any(|db| db.name == databases.default_database)
    );

    Ok(())
}

#[tokio::test]
async fn test_create_and_drop_database() -> Result<(), NexusError> {
    let client = NexusClient::new("http://localhost:15474")?;

    let db_name = "test_temp_db";

    // Create database
    let create_result = client.create_database(db_name).await?;
    assert!(create_result.success);
    assert_eq!(create_result.name, db_name);

    // Verify it exists
    let mut databases = client.list_databases().await?;
    assert!(databases.databases.iter().any(|db| db.name == db_name));

    // Drop database
    let drop_result = client.drop_database(db_name).await?;
    assert!(drop_result.success);

    // Verify it's gone
    databases = client.list_databases().await?;
    assert!(!databases.databases.iter().any(|db| db.name == db_name));

    Ok(())
}

#[tokio::test]
async fn test_switch_database() -> Result<(), NexusError> {
    let client = NexusClient::new("http://localhost:15474")?;

    let db_name = "test_switch_db";

    // Create a test database
    client.create_database(db_name).await?;

    // Get initial database
    let initial_db = client.get_current_database().await?;

    // Switch to test database
    let switch_result = client.switch_database(db_name).await?;
    assert!(switch_result.success);

    // Verify we're in the new database
    let mut current_db = client.get_current_database().await?;
    assert_eq!(current_db, db_name);

    // Switch back
    let switch_back = client.switch_database(&initial_db).await?;
    assert!(switch_back.success);

    // Verify we're back
    current_db = client.get_current_database().await?;
    assert_eq!(current_db, initial_db);

    // Clean up
    client.drop_database(db_name).await?;

    Ok(())
}

#[tokio::test]
async fn test_get_database_info() -> Result<(), NexusError> {
    let client = NexusClient::new("http://localhost:15474")?;

    let db_name = "test_info_db";

    // Create a test database
    client.create_database(db_name).await?;

    // Get database info
    let db_info = client.get_database(db_name).await?;
    assert_eq!(db_info.name, db_name);
    assert!(!db_info.path.is_empty());
    // node_count, relationship_count, and storage_size are u64, so always >= 0
    // Just verify they exist (the struct was successfully deserialized)
    let _ = db_info.node_count;
    let _ = db_info.relationship_count;
    let _ = db_info.storage_size;

    // Clean up
    client.drop_database(db_name).await?;

    Ok(())
}

#[tokio::test]
async fn test_data_isolation() -> Result<(), NexusError> {
    let client = NexusClient::new("http://localhost:15474")?;

    let db1_name = "test_isolation_db1";
    let db2_name = "test_isolation_db2";

    // Create two test databases
    client.create_database(db1_name).await?;
    client.create_database(db2_name).await?;

    // Switch to db1 and create a node
    client.switch_database(db1_name).await?;
    let mut params = HashMap::new();
    params.insert("name".to_string(), Value::String("DB1 Node".to_string()));
    let result = client
        .execute_cypher("CREATE (n:TestNode {name: $name}) RETURN n", Some(params))
        .await?;
    assert_eq!(result.rows.len(), 1);

    // Verify node exists in db1
    let count_result = client
        .execute_cypher("MATCH (n:TestNode) RETURN count(n) AS count", None)
        .await?;
    // Row is an array: [count_value]
    let count = count_result.rows[0].as_array().unwrap()[0]
        .as_i64()
        .unwrap();
    assert_eq!(count, 1);

    // Switch to db2
    client.switch_database(db2_name).await?;

    // Verify node does NOT exist in db2 (isolation)
    let count_result = client
        .execute_cypher("MATCH (n:TestNode) RETURN count(n) AS count", None)
        .await?;
    let count = count_result.rows[0].as_array().unwrap()[0]
        .as_i64()
        .unwrap();
    assert_eq!(count, 0);

    // Create a different node in db2
    let mut params = HashMap::new();
    params.insert("name".to_string(), Value::String("DB2 Node".to_string()));
    let result = client
        .execute_cypher("CREATE (n:TestNode {name: $name}) RETURN n", Some(params))
        .await?;
    assert_eq!(result.rows.len(), 1);

    // Verify only one node in db2
    let count_result = client
        .execute_cypher("MATCH (n:TestNode) RETURN count(n) AS count", None)
        .await?;
    let count = count_result.rows[0].as_array().unwrap()[0]
        .as_i64()
        .unwrap();
    assert_eq!(count, 1);

    // Switch back to db1
    client.switch_database(db1_name).await?;

    // Verify still only one node in db1
    let count_result = client
        .execute_cypher("MATCH (n:TestNode) RETURN count(n) AS count", None)
        .await?;
    let count = count_result.rows[0].as_array().unwrap()[0]
        .as_i64()
        .unwrap();
    assert_eq!(count, 1);

    // Clean up
    let databases = client.list_databases().await?;
    client.switch_database(&databases.default_database).await?;
    client.drop_database(db1_name).await?;
    client.drop_database(db2_name).await?;

    Ok(())
}

#[tokio::test]
async fn test_client_with_database_parameter() -> Result<(), NexusError> {
    // Create a test database first
    let setup_client = NexusClient::new("http://localhost:15474")?;

    let db_name = "test_param_db";
    setup_client.create_database(db_name).await?;

    // Create a client and switch to the specific database
    let client = NexusClient::new("http://localhost:15474")?;
    client.switch_database(db_name).await?;

    // Verify we're connected to the right database
    let current_db = client.get_current_database().await?;
    assert_eq!(current_db, db_name);

    // Clean up
    let databases = setup_client.list_databases().await?;
    setup_client
        .switch_database(&databases.default_database)
        .await?;
    setup_client.drop_database(db_name).await?;

    Ok(())
}

#[tokio::test]
async fn test_cannot_drop_current_database() -> Result<(), NexusError> {
    let client = NexusClient::new("http://localhost:15474")?;

    let db_name = "test_no_drop_db";

    // Create a test database
    client.create_database(db_name).await?;

    // Switch to the database
    client.switch_database(db_name).await?;

    // Try to drop it while it's active - should fail
    let drop_result: Result<_, NexusError> = client.drop_database(db_name).await;
    assert!(drop_result.is_err());

    // Switch to a different database
    let databases = client.list_databases().await?;
    client.switch_database(&databases.default_database).await?;

    // Now we should be able to drop it
    let drop_result = client.drop_database(db_name).await?;
    assert!(drop_result.success);

    Ok(())
}

#[tokio::test]
async fn test_cannot_drop_default_database() -> Result<(), NexusError> {
    let client = NexusClient::new("http://localhost:15474")?;

    // Get default database
    let databases = client.list_databases().await?;
    let default_db = &databases.default_database;

    // Try to drop it - should fail
    let drop_result: Result<_, NexusError> = client.drop_database(default_db).await;
    assert!(drop_result.is_err());

    Ok(())
}
