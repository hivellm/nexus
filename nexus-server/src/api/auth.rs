//! Authentication and user management REST API endpoints

use crate::NexusServer;
use axum::extract::{Path, State};
use axum::http::StatusCode;
use axum::response::Json;
use nexus_core::auth::{Permission, User};
use serde::{Deserialize, Serialize};
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
        // Hash password with Argon2
        use argon2::password_hash::{SaltString, rand_core::OsRng};
        use argon2::{Argon2, PasswordHasher};

        let argon2 = Argon2::default();
        let salt = SaltString::generate(&mut OsRng);
        let password_hash = argon2
            .hash_password(password.as_bytes(), &salt)
            .map_err(|e| {
                (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    Json(serde_json::json!({
                        "error": format!("Failed to hash password: {}", e)
                    })),
                )
            })?;

        let mut user = User::with_password_hash(
            user_id.clone(),
            request.username.clone(),
            password_hash.to_string(),
        );
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
    let user_id = users_list
        .iter()
        .find(|u| u.username == username)
        .map(|u| u.id.clone());

    if let Some(user_id) = user_id {
        if let Some(user_mut) = rbac.get_user_mut(&user_id) {
            for perm in &permissions {
                user_mut.add_permission(perm.clone());
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
    Path((username, permission_str)): Path<(String, String)>,
) -> Result<Json<serde_json::Value>, (StatusCode, Json<serde_json::Value>)> {
    let mut rbac = server.rbac.write().await;

    let permission = match permission_str.to_uppercase().as_str() {
        "READ" => Permission::Read,
        "WRITE" => Permission::Write,
        "ADMIN" => Permission::Admin,
        "SUPER" => Permission::Super,
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
    let user_id = users_list
        .iter()
        .find(|u| u.username == username)
        .map(|u| u.id.clone());

    if let Some(user_id) = user_id {
        if let Some(user_mut) = rbac.get_user_mut(&user_id) {
            user_mut.remove_permission(&permission);
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
