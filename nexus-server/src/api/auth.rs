//! Authentication and user management REST API endpoints

use crate::NexusServer;
use axum::extract::{Extension, Path, Query, State};
use axum::http::StatusCode;
use axum::response::Json;
use nexus_core::auth::middleware::AuthContext;
use nexus_core::auth::{Permission, User, verify_password};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Arc;

/// Request to create a user
#[derive(Debug, Deserialize)]
pub struct CreateUserRequest {
    /// Username
    pub username: String,
    /// Password (optional)
    pub password: Option<String>,
    /// Email (optional)
    pub email: Option<String>,
}

/// Request to update user permissions
#[derive(Debug, Deserialize)]
pub struct UpdatePermissionsRequest {
    /// Permissions to add
    pub permissions: Vec<String>,
}

/// User response
#[derive(Debug, Serialize)]
pub struct UserResponse {
    /// User ID
    pub id: String,
    /// Username
    pub username: String,
    /// Email
    pub email: Option<String>,
    /// Roles
    pub roles: Vec<String>,
    /// Permissions
    pub permissions: Vec<String>,
    /// Whether user is active
    pub is_active: bool,
    /// Whether user is root
    pub is_root: bool,
}

/// List of users response
#[derive(Debug, Serialize)]
pub struct UsersResponse {
    /// List of users
    pub users: Vec<UserResponse>,
}

/// Create a new user
/// POST /auth/users
pub async fn create_user(
    State(server): State<Arc<NexusServer>>,
    Extension(auth_context): Extension<Option<AuthContext>>,
    Json(request): Json<CreateUserRequest>,
) -> Result<Json<UserResponse>, (StatusCode, Json<serde_json::Value>)> {
    let mut rbac = server.rbac.write().await;

    // Check if user already exists
    let users_list = rbac.list_users();
    if users_list.iter().any(|u| u.username == request.username) {
        return Err((
            StatusCode::CONFLICT,
            Json(serde_json::json!({
                "error": format!("User '{}' already exists", request.username)
            })),
        ));
    }

    let user_id = uuid::Uuid::new_v4().to_string();
    let user = if let Some(password) = &request.password {
        // Hash password with SHA512
        let password_hash = nexus_core::auth::hash_password(password);

        let mut user =
            User::with_password_hash(user_id.clone(), request.username.clone(), password_hash);
        if let Some(email) = request.email {
            user.email = Some(email);
        }
        user
    } else {
        let mut user = User::new(user_id.clone(), request.username.clone());
        if let Some(email) = request.email {
            user.email = Some(email);
        }
        user
    };

    rbac.add_user(user.clone());

    // Extract actor info from auth context for audit logging
    let (actor_user_id, actor_username, _) = auth_context
        .as_ref()
        .map(|ctx| {
            let api_key_id = Some(ctx.api_key.id.clone());
            let user_id = ctx.api_key.user_id.clone();
            let username = None; // Username not available in ApiKey
            (user_id, username, api_key_id)
        })
        .unwrap_or((None, None, None));

    // Check if root should be disabled after first admin creation
    drop(rbac); // Release lock before async call
    server.check_and_disable_root_if_needed().await;

    // Log user creation
    let _ = server
        .audit_logger
        .log_user_created(
            actor_user_id,
            actor_username,
            user.username.clone(),
            user.id.clone(),
            nexus_core::auth::AuditResult::Success,
        )
        .await;

    let permissions: Vec<String> = user
        .additional_permissions
        .permissions()
        .iter()
        .map(|p| p.to_string())
        .collect();

    Ok(Json(UserResponse {
        id: user.id,
        username: user.username,
        email: user.email,
        roles: user.roles,
        permissions,
        is_active: user.is_active,
        is_root: user.is_root,
    }))
}

/// List all users
/// GET /auth/users
pub async fn list_users(State(server): State<Arc<NexusServer>>) -> Json<UsersResponse> {
    let rbac = server.rbac.read().await;
    let users = rbac.list_users();

    let user_responses: Vec<UserResponse> = users
        .iter()
        .map(|user| {
            let permissions: Vec<String> = user
                .additional_permissions
                .permissions()
                .iter()
                .map(|p| p.to_string())
                .collect();

            UserResponse {
                id: user.id.clone(),
                username: user.username.clone(),
                email: user.email.clone(),
                roles: user.roles.clone(),
                permissions,
                is_active: user.is_active,
                is_root: user.is_root,
            }
        })
        .collect();

    Json(UsersResponse {
        users: user_responses,
    })
}

