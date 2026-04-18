use anyhow::{Result, anyhow};
use reqwest::{Client, StatusCode};
use serde::{Deserialize, Serialize};
use serde_json::Value;

pub struct NexusClient {
    client: Client,
    base_url: String,
    api_key: Option<String>,
    username: Option<String>,
    password: Option<String>,
}

#[derive(Debug, Serialize)]
struct CypherRequest {
    query: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    params: Option<Value>,
}

#[derive(Debug, Deserialize)]
pub struct QueryResult {
    pub columns: Vec<String>,
    pub rows: Vec<Vec<Value>>,
    #[serde(default)]
    pub stats: Option<QueryStats>,
}

#[derive(Debug, Deserialize)]
pub struct QueryStats {
    #[serde(default)]
    pub nodes_created: i64,
    #[serde(default)]
    pub nodes_deleted: i64,
    #[serde(default)]
    pub relationships_created: i64,
    #[serde(default)]
    pub relationships_deleted: i64,
    #[serde(default)]
    pub properties_set: i64,
    #[serde(default)]
    pub execution_time_ms: f64,
}

#[derive(Debug, Deserialize)]
pub struct UsersResponse {
    pub users: Vec<UserInfo>,
}

#[derive(Debug, Deserialize)]
pub struct KeysResponse {
    pub keys: Vec<ApiKeyInfo>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct UserInfo {
    #[serde(default)]
    pub id: Option<String>,
    pub username: String,
    #[serde(default)]
    pub email: Option<String>,
    #[serde(default)]
    pub roles: Vec<String>,
    #[serde(default)]
    pub permissions: Vec<String>,
    #[serde(default)]
    pub is_active: bool,
    #[serde(default)]
    pub is_root: bool,
    #[serde(default)]
    pub created_at: Option<String>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct ApiKeyInfo {
    pub id: String,
    pub name: String,
    #[serde(default)]
    pub permissions: Vec<String>,
    #[serde(default)]
    pub is_active: bool,
    #[serde(default)]
    pub expires_at: Option<String>,
    #[serde(default)]
    pub created_at: Option<String>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct ApiKeyCreateResponse {
    pub id: String,
    pub name: String,
    pub key: String,
    #[serde(default)]
    pub permissions: Vec<String>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct ServerStatus {
    pub status: String,
    #[serde(default)]
    pub version: Option<String>,
    #[serde(default)]
    pub uptime_seconds: Option<u64>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct DatabaseStats {
    #[serde(default)]
    pub node_count: i64,
    #[serde(default)]
    pub relationship_count: i64,
    #[serde(default)]
    pub label_count: i64,
    #[serde(default)]
    pub property_key_count: i64,
}

impl NexusClient {
    pub fn new(
        url: Option<&str>,
        api_key: Option<&str>,
        username: Option<&str>,
        password: Option<&str>,
    ) -> Result<Self> {
        let base_url = url.unwrap_or("http://localhost:3000").to_string();

        Ok(Self {
            client: Client::new(),
            base_url,
            api_key: api_key.map(String::from),
            username: username.map(String::from),
            password: password.map(String::from),
        })
    }

    fn build_request(&self, method: reqwest::Method, path: &str) -> reqwest::RequestBuilder {
        let url = format!("{}{}", self.base_url, path);
        let mut req = self.client.request(method, &url);

        if let Some(ref key) = self.api_key {
            req = req.header("X-API-Key", key);
        }

        if let (Some(user), Some(pass)) = (&self.username, &self.password) {
            req = req.basic_auth(user, Some(pass));
        }

        req
    }

    pub async fn query(&self, cypher: &str, params: Option<Value>) -> Result<QueryResult> {
        let req = CypherRequest {
            query: cypher.to_string(),
            params,
        };

        let response = self
            .build_request(reqwest::Method::POST, "/cypher")
            .json(&req)
            .send()
            .await?;

        if response.status() == StatusCode::OK {
            let result: QueryResult = response.json().await?;
            Ok(result)
        } else {
            let status = response.status();
            let text = response.text().await.unwrap_or_default();
            Err(anyhow!("Query failed ({}): {}", status, text))
        }
    }

    pub async fn ping(&self) -> Result<bool> {
        let response = self
            .build_request(reqwest::Method::GET, "/health")
            .send()
            .await?;

        Ok(response.status() == StatusCode::OK)
    }

    pub async fn status(&self) -> Result<ServerStatus> {
        let response = self
            .build_request(reqwest::Method::GET, "/status")
            .send()
            .await?;

        if response.status() == StatusCode::OK {
            Ok(response.json().await?)
        } else {
            // Fallback for servers without /status endpoint
            Ok(ServerStatus {
                status: "running".to_string(),
                version: None,
                uptime_seconds: None,
            })
        }
    }

    pub async fn health(&self) -> Result<Value> {
        let response = self
            .build_request(reqwest::Method::GET, "/health")
            .send()
            .await?;

        if response.status() == StatusCode::OK {
            Ok(response.json().await?)
        } else {
            Err(anyhow!("Health check failed"))
        }
    }

    pub async fn stats(&self) -> Result<DatabaseStats> {
        let response = self
            .build_request(reqwest::Method::GET, "/stats")
            .send()
            .await?;

        if response.status() == StatusCode::OK {
            Ok(response.json().await?)
        } else {
            Err(anyhow!("Failed to get stats"))
        }
    }

    pub async fn get_users(&self) -> Result<Vec<UserInfo>> {
        let response = self
            .build_request(reqwest::Method::GET, "/auth/users")
            .send()
            .await?;

        if response.status() == StatusCode::OK {
            let result: UsersResponse = response.json().await?;
            Ok(result.users)
        } else {
            Err(anyhow!("Failed to get users"))
        }
    }

    pub async fn create_user(
        &self,
        username: &str,
        password: &str,
        _roles: &[String],
    ) -> Result<()> {
        let cypher = format!(
            "CREATE USER {} SET PASSWORD '{}'",
            username,
            password.replace('\'', "''")
        );
        let result = self.query(&cypher, None).await?;
        if result.rows.is_empty() {
            anyhow::bail!("Failed to create user");
        }
        Ok(())
    }

    pub async fn delete_user(&self, username: &str) -> Result<()> {
        let cypher = format!("DROP USER {}", username);
        self.query(&cypher, None).await?;
        Ok(())
    }

    pub async fn get_api_keys(&self) -> Result<Vec<ApiKeyInfo>> {
        let response = self
            .build_request(reqwest::Method::GET, "/auth/keys")
            .send()
            .await?;

        if response.status() == StatusCode::OK {
            let result: KeysResponse = response.json().await?;
            Ok(result.keys)
        } else {
            Err(anyhow!("Failed to get API keys"))
        }
    }

    pub async fn create_api_key(
        &self,
        name: &str,
        permissions: &[String],
    ) -> Result<ApiKeyCreateResponse> {
        let permissions_str = if permissions.is_empty() {
            String::new()
        } else {
            format!(" WITH PERMISSIONS {}", permissions.join(", "))
        };
        let cypher = format!("CREATE API KEY {}{}", name, permissions_str);
        let result = self.query(&cypher, None).await?;

        if result.rows.is_empty() {
            anyhow::bail!("Failed to create API key");
        }

        // Columns: ["key_id", "name", "key", "message"]
        let row = &result.rows[0];
        let key_id = row
            .first()
            .and_then(|v| v.as_str())
            .unwrap_or("unknown")
            .to_string();
        let key = row
            .get(2)
            .and_then(|v| v.as_str())
            .unwrap_or("unknown")
            .to_string();

        Ok(ApiKeyCreateResponse {
            id: key_id,
            name: name.to_string(),
            key,
            permissions: permissions.to_vec(),
        })
    }

    pub async fn revoke_api_key(&self, id: &str) -> Result<()> {
        let cypher = format!("REVOKE API KEY {}", id);
        self.query(&cypher, None).await?;
        Ok(())
    }

    pub async fn get_labels(&self) -> Result<Vec<String>> {
        let result = self.query("CALL db.labels()", None).await?;
        let labels: Vec<String> = result
            .rows
            .iter()
            .filter_map(|row| row.first().and_then(|v| v.as_str().map(String::from)))
            .collect();
        Ok(labels)
    }

    pub async fn get_relationship_types(&self) -> Result<Vec<String>> {
        let result = self.query("CALL db.relationshipTypes()", None).await?;
        let types: Vec<String> = result
            .rows
            .iter()
            .filter_map(|row| row.first().and_then(|v| v.as_str().map(String::from)))
            .collect();
        Ok(types)
    }

    pub async fn get_indexes(&self) -> Result<Vec<Value>> {
        let result = self.query("SHOW INDEXES", None).await?;
        Ok(result.rows.into_iter().map(|r| Value::Array(r)).collect())
    }

    pub async fn clear_database(&self) -> Result<()> {
        self.query("MATCH (n) DETACH DELETE n", None).await?;
        Ok(())
    }

    pub async fn export_data(&self, format: &str) -> Result<String> {
        let response = self
            .build_request(reqwest::Method::GET, &format!("/export?format={}", format))
            .send()
            .await?;

        if response.status() == StatusCode::OK {
            Ok(response.text().await?)
        } else {
            Err(anyhow!("Export failed"))
        }
    }

    pub async fn import_data(&self, data: &str, format: &str) -> Result<()> {
        let response = self
            .build_request(reqwest::Method::POST, &format!("/import?format={}", format))
            .body(data.to_string())
            .send()
            .await?;

        if response.status() == StatusCode::OK {
            Ok(())
        } else {
            let text = response.text().await.unwrap_or_default();
            Err(anyhow!("Import failed: {}", text))
        }
    }
}
