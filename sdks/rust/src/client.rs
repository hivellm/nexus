//! Nexus client implementation

use crate::error::{NexusError, Result};
use crate::models::*;
use base64::Engine;
use reqwest::{Client, ClientBuilder, Response};
use std::collections::HashMap;
use std::time::Duration;
use url::Url;

/// Nexus client for interacting with the Nexus graph database
#[derive(Debug, Clone)]
pub struct NexusClient {
    /// HTTP client
    client: Client,
    /// Base URL
    base_url: Url,
    /// API key (optional)
    api_key: Option<String>,
    /// Username (optional)
    username: Option<String>,
    /// Password (optional)
    password: Option<String>,
    /// Maximum retries (currently unused, reserved for future retry implementation)
    #[allow(dead_code)]
    max_retries: u32,
}

impl NexusClient {
    /// Get the HTTP client
    pub(crate) fn get_client(&self) -> &Client {
        &self.client
    }

    /// Get the base URL
    pub(crate) fn get_base_url(&self) -> &Url {
        &self.base_url
    }

    /// Create a new Nexus client with default configuration
    ///
    /// # Arguments
    ///
    /// * `base_url` - Base URL of the Nexus server (e.g., "http://localhost:15474")
    ///
    /// # Example
    ///
    /// ```no_run
    /// use nexus_sdk::NexusClient;
    ///
    /// let client = NexusClient::new("http://localhost:15474")?;
    /// # Ok::<(), nexus_sdk::NexusError>(())
    /// ```
    pub fn new(base_url: &str) -> Result<Self> {
        Self::with_config(ClientConfig {
            base_url: base_url.to_string(),
            ..Default::default()
        })
    }

    /// Create a new Nexus client with API key authentication
    ///
    /// # Arguments
    ///
    /// * `base_url` - Base URL of the Nexus server
    /// * `api_key` - API key for authentication
    ///
    /// # Example
    ///
    /// ```no_run
    /// use nexus_sdk::NexusClient;
    ///
    /// let client = NexusClient::with_api_key("http://localhost:15474", "your-api-key")?;
    /// # Ok::<(), nexus_sdk::NexusError>(())
    /// ```
    pub fn with_api_key(base_url: &str, api_key: &str) -> Result<Self> {
        Self::with_config(ClientConfig {
            base_url: base_url.to_string(),
            api_key: Some(api_key.to_string()),
            ..Default::default()
        })
    }

    /// Create a new Nexus client with username/password authentication
    ///
    /// # Arguments
    ///
    /// * `base_url` - Base URL of the Nexus server
    /// * `username` - Username for authentication
    /// * `password` - Password for authentication
    ///
    /// # Example
    ///
    /// ```no_run
    /// use nexus_sdk::NexusClient;
    ///
    /// let client = NexusClient::with_credentials("http://localhost:15474", "user", "pass")?;
    /// # Ok::<(), nexus_sdk::NexusError>(())
    /// ```
    pub fn with_credentials(base_url: &str, username: &str, password: &str) -> Result<Self> {
        Self::with_config(ClientConfig {
            base_url: base_url.to_string(),
            username: Some(username.to_string()),
            password: Some(password.to_string()),
            ..Default::default()
        })
    }

    /// Create a new Nexus client with custom configuration
    ///
    /// # Arguments
    ///
    /// * `config` - Client configuration
    pub fn with_config(config: ClientConfig) -> Result<Self> {
        let base_url = Url::parse(&config.base_url)
            .map_err(|e| NexusError::Configuration(format!("Invalid base URL: {}", e)))?;

        let timeout = Duration::from_secs(config.timeout_secs);

        let client_builder = ClientBuilder::new()
            .timeout(timeout)
            .user_agent("nexus-sdk/0.1.0");

        // Build HTTP client
        let client = client_builder.build()?;

        Ok(Self {
            client,
            base_url,
            api_key: config.api_key,
            username: config.username,
            password: config.password,
            max_retries: config.max_retries,
        })
    }