/// Get a specific user
/// GET /auth/users/{username}
pub async fn get_user(
    State(server): State<Arc<NexusServer>>,
    Path(username): Path<String>,
) -> Result<Json<UserResponse>, (StatusCode, Json<serde_json::Value>)> {
    let rbac = server.rbac.read().await;
    let users = rbac.list_users();

    let user = users.iter().find(|u| u.username == username);

    if let Some(user) = user {
        let permissions: Vec<String> = user
            .additional_permissions
            .permissions()
            .iter()
            .map(|p| p.to_string())
            .collect();

        Ok(Json(UserResponse {
            id: user.id.clone(),
            username: user.username.clone(),
            email: user.email.clone(),
            roles: user.roles.clone(),
            permissions,
            is_active: user.is_active,
            is_root: user.is_root,
        }))
    } else {
        Err((
            StatusCode::NOT_FOUND,
            Json(serde_json::json!({
                "error": format!("User '{}' not found", username)
            })),
        ))
    }
}

/// Delete a user
/// DELETE /auth/users/{username}
pub async fn delete_user(
    State(server): State<Arc<NexusServer>>,
    Extension(auth_context): Extension<Option<AuthContext>>,
    Path(username): Path<String>,
) -> Result<Json<serde_json::Value>, (StatusCode, Json<serde_json::Value>)> {
    let mut rbac = server.rbac.write().await;
    let users_list = rbac.list_users();

    let user_info = users_list
        .iter()
        .find(|u| u.username == username)
        .map(|u| (u.id.clone(), u.is_root));

    if let Some((user_id, is_root)) = user_info {
        // Prevent deletion of root user
        if is_root {
            return Err((
                StatusCode::FORBIDDEN,
                Json(serde_json::json!({
                    "error": "Cannot delete root user. Use DISABLE instead."
                })),
            ));
        }

        if rbac.remove_user(&user_id).is_some() {
            // Extract actor info from auth context for audit logging
            let (actor_user_id, actor_username, _) = auth_context
                .as_ref()
                .map(|ctx| {
                    let api_key_id = Some(ctx.api_key.id.clone());
                    let user_id = ctx.api_key.user_id.clone();
                    let username = None; // Username not available in ApiKey
                    (user_id, username, api_key_id)
                })
                .unwrap_or((None, None, None));

            // Log user deletion
            let _ = server
                .audit_logger
                .log_user_deleted(
                    actor_user_id,
                    actor_username,
                    username.clone(),
                    user_id.clone(),
                    nexus_core::auth::AuditResult::Success,
                )
                .await;

            Ok(Json(serde_json::json!({
                "message": format!("User '{}' deleted successfully", username)
            })))
        } else {
            Err((
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({
                    "error": format!("Failed to delete user '{}'", username)
                })),
            ))
        }
    } else {
        Err((
            StatusCode::NOT_FOUND,
            Json(serde_json::json!({
                "error": format!("User '{}' not found", username)
            })),
        ))
    }
}

