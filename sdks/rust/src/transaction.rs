//! Transaction support for Nexus SDK

use crate::client::NexusClient;
use crate::error::{NexusError, Result};

/// Transaction handle for managing database transactions
#[derive(Debug, Clone)]
pub struct Transaction {
    client: NexusClient,
    transaction_id: Option<String>,
    active: bool,
}

/// Transaction status
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TransactionStatus {
    /// Transaction is active
    Active,
    /// Transaction has been committed
    Committed,
    /// Transaction has been rolled back
    RolledBack,
    /// Transaction is not started
    NotStarted,
}

impl Transaction {
    /// Create a new transaction handle
    pub(crate) fn new(client: NexusClient) -> Self {
        Self {
            client,
            transaction_id: None,
            active: false,
        }
    }

    /// Begin a new transaction
    ///
    /// # Example
    ///
    /// ```no_run
    /// # use nexus_sdk::NexusClient;
    /// # #[tokio::main]
    /// # async fn main() -> Result<(), nexus_sdk::NexusError> {
    /// # let client = NexusClient::new("http://localhost:15474")?;
    /// let mut tx = client.begin_transaction().await?;
    /// // Perform operations...
    /// tx.commit().await?;
    /// # Ok(())
    /// # }
    /// ```
    pub async fn begin(&mut self) -> Result<()> {
        if self.active {
            return Err(NexusError::Validation(
                "Transaction already active".to_string(),
            ));
        }

        let _result = self
            .client
            .execute_cypher("BEGIN TRANSACTION", None)
            .await?;

        self.active = true;
        self.transaction_id = Some(format!(
            "tx_{}",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));

        Ok(())
    }

    /// Commit the transaction
    ///
    /// # Example
    ///
    /// ```no_run
    /// # use nexus_sdk::{NexusClient, Transaction};
    /// # #[tokio::main]
    /// # async fn main() -> Result<(), nexus_sdk::NexusError> {
    /// # let client = NexusClient::new("http://localhost:15474")?;
    /// # let mut tx: Transaction = client.begin_transaction().await?;
    /// tx.commit().await?;
    /// # Ok(())
    /// # }
    /// ```
    pub async fn commit(&mut self) -> Result<()> {
        if !self.active {
            return Err(NexusError::Validation(
                "No active transaction to commit".to_string(),
            ));
        }

        let _result = self
            .client
            .execute_cypher("COMMIT TRANSACTION", None)
            .await?;

        self.active = false;
        self.transaction_id = None;

        Ok(())
    }

    /// Rollback the transaction
    ///
    /// # Example
    ///
    /// ```no_run
    /// # use nexus_sdk::{NexusClient, Transaction};
    /// # #[tokio::main]
    /// # async fn main() -> Result<(), nexus_sdk::NexusError> {
    /// # let client = NexusClient::new("http://localhost:15474")?;
    /// # let mut tx: Transaction = client.begin_transaction().await?;
    /// tx.rollback().await?;
    /// # Ok(())
    /// # }
    /// ```
    pub async fn rollback(&mut self) -> Result<()> {
        if !self.active {
            return Err(NexusError::Validation(
                "No active transaction to rollback".to_string(),
            ));
        }

        let _result = self
            .client
            .execute_cypher("ROLLBACK TRANSACTION", None)
            .await?;

        self.active = false;
        self.transaction_id = None;

        Ok(())
    }

    /// Check if transaction is active
    pub fn is_active(&self) -> bool {
        self.active
    }

    /// Get transaction status
    pub fn status(&self) -> TransactionStatus {
        if self.active {
            TransactionStatus::Active
        } else if self.transaction_id.is_some() {
            TransactionStatus::Committed
        } else {
            TransactionStatus::NotStarted
        }
    }

    /// Execute a Cypher query within this transaction
    ///
    /// # Example
    ///
    /// ```no_run
    /// # use nexus_sdk::{NexusClient, Transaction};
    /// # use std::collections::HashMap;
    /// # #[tokio::main]
    /// # async fn main() -> Result<(), nexus_sdk::NexusError> {
    /// # let client = NexusClient::new("http://localhost:15474")?;
    /// # let mut tx: Transaction = client.begin_transaction().await?;
    /// let result = tx.execute("CREATE (n:Person {name: 'Alice'}) RETURN n", None).await?;
    /// tx.commit().await?;
    /// # Ok(())
    /// # }
    /// ```
    pub async fn execute(
        &self,
        query: &str,
        params: Option<std::collections::HashMap<String, crate::models::Value>>,
    ) -> Result<crate::models::QueryResult> {
        if !self.active {
            return Err(NexusError::Validation(
                "Transaction is not active".to_string(),
            ));
        }

        self.client.execute_cypher(query, params).await
    }
}

impl NexusClient {
    /// Begin a new transaction
    ///
    /// # Example
    ///
    /// ```no_run
    /// # use nexus_sdk::{NexusClient, Transaction};
    /// # #[tokio::main]
    /// # async fn main() -> Result<(), nexus_sdk::NexusError> {
    /// # let client = NexusClient::new("http://localhost:15474")?;
    /// let mut tx: Transaction = client.begin_transaction().await?;
    /// // Perform operations...
    /// tx.commit().await?;
    /// # Ok(())
    /// # }
    /// ```
    pub async fn begin_transaction(&self) -> Result<Transaction> {
        let mut tx = Transaction::new(self.clone());
        tx.begin().await?;
        Ok(tx)
    }
}