    /// Execute a Cypher query
    ///
    /// # Arguments
    ///
    /// * `query` - Cypher query string
    /// * `parameters` - Optional query parameters
    ///
    /// # Example
    ///
    /// ```no_run
    /// # use nexus_sdk::NexusClient;
    /// # #[tokio::main]
    /// # async fn main() -> Result<(), nexus_sdk::NexusError> {
    /// # let client = NexusClient::new("http://localhost:15474")?;
    /// let result = client.execute_cypher("MATCH (n) RETURN n LIMIT 10", None).await?;
    /// tracing::info!("Found {} rows", result.rows.len());
    /// # Ok(())
    /// # }
    /// ```
    pub async fn execute_cypher(
        &self,
        query: &str,
        parameters: Option<HashMap<String, Value>>,
    ) -> Result<QueryResult> {
        let request = CypherRequest {
            query: query.to_string(),
            parameters,
        };

        let url = self.get_base_url().join("/cypher")?;
        let mut request_builder = self.get_client().post(url).json(&request);

        // Add authentication headers
        request_builder = self.add_auth_headers(request_builder)?;

        let response = self.execute_with_retry(request_builder).await?;
        self.handle_response(response).await
    }

    /// Get database statistics
    ///
    /// # Example
    ///
    /// ```no_run
    /// # use nexus_sdk::NexusClient;
    /// # #[tokio::main]
    /// # async fn main() -> Result<(), nexus_sdk::NexusError> {
    /// # let client = NexusClient::new("http://localhost:15474")?;
    /// let stats = client.get_stats().await?;
    /// tracing::info!("Total nodes: {}", stats.catalog.node_count);
    /// # Ok(())
    /// # }
    /// ```
    pub async fn get_stats(&self) -> Result<DatabaseStats> {
        let url = self.get_base_url().join("/stats")?;
        let mut request_builder = self.get_client().get(url);

        // Add authentication headers
        request_builder = self.add_auth_headers(request_builder)?;

        let response = self.execute_with_retry(request_builder).await?;
        let stats: DatabaseStats = response.json().await?;
        Ok(stats)
    }

    /// Check server health
    ///
    /// # Example
    ///
    /// ```no_run
    /// # use nexus_sdk::NexusClient;
    /// # #[tokio::main]
    /// # async fn main() -> Result<(), nexus_sdk::NexusError> {
    /// # let client = NexusClient::new("http://localhost:15474")?;
    /// let healthy = client.health_check().await?;
    /// tracing::info!("Server is healthy: {}", healthy);
    /// # Ok(())
    /// # }
    /// ```
    pub async fn health_check(&self) -> Result<bool> {
        let url = self.get_base_url().join("/health")?;
        let request_builder = self.get_client().get(url);

        match self.execute_with_retry(request_builder).await {
            Ok(response) => Ok(response.status().is_success()),
            Err(_) => Ok(false),
        }
    }

    /// Add authentication headers to request
    pub(crate) fn add_auth_headers(
        &self,
        mut builder: reqwest::RequestBuilder,
    ) -> Result<reqwest::RequestBuilder> {
        if let Some(api_key) = &self.api_key {
            builder = builder.header("X-API-Key", api_key);
        } else if let (Some(username), Some(password)) = (&self.username, &self.password) {
            // For basic auth, we'll need to handle token management
            // For now, we'll add basic auth header
            let auth = base64::engine::general_purpose::STANDARD
                .encode(format!("{}:{}", username, password));
            builder = builder.header("Authorization", format!("Basic {}", auth));
        }

        Ok(builder)
    }