/// Grant permissions to a user
/// POST /auth/users/{username}/permissions
pub async fn grant_permissions(
    State(server): State<Arc<NexusServer>>,
    Extension(auth_context): Extension<Option<AuthContext>>,
    Path(username): Path<String>,
    Json(request): Json<UpdatePermissionsRequest>,
) -> Result<Json<serde_json::Value>, (StatusCode, Json<serde_json::Value>)> {
    let mut rbac = server.rbac.write().await;

    // Parse permissions
    let permissions: Result<Vec<Permission>, _> = request
        .permissions
        .iter()
        .map(|p| match p.to_uppercase().as_str() {
            "READ" => Ok(Permission::Read),
            "WRITE" => Ok(Permission::Write),
            "ADMIN" => Ok(Permission::Admin),
            "SUPER" => Ok(Permission::Super),
            "QUEUE" => Ok(Permission::Queue),
            "CHATROOM" => Ok(Permission::Chatroom),
            "REST" => Ok(Permission::Rest),
            _ => Err(format!("Unknown permission: {}", p)),
        })
        .collect();

    let permissions = match permissions {
        Ok(p) => p,
        Err(e) => {
            return Err((
                StatusCode::BAD_REQUEST,
                Json(serde_json::json!({ "error": e })),
            ));
        }
    };

    let users_list = rbac.list_users();
    let target_user = users_list.iter().find(|u| u.username == username);

    // Check if trying to modify root user
    if let Some(user) = target_user {
        if user.is_root {
            return Err((
                StatusCode::FORBIDDEN,
                Json(serde_json::json!({
                    "error": "Cannot modify root user permissions. Only root users can modify root users."
                })),
            ));
        }
    }

    let target_user_id = target_user.map(|u| u.id.clone());

    if let Some(target_user_id) = target_user_id {
        let is_admin_grant = permissions.iter().any(|p| matches!(p, Permission::Admin));
        if let Some(user_mut) = rbac.get_user_mut(&target_user_id) {
            for perm in &permissions {
                user_mut.add_permission(perm.clone());
            }
        }

        // Extract actor info from auth context for audit logging
        let (actor_user_id, actor_username, _) = auth_context
            .as_ref()
            .map(|ctx| {
                let api_key_id = Some(ctx.api_key.id.clone());
                let user_id = ctx.api_key.user_id.clone();
                let username = None; // Username not available in ApiKey
                (user_id, username, api_key_id)
            })
            .unwrap_or((None, None, None));
        let permission_strings: Vec<String> = permissions.iter().map(|p| p.to_string()).collect();

        // Log permission grant
        let _ = server
            .audit_logger
            .log_permission_granted(
                actor_user_id,
                actor_username,
                username.clone(),
                target_user_id.clone(),
                permission_strings,
                nexus_core::auth::AuditResult::Success,
            )
            .await;

        // Check if root should be disabled after granting Admin permission
        if is_admin_grant {
            drop(rbac); // Release lock before async call
            server.check_and_disable_root_if_needed().await;
        }

        Ok(Json(serde_json::json!({
            "message": format!("Granted permissions to user '{}'", username)
        })))
    } else {
        Err((
            StatusCode::NOT_FOUND,
            Json(serde_json::json!({
                "error": format!("User '{}' not found", username)
            })),
        ))
    }
}

/// Revoke permissions from a user
/// DELETE /auth/users/{username}/permissions/{permission}
pub async fn revoke_permission(
    State(server): State<Arc<NexusServer>>,
    Extension(auth_context): Extension<Option<AuthContext>>,
    Path((username, permission_str)): Path<(String, String)>,
) -> Result<Json<serde_json::Value>, (StatusCode, Json<serde_json::Value>)> {
    let mut rbac = server.rbac.write().await;

    let permission = match permission_str.to_uppercase().as_str() {
        "READ" => Permission::Read,
        "WRITE" => Permission::Write,
        "ADMIN" => Permission::Admin,
        "SUPER" => Permission::Super,
        "QUEUE" => Permission::Queue,
        "CHATROOM" => Permission::Chatroom,
        "REST" => Permission::Rest,
        _ => {
            return Err((
                StatusCode::BAD_REQUEST,
                Json(serde_json::json!({
                    "error": format!("Unknown permission: {}", permission_str)
                })),
            ));
        }
    };

    let users_list = rbac.list_users();
    let target_user = users_list.iter().find(|u| u.username == username);

    // Check if trying to modify root user
    if let Some(user) = target_user {
        if user.is_root {
            return Err((
                StatusCode::FORBIDDEN,
                Json(serde_json::json!({
                    "error": "Cannot modify root user permissions. Only root users can modify root users."
                })),
            ));
        }
    }

    let user_id = target_user.map(|u| u.id.clone());
    if let Some(user_id) = user_id {
        if let Some(user_mut) = rbac.get_user_mut(&user_id) {
            user_mut.remove_permission(&permission);

            // Extract actor info from auth context for audit logging
            let (actor_user_id, actor_username, _) = auth_context
                .as_ref()
                .map(|ctx| {
                    let api_key_id = Some(ctx.api_key.id.clone());
                    let user_id = ctx.api_key.user_id.clone();
                    let username = None; // Username not available in ApiKey
                    (user_id, username, api_key_id)
                })
                .unwrap_or((None, None, None));

            // Log permission revocation
            let _ = server
                .audit_logger
                .log_permission_revoked(
                    actor_user_id,
                    actor_username,
                    username.clone(),
                    user_id.clone(),
                    vec![permission_str.clone()],
                    nexus_core::auth::AuditResult::Success,
                )
                .await;

            Ok(Json(serde_json::json!({
                "message": format!("Revoked permission '{}' from user '{}'", permission_str, username)
            })))
        } else {
            Err((
                StatusCode::NOT_FOUND,
                Json(serde_json::json!({
                    "error": format!("User '{}' not found", username)
                })),
            ))
        }
    } else {
        Err((
            StatusCode::NOT_FOUND,
            Json(serde_json::json!({
                "error": format!("User '{}' not found", username)
            })),
        ))
    }
}

