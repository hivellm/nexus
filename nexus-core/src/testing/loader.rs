//! Test helper for BulkLoader tests

use crate::catalog::Catalog;
use crate::index::IndexManager;
use crate::loader::{BulkLoadConfig, BulkLoader};
use crate::storage::RecordStore;
use crate::transaction::TransactionManager;
use std::sync::Arc;
use tokio::sync::RwLock;

use super::context::TestContext;

/// Create a test BulkLoader with an isolated temporary directory
pub fn create_test_loader() -> (BulkLoader, TestContext) {
    let ctx = TestContext::new();
    let catalog = Arc::new(Catalog::new(ctx.path()).unwrap());
    let storage = Arc::new(RwLock::new(RecordStore::new(ctx.path()).unwrap()));
    let indexes = Arc::new(IndexManager::new(ctx.path()).unwrap());
    let transaction_manager = Arc::new(RwLock::new(TransactionManager::new().unwrap()));

    let loader = BulkLoader::new(
        catalog,
        storage,
        indexes,
        transaction_manager,
        BulkLoadConfig::default(),
    );

    (loader, ctx)
}