    /// Execute request with retry logic
    pub(crate) async fn execute_with_retry(
        &self,
        builder: reqwest::RequestBuilder,
    ) -> Result<Response> {
        let max_retries = self.max_retries;
        let mut last_error = None;

        for attempt in 0..=max_retries {
            match builder.try_clone() {
                Some(cloned_builder) => {
                    match cloned_builder.send().await {
                        Ok(response) => {
                            // Check if status is retryable (5xx errors)
                            let status = response.status();
                            if status.is_server_error() && attempt < max_retries {
                                // Calculate exponential backoff delay
                                let delay_ms = 100u64 * (1u64 << attempt.min(5)); // Cap at 3.2s
                                tokio::time::sleep(tokio::time::Duration::from_millis(delay_ms))
                                    .await;
                                continue;
                            }
                            return Ok(response);
                        }
                        Err(e) => {
                            // Check if error is retryable (network errors, timeouts) before moving
                            let is_retryable = e.is_timeout() || e.is_connect() || e.is_request();
                            last_error = Some(e);
                            if is_retryable && attempt < max_retries {
                                // Calculate exponential backoff delay
                                let delay_ms = 100u64 * (1u64 << attempt.min(5)); // Cap at 3.2s
                                tokio::time::sleep(tokio::time::Duration::from_millis(delay_ms))
                                    .await;
                                continue;
                            }
                            // Non-retryable error or max retries reached
                            break;
                        }
                    }
                }
                None => {
                    // Cannot clone builder, execute directly
                    return builder.send().await.map_err(NexusError::Http);
                }
            }
        }

        // Return last error or create a generic error
        match last_error {
            Some(e) => Err(NexusError::Http(e)),
            None => Err(NexusError::Network(
                "Request failed after retries".to_string(),
            )),
        }
    }

    /// Handle HTTP response and convert to result
    async fn handle_response(&self, response: Response) -> Result<QueryResult> {
        let status = response.status();

        if status.is_success() {
            let result: QueryResult = response.json().await?;
            Ok(result)
        } else {
            let error_text = response
                .text()
                .await
                .unwrap_or_else(|_| "Unknown error".to_string());
            Err(NexusError::Api {
                message: error_text,
                status: status.as_u16(),
            })
        }
    }

    // =========================================================================
    // Database Management Methods
    // =========================================================================

    /// List all databases
    ///
    /// # Example
    ///
    /// ```no_run
    /// # use nexus_sdk::NexusClient;
    /// # #[tokio::main]
    /// # async fn main() -> Result<(), nexus_sdk::NexusError> {
    /// # let client = NexusClient::new("http://localhost:15474")?;
    /// let response = client.list_databases().await?;
    /// for db in &response.databases {
    ///     println!("Database: {}", db.name);
    /// }
    /// # Ok(())
    /// # }
    /// ```
    pub async fn list_databases(&self) -> Result<ListDatabasesResponse> {
        let url = self.get_base_url().join("/databases")?;
        let mut request_builder = self.get_client().get(url);
        request_builder = self.add_auth_headers(request_builder)?;

        let response = self.execute_with_retry(request_builder).await?;
        let result: ListDatabasesResponse = response.json().await?;
        Ok(result)
    }

    /// Create a new database
    ///
    /// # Arguments
    ///
    /// * `name` - Database name (alphanumeric with underscores and hyphens)
    ///
    /// # Example
    ///
    /// ```no_run
    /// # use nexus_sdk::NexusClient;
    /// # #[tokio::main]
    /// # async fn main() -> Result<(), nexus_sdk::NexusError> {
    /// # let client = NexusClient::new("http://localhost:15474")?;
    /// let response = client.create_database("mydb").await?;
    /// println!("Created database: {}", response.name);
    /// # Ok(())
    /// # }
    /// ```
    pub async fn create_database(&self, name: &str) -> Result<CreateDatabaseResponse> {
        let url = self.get_base_url().join("/databases")?;
        let request = CreateDatabaseRequest {
            name: name.to_string(),
        };
        let mut request_builder = self.get_client().post(url).json(&request);
        request_builder = self.add_auth_headers(request_builder)?;

        let response = self.execute_with_retry(request_builder).await?;
        let result: CreateDatabaseResponse = response.json().await?;
        Ok(result)
    }