/// Get user permissions
/// GET /auth/users/{username}/permissions
pub async fn get_user_permissions(
    State(server): State<Arc<NexusServer>>,
    Path(username): Path<String>,
) -> Result<Json<serde_json::Value>, (StatusCode, Json<serde_json::Value>)> {
    let rbac = server.rbac.read().await;
    let users = rbac.list_users();

    let user = users.iter().find(|u| u.username == username);

    if let Some(user) = user {
        let permissions: Vec<String> = user
            .additional_permissions
            .permissions()
            .iter()
            .map(|p| p.to_string())
            .collect();

        Ok(Json(serde_json::json!({
            "username": username,
            "permissions": permissions
        })))
    } else {
        Err((
            StatusCode::NOT_FOUND,
            Json(serde_json::json!({
                "error": format!("User '{}' not found", username)
            })),
        ))
    }
}

// ============================================================================
// JWT Authentication Endpoints
// ============================================================================

/// Login request
#[derive(Debug, Deserialize)]
pub struct LoginRequest {
    /// Username
    pub username: String,
    /// Password
    pub password: String,
}

/// Refresh token request
#[derive(Debug, Deserialize)]
pub struct RefreshTokenRequest {
    /// Refresh token
    pub refresh_token: String,
}

/// Login response
#[derive(Debug, Serialize)]
pub struct LoginResponse {
    /// Access token
    pub access_token: String,
    /// Refresh token
    pub refresh_token: String,
    /// Token type (always "Bearer")
    pub token_type: String,
    /// Expiration time in seconds
    pub expires_in: u64,
}

/// Refresh token response
#[derive(Debug, Serialize)]
pub struct RefreshTokenResponse {
    /// New access token
    pub access_token: String,
    /// Token type (always "Bearer")
    pub token_type: String,
    /// Expiration time in seconds
    pub expires_in: u64,
}

/// Authenticate user and return JWT tokens
/// POST /auth/login
pub async fn login(
    State(server): State<Arc<NexusServer>>,
    Json(request): Json<LoginRequest>,
) -> Result<Json<LoginResponse>, (StatusCode, Json<serde_json::Value>)> {
    // Find user by username
    let rbac = server.rbac.read().await;
    let users = rbac.list_users();
    let user = users.iter().find(|u| u.username == request.username);

    if let Some(user) = user {
        // Check if user is active
        if !user.is_active {
            // Log authentication failure - user disabled
            let _ = server
                .audit_logger
                .log_authentication_failed(
                    Some(request.username.clone()),
                    "User account is disabled".to_string(),
                    None,
                )
                .await;

            return Err((
                StatusCode::FORBIDDEN,
                Json(serde_json::json!({
                    "error": "User account is disabled"
                })),
            ));
        }

        // Verify password
        if let Some(ref password_hash) = user.password_hash {
            if !verify_password(&request.password, password_hash) {
                // Log authentication failure - invalid password
                let _ = server
                    .audit_logger
                    .log_authentication_failed(
                        Some(request.username.clone()),
                        "Invalid password".to_string(),
                        None,
                    )
                    .await;

                return Err((
                    StatusCode::UNAUTHORIZED,
                    Json(serde_json::json!({
                        "error": "Invalid username or password"
                    })),
                ));
            }
        } else {
            // User has no password set
            // Log authentication failure - no password set
            let _ = server
                .audit_logger
                .log_authentication_failed(
                    Some(request.username.clone()),
                    "User has no password set".to_string(),
                    None,
                )
                .await;

            return Err((
                StatusCode::UNAUTHORIZED,
                Json(serde_json::json!({
                    "error": "User has no password set"
                })),
            ));
        }

        // Generate JWT tokens
        match server.jwt_manager.generate_token_pair(user) {
            Ok(token_pair) => {
                // Log successful authentication
                let _ = server
                    .audit_logger
                    .log_authentication_success(
                        user.username.clone(),
                        user.id.clone(),
                        "password".to_string(),
                    )
                    .await;

                Ok(Json(LoginResponse {
                    access_token: token_pair.access_token,
                    refresh_token: token_pair.refresh_token,
                    token_type: token_pair.token_type,
                    expires_in: token_pair.expires_in,
                }))
            }
            Err(e) => {
                // Log authentication failure - token generation error
                let _ = server
                    .audit_logger
                    .log_authentication_failed(
                        Some(request.username.clone()),
                        format!("Failed to generate tokens: {}", e),
                        None,
                    )
                    .await;

                Err((
                    StatusCode::INTERNAL_SERVER_ERROR,
                    Json(serde_json::json!({
                        "error": format!("Failed to generate tokens: {}", e)
                    })),
                ))
            }
        }
    } else {
        // Log authentication failure - user not found
        let _ = server
            .audit_logger
            .log_authentication_failed(
                Some(request.username.clone()),
                "User not found".to_string(),
                None,
            )
            .await;

        Err((
            StatusCode::UNAUTHORIZED,
            Json(serde_json::json!({
                "error": "Invalid username or password"
            })),
        ))
    }
}

/// Refresh access token using refresh token
/// POST /auth/refresh
pub async fn refresh_token(
    State(server): State<Arc<NexusServer>>,
    Json(request): Json<RefreshTokenRequest>,
) -> Result<Json<RefreshTokenResponse>, (StatusCode, Json<serde_json::Value>)> {
    // Validate refresh token and extract user ID
    let user_id = match server
        .jwt_manager
        .validate_refresh_token(&request.refresh_token)
    {
        Ok(id) => id,
        Err(_) => {
            return Err((
                StatusCode::UNAUTHORIZED,
                Json(serde_json::json!({
                    "error": "Invalid or expired refresh token"
                })),
            ));
        }
    };

    // Find user by ID
    let rbac = server.rbac.read().await;
    let users = rbac.list_users();
    let user = users.iter().find(|u| u.id == user_id);

    if let Some(user) = user {
        // Check if user is still active
        if !user.is_active {
            return Err((
                StatusCode::FORBIDDEN,
                Json(serde_json::json!({
                    "error": "User account is disabled"
                })),
            ));
        }

        // Generate new access token
        match server
            .jwt_manager
            .refresh_access_token(&request.refresh_token, user)
        {
            Ok(access_token) => Ok(Json(RefreshTokenResponse {
                access_token,
                token_type: "Bearer".to_string(),
                expires_in: server.jwt_manager.expiration_seconds(),
            })),
            Err(e) => Err((
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({
                    "error": format!("Failed to refresh token: {}", e)
                })),
            )),
        }
    } else {
        Err((
            StatusCode::UNAUTHORIZED,
            Json(serde_json::json!({
                "error": "User not found"
            })),
        ))
    }
}

// ============================================================================
// API Key Management Endpoints
// ============================================================================

/// Request to create an API key
#[derive(Debug, Deserialize)]
pub struct CreateApiKeyRequest {
    /// Key name
    pub name: String,
    /// Username (optional, to associate key with user)
    pub username: Option<String>,
    /// Permissions (optional, defaults to Read, Write)
    pub permissions: Option<Vec<String>>,
    /// Expiration duration (optional, e.g., "7d", "24h", "30m")
    pub expires_in: Option<String>,
}

/// Request to revoke an API key
#[derive(Debug, Deserialize)]
pub struct RevokeApiKeyRequest {
    /// Revocation reason (optional)
    pub reason: Option<String>,
}

/// API key response (without the actual key value for security)
#[derive(Debug, Serialize)]
pub struct ApiKeyResponse {
    /// Key ID
    pub id: String,
    /// Key name
    pub name: String,
    /// User ID (if associated with a user)
    pub user_id: Option<String>,
    /// Permissions
    pub permissions: Vec<String>,
    /// Created at (RFC3339)
    pub created_at: String,
    /// Expires at (RFC3339, if set)
    pub expires_at: Option<String>,
    /// Whether key is active
    pub is_active: bool,
    /// Whether key is revoked
    pub is_revoked: bool,
    /// Revocation reason (if revoked)
    pub revocation_reason: Option<String>,
}