    /// Get database information
    ///
    /// # Arguments
    ///
    /// * `name` - Database name
    ///
    /// # Example
    ///
    /// ```no_run
    /// # use nexus_sdk::NexusClient;
    /// # #[tokio::main]
    /// # async fn main() -> Result<(), nexus_sdk::NexusError> {
    /// # let client = NexusClient::new("http://localhost:15474")?;
    /// let db = client.get_database("neo4j").await?;
    /// println!("Nodes: {}", db.node_count);
    /// # Ok(())
    /// # }
    /// ```
    pub async fn get_database(&self, name: &str) -> Result<DatabaseInfo> {
        let url = self.get_base_url().join(&format!("/databases/{}", name))?;
        let mut request_builder = self.get_client().get(url);
        request_builder = self.add_auth_headers(request_builder)?;

        let response = self.execute_with_retry(request_builder).await?;
        let result: DatabaseInfo = response.json().await?;
        Ok(result)
    }

    /// Drop a database
    ///
    /// # Arguments
    ///
    /// * `name` - Database name
    ///
    /// # Example
    ///
    /// ```no_run
    /// # use nexus_sdk::NexusClient;
    /// # #[tokio::main]
    /// # async fn main() -> Result<(), nexus_sdk::NexusError> {
    /// # let client = NexusClient::new("http://localhost:15474")?;
    /// let response = client.drop_database("mydb").await?;
    /// println!("Dropped: {}", response.success);
    /// # Ok(())
    /// # }
    /// ```
    pub async fn drop_database(&self, name: &str) -> Result<DropDatabaseResponse> {
        let url = self.get_base_url().join(&format!("/databases/{}", name))?;
        let mut request_builder = self.get_client().delete(url);
        request_builder = self.add_auth_headers(request_builder)?;

        let response = self.execute_with_retry(request_builder).await?;
        let result: DropDatabaseResponse = response.json().await?;
        Ok(result)
    }

    /// Get the current session database
    ///
    /// # Example
    ///
    /// ```no_run
    /// # use nexus_sdk::NexusClient;
    /// # #[tokio::main]
    /// # async fn main() -> Result<(), nexus_sdk::NexusError> {
    /// # let client = NexusClient::new("http://localhost:15474")?;
    /// let db = client.get_current_database().await?;
    /// println!("Current database: {}", db);
    /// # Ok(())
    /// # }
    /// ```
    pub async fn get_current_database(&self) -> Result<String> {
        let url = self.get_base_url().join("/session/database")?;
        let mut request_builder = self.get_client().get(url);
        request_builder = self.add_auth_headers(request_builder)?;

        let response = self.execute_with_retry(request_builder).await?;
        let result: SessionDatabaseResponse = response.json().await?;
        Ok(result.database)
    }

    /// Switch to a different database
    ///
    /// # Arguments
    ///
    /// * `name` - Database name to switch to
    ///
    /// # Example
    ///
    /// ```no_run
    /// # use nexus_sdk::NexusClient;
    /// # #[tokio::main]
    /// # async fn main() -> Result<(), nexus_sdk::NexusError> {
    /// # let client = NexusClient::new("http://localhost:15474")?;
    /// let response = client.switch_database("mydb").await?;
    /// println!("Switched: {}", response.success);
    /// # Ok(())
    /// # }
    /// ```
    pub async fn switch_database(&self, name: &str) -> Result<SwitchDatabaseResponse> {
        let url = self.get_base_url().join("/session/database")?;
        let request = SwitchDatabaseRequest {
            name: name.to_string(),
        };
        let mut request_builder = self.get_client().put(url).json(&request);
        request_builder = self.add_auth_headers(request_builder)?;

        let response = self.execute_with_retry(request_builder).await?;
        let result: SwitchDatabaseResponse = response.json().await?;
        Ok(result)
    }
}