/// API key creation response (includes the full key only once)
#[derive(Debug, Serialize)]
pub struct CreateApiKeyResponse {
    /// Key ID
    pub id: String,
    /// Key name
    pub name: String,
    /// Full API key (only shown once on creation)
    pub key: String,
    /// User ID (if associated with a user)
    pub user_id: Option<String>,
    /// Permissions
    pub permissions: Vec<String>,
    /// Created at (RFC3339)
    pub created_at: String,
    /// Expires at (RFC3339, if set)
    pub expires_at: Option<String>,
}

/// List of API keys response
#[derive(Debug, Serialize)]
pub struct ApiKeysResponse {
    /// List of API keys
    pub keys: Vec<ApiKeyResponse>,
}

/// Helper function to parse duration string (e.g., "7d", "24h", "30m")
fn parse_duration(duration_str: &str) -> Result<chrono::DateTime<chrono::Utc>, String> {
    use chrono::{Duration, Utc};

    let duration_str = duration_str.trim();
    if duration_str.is_empty() {
        return Err("Duration cannot be empty".to_string());
    }

    let (num_str, unit) = if let Some(pos) = duration_str
        .char_indices()
        .find(|(_, c)| c.is_alphabetic())
        .map(|(i, _)| i)
    {
        let (num, unit) = duration_str.split_at(pos);
        (num, unit)
    } else {
        return Err(format!("Invalid duration format: {}", duration_str));
    };

    let num: i64 = num_str
        .parse()
        .map_err(|_| format!("Invalid number in duration: {}", num_str))?;

    let duration = match unit.to_lowercase().as_str() {
        "s" | "sec" | "second" | "seconds" => Duration::seconds(num),
        "m" | "min" | "minute" | "minutes" => Duration::minutes(num),
        "h" | "hr" | "hour" | "hours" => Duration::hours(num),
        "d" | "day" | "days" => Duration::days(num),
        "w" | "week" | "weeks" => Duration::weeks(num),
        "mo" | "month" | "months" => Duration::days(num * 30), // Approximate
        "y" | "yr" | "year" | "years" => Duration::days(num * 365), // Approximate
        _ => return Err(format!("Unknown duration unit: {}", unit)),
    };

    Ok(Utc::now() + duration)
}

/// Create a new API key
/// POST /auth/keys
pub async fn create_api_key(
    State(server): State<Arc<NexusServer>>,
    Extension(auth_context): Extension<Option<AuthContext>>,
    Json(request): Json<CreateApiKeyRequest>,
) -> Result<Json<CreateApiKeyResponse>, (StatusCode, Json<serde_json::Value>)> {
    use nexus_core::auth::Permission;

    // Parse permissions
    let permissions: Result<Vec<Permission>, _> = request
        .permissions
        .as_ref()
        .map(|p| {
            p.iter()
                .map(|perm| match perm.to_uppercase().as_str() {
                    "READ" => Ok(Permission::Read),
                    "WRITE" => Ok(Permission::Write),
                    "ADMIN" => Ok(Permission::Admin),
                    "SUPER" => Ok(Permission::Super),
                    "QUEUE" => Ok(Permission::Queue),
                    "CHATROOM" => Ok(Permission::Chatroom),
                    "REST" => Ok(Permission::Rest),
                    _ => Err(format!("Unknown permission: {}", perm)),
                })
                .collect()
        })
        .unwrap_or_else(|| Ok(vec![]));

    let permissions = match permissions {
        Ok(p) => {
            if p.is_empty() {
                // Default permissions if none specified
                vec![Permission::Read, Permission::Write]
            } else {
                p
            }
        }
        Err(e) => {
            return Err((
                StatusCode::BAD_REQUEST,
                Json(serde_json::json!({ "error": e })),
            ));
        }
    };

    // Resolve user_id if username is provided
    let user_id = if let Some(ref username) = request.username {
        let rbac = server.rbac.read().await;
        let users_list = rbac.list_users();
        match users_list.iter().find(|u| u.username == *username) {
            Some(user) => Some(user.id.clone()),
            None => {
                return Err((
                    StatusCode::NOT_FOUND,
                    Json(serde_json::json!({
                        "error": format!("User '{}' not found", username)
                    })),
                ));
            }
        }
    } else {
        None
    };

    // Parse expiration if provided
    let expires_at = if let Some(ref duration_str) = request.expires_in {
        match parse_duration(duration_str) {
            Ok(dt) => Some(dt),
            Err(e) => {
                return Err((
                    StatusCode::BAD_REQUEST,
                    Json(serde_json::json!({ "error": e })),
                ));
            }
        }
    } else {
        None
    };

    // Generate API key
    let auth_manager = &server.auth_manager;
    let result = if let Some(user_id) = user_id {
        if let Some(expires_at) = expires_at {
            // User ID with expiration
            auth_manager.generate_api_key_for_user_with_expiration(
                request.name.clone(),
                user_id,
                permissions,
                expires_at,
            )
        } else {
            // User ID without expiration
            auth_manager.generate_api_key_for_user(request.name.clone(), user_id, permissions)
        }
    } else if let Some(expires_at) = expires_at {
        // Temporary key without user
        auth_manager.generate_temporary_api_key(request.name.clone(), permissions, expires_at)
    } else {
        // Regular key without user
        auth_manager.generate_api_key(request.name.clone(), permissions)
    };

    match result {
        Ok((api_key, full_key)) => {
            // Extract actor info from auth context for audit logging
            let (actor_user_id, actor_username, _) = auth_context
                .as_ref()
                .map(|ctx| {
                    let api_key_id = Some(ctx.api_key.id.clone());
                    let user_id = ctx.api_key.user_id.clone();
                    let username = None; // Username not available in ApiKey
                    (user_id, username, api_key_id)
                })
                .unwrap_or((None, None, None));

            // Log API key creation
            let _ = server
                .audit_logger
                .log_api_key_created(
                    actor_user_id,
                    actor_username,
                    api_key.id.clone(),
                    api_key.name.clone(),
                    api_key.user_id.clone(),
                    nexus_core::auth::AuditResult::Success,
                )
                .await;

            let permissions: Vec<String> =
                api_key.permissions.iter().map(|p| p.to_string()).collect();

            Ok(Json(CreateApiKeyResponse {
                id: api_key.id,
                name: api_key.name,
                key: full_key,
                user_id: api_key.user_id,
                permissions,
                created_at: api_key.created_at.to_rfc3339(),
                expires_at: api_key.expires_at.map(|dt| dt.to_rfc3339()),
            }))
        }
        Err(e) => Err((
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({
                "error": format!("Failed to create API key: {}", e)
            })),
        )),
    }
}

/// List all API keys
/// GET /auth/keys?username=...
pub async fn list_api_keys(
    State(server): State<Arc<NexusServer>>,
    Query(params): Query<HashMap<String, String>>,
) -> Result<Json<ApiKeysResponse>, (StatusCode, Json<serde_json::Value>)> {
    let auth_manager = &server.auth_manager;

    let api_keys = if let Some(username) = params.get("username") {
        // Get keys for specific user
        let rbac = server.rbac.read().await;
        let users_list = rbac.list_users();
        if let Some(user) = users_list.iter().find(|u| u.username == *username) {
            auth_manager.get_api_keys_for_user(&user.id)
        } else {
            return Err((
                StatusCode::NOT_FOUND,
                Json(serde_json::json!({
                    "error": format!("User '{}' not found", username)
                })),
            ));
        }
    } else {
        // Get all keys
        auth_manager.list_api_keys()
    };

    let key_responses: Vec<ApiKeyResponse> = api_keys
        .iter()
        .map(|api_key| {
            let permissions: Vec<String> =
                api_key.permissions.iter().map(|p| p.to_string()).collect();

            ApiKeyResponse {
                id: api_key.id.clone(),
                name: api_key.name.clone(),
                user_id: api_key.user_id.clone(),
                permissions,
                created_at: api_key.created_at.to_rfc3339(),
                expires_at: api_key.expires_at.map(|dt| dt.to_rfc3339()),
                is_active: api_key.is_active,
                is_revoked: api_key.is_revoked,
                revocation_reason: api_key.revocation_reason.clone(),
            }
        })
        .collect();

    Ok(Json(ApiKeysResponse {
        keys: key_responses,
    }))
}

/// Get a specific API key by ID
/// GET /auth/keys/{key_id}
pub async fn get_api_key(
    State(server): State<Arc<NexusServer>>,
    Path(key_id): Path<String>,
) -> Result<Json<ApiKeyResponse>, (StatusCode, Json<serde_json::Value>)> {
    let auth_manager = &server.auth_manager;

    if let Some(api_key) = auth_manager.get_api_key(&key_id) {
        let permissions: Vec<String> = api_key.permissions.iter().map(|p| p.to_string()).collect();

        Ok(Json(ApiKeyResponse {
            id: api_key.id,
            name: api_key.name,
            user_id: api_key.user_id,
            permissions,
            created_at: api_key.created_at.to_rfc3339(),
            expires_at: api_key.expires_at.map(|dt| dt.to_rfc3339()),
            is_active: api_key.is_active,
            is_revoked: api_key.is_revoked,
            revocation_reason: api_key.revocation_reason,
        }))
    } else {
        Err((
            StatusCode::NOT_FOUND,
            Json(serde_json::json!({
                "error": format!("API key '{}' not found", key_id)
            })),
        ))
    }
}

/// Delete an API key
/// DELETE /auth/keys/{key_id}
pub async fn delete_api_key(
    State(server): State<Arc<NexusServer>>,
    Extension(auth_context): Extension<Option<AuthContext>>,
    Path(key_id): Path<String>,
) -> Result<Json<serde_json::Value>, (StatusCode, Json<serde_json::Value>)> {
    let auth_manager = &server.auth_manager;

    if auth_manager.delete_api_key(&key_id) {
        // Extract actor info from auth context for audit logging
        let (actor_user_id, actor_username, _) = auth_context
            .as_ref()
            .map(|ctx| {
                let api_key_id = Some(ctx.api_key.id.clone());
                let user_id = ctx.api_key.user_id.clone();
                let username = None; // Username not available in ApiKey
                (user_id, username, api_key_id)
            })
            .unwrap_or((None, None, None));

        // Log API key deletion
        let _ = server
            .audit_logger
            .log_api_key_deleted(
                actor_user_id,
                actor_username,
                key_id.clone(),
                nexus_core::auth::AuditResult::Success,
            )
            .await;

        Ok(Json(serde_json::json!({
            "message": format!("API key '{}' deleted successfully", key_id)
        })))
    } else {
        Err((
            StatusCode::NOT_FOUND,
            Json(serde_json::json!({
                "error": format!("API key '{}' not found", key_id)
            })),
        ))
    }
}

/// Revoke an API key
/// POST /auth/keys/{key_id}/revoke
pub async fn revoke_api_key(
    State(server): State<Arc<NexusServer>>,
    Extension(auth_context): Extension<Option<AuthContext>>,
    Path(key_id): Path<String>,
    Json(request): Json<RevokeApiKeyRequest>,
) -> Result<Json<serde_json::Value>, (StatusCode, Json<serde_json::Value>)> {
    let auth_manager = &server.auth_manager;

    match auth_manager.revoke_api_key(&key_id, request.reason.clone()) {
        Ok(_) => {
            // Extract actor info from auth context for audit logging
            let (actor_user_id, actor_username, _) = auth_context
                .as_ref()
                .map(|ctx| {
                    let api_key_id = Some(ctx.api_key.id.clone());
                    let user_id = ctx.api_key.user_id.clone();
                    let username = None; // Username not available in ApiKey
                    (user_id, username, api_key_id)
                })
                .unwrap_or((None, None, None));

            // Log API key revocation
            let _ = server
                .audit_logger
                .log_api_key_revoked(
                    actor_user_id,
                    actor_username,
                    key_id.clone(),
                    request.reason.clone(),
                    nexus_core::auth::AuditResult::Success,
                )
                .await;

            Ok(Json(serde_json::json!({
                "message": format!("API key '{}' revoked successfully", key_id)
            })))
        }
        Err(e) => Err((
            StatusCode::NOT_FOUND,
            Json(serde_json::json!({
                "error": format!("Failed to revoke API key: {}", e)
            })),
        )),
    }
}
