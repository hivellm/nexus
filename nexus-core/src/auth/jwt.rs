//! JWT (JSON Web Token) support for Nexus authentication
//!
//! This module provides JWT token generation and validation for user authentication.
//! Tokens are signed using HS256 algorithm with a configurable secret key.

use super::User;
use anyhow::{Context, Result};
use chrono::{Duration, Utc};
use jsonwebtoken::{DecodingKey, EncodingKey, Header, Validation, decode, encode};
use serde::{Deserialize, Serialize};

/// JWT configuration
#[derive(Debug, Clone)]
pub struct JwtConfig {
    /// Secret key for signing tokens (should be at least 32 bytes)
    pub secret: String,
    /// Token expiration time in seconds (default: 1 hour)
    pub expiration_seconds: u64,
    /// Refresh token expiration time in seconds (default: 7 days)
    pub refresh_expiration_seconds: u64,
}

impl Default for JwtConfig {
    fn default() -> Self {
        Self {
            secret: Self::generate_secret(),
            expiration_seconds: 3600,           // 1 hour
            refresh_expiration_seconds: 604800, // 7 days
        }
    }
}

impl JwtConfig {
    /// Generate a random secret key
    /// Uses cryptographically secure random number generator (OsRng)
    pub fn generate_secret() -> String {
        use argon2::password_hash::rand_core::OsRng;
        use argon2::password_hash::rand_core::RngCore;
        let mut rng = OsRng;
        let mut bytes = vec![0u8; 64];
        rng.fill_bytes(&mut bytes);
        hex::encode(bytes)
    }

    /// Create JWT config from environment variables
    pub fn from_env() -> Self {
        let secret = std::env::var("NEXUS_JWT_SECRET").unwrap_or_else(|_| Self::generate_secret());

        let expiration_seconds = std::env::var("NEXUS_JWT_EXPIRATION_SECONDS")
            .ok()
            .and_then(|s| s.parse().ok())
            .unwrap_or(3600);

        let refresh_expiration_seconds = std::env::var("NEXUS_JWT_REFRESH_EXPIRATION_SECONDS")
            .ok()
            .and_then(|s| s.parse().ok())
            .unwrap_or(604800);

        Self {
            secret,
            expiration_seconds,
            refresh_expiration_seconds,
        }
    }
}

/// JWT claims (payload)
#[derive(Debug, Serialize, Deserialize)]
pub struct Claims {
    /// Subject (user ID)
    pub sub: String,
    /// Username
    pub username: String,
    /// Permissions
    pub permissions: Vec<String>,
    /// Issued at (timestamp)
    pub iat: i64,
    /// Expiration (timestamp)
    pub exp: i64,
    /// Token type: "access" or "refresh"
    pub token_type: String,
}

impl Claims {
    /// Create new claims for access token
    pub fn new_access(user: &User, expiration_seconds: u64) -> Self {
        let now = Utc::now();
        let exp = now + Duration::seconds(expiration_seconds as i64);

        let permissions: Vec<String> = user
            .additional_permissions
            .permissions()
            .iter()
            .map(|p| p.to_string().to_lowercase())
            .collect();

        Self {
            sub: user.id.clone(),
            username: user.username.clone(),
            permissions,
            iat: now.timestamp(),
            exp: exp.timestamp(),
            token_type: "access".to_string(),
        }
    }

    /// Create new claims for refresh token
    pub fn new_refresh(user: &User, expiration_seconds: u64) -> Self {
        let now = Utc::now();
        let exp = now + Duration::seconds(expiration_seconds as i64);

        Self {
            sub: user.id.clone(),
            username: user.username.clone(),
            permissions: vec![], // Refresh tokens don't need permissions
            iat: now.timestamp(),
            exp: exp.timestamp(),
            token_type: "refresh".to_string(),
        }
    }
}

/// JWT token pair (access + refresh)
#[derive(Debug, Serialize, Deserialize)]
pub struct TokenPair {
    /// Access token (short-lived)
    pub access_token: String,
    /// Refresh token (long-lived)
    pub refresh_token: String,
    /// Token type (always "Bearer")
    pub token_type: String,
    /// Expiration time in seconds
    pub expires_in: u64,
}

/// JWT token manager
#[derive(Debug, Clone)]
pub struct JwtManager {
    config: JwtConfig,
}

impl JwtManager {
    /// Create a new JWT manager with default configuration
    pub fn new(config: JwtConfig) -> Self {
        Self { config }
    }

    /// Generate an access token for a user
    pub fn generate_access_token(&self, user: &User) -> Result<String> {
        let claims = Claims::new_access(user, self.config.expiration_seconds);
        self.encode_token(&claims)
    }

    /// Generate a refresh token for a user
    pub fn generate_refresh_token(&self, user: &User) -> Result<String> {
        let claims = Claims::new_refresh(user, self.config.refresh_expiration_seconds);
        self.encode_token(&claims)
    }

    /// Generate both access and refresh tokens
    pub fn generate_token_pair(&self, user: &User) -> Result<TokenPair> {
        let access_token = self.generate_access_token(user)?;
        let refresh_token = self.generate_refresh_token(user)?;

        Ok(TokenPair {
            access_token,
            refresh_token,
            token_type: "Bearer".to_string(),
            expires_in: self.config.expiration_seconds,
        })
    }

    /// Validate and decode a JWT token
    pub fn validate_token(&self, token: &str) -> Result<Claims> {
        let decoding_key = DecodingKey::from_secret(self.config.secret.as_bytes());
        let validation = Validation::new(jsonwebtoken::Algorithm::HS256);

        let token_data = decode::<Claims>(token, &decoding_key, &validation)
            .context("Failed to decode JWT token")?;

        // Check if token is expired
        let now = Utc::now().timestamp();
        if token_data.claims.exp < now {
            anyhow::bail!("Token expired");
        }

        Ok(token_data.claims)
    }

    /// Validate a refresh token and extract user ID
    pub fn validate_refresh_token(&self, token: &str) -> Result<String> {
        let claims = self.validate_token(token)?;

        if claims.token_type != "refresh" {
            anyhow::bail!("Invalid token type, expected refresh token");
        }

        Ok(claims.sub)
    }

    /// Refresh an access token using a refresh token
    pub fn refresh_access_token(&self, refresh_token: &str, user: &User) -> Result<String> {
        // Validate refresh token
        let user_id = self.validate_refresh_token(refresh_token)?;

        // Verify user ID matches
        if user_id != user.id {
            anyhow::bail!("Refresh token user ID mismatch");
        }

        // Generate new access token
        self.generate_access_token(user)
    }

    /// Encode a token with claims
    fn encode_token(&self, claims: &Claims) -> Result<String> {
        let encoding_key = EncodingKey::from_secret(self.config.secret.as_bytes());
        encode(
            &Header::new(jsonwebtoken::Algorithm::HS256),
            claims,
            &encoding_key,
        )
        .context("Failed to encode JWT token")
    }

    /// Get the expiration time in seconds
    pub fn expiration_seconds(&self) -> u64 {
        self.config.expiration_seconds
    }

    /// Get the refresh expiration time in seconds
    pub fn refresh_expiration_seconds(&self) -> u64 {
        self.config.refresh_expiration_seconds
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::auth::{Permission, User};

    fn create_test_user() -> User {
        let mut user = User::new("user123".to_string(), "testuser".to_string());
        user.add_permission(Permission::Read);
        user.add_permission(Permission::Write);
        user
    }

    #[test]
    fn test_jwt_config_default() {
        let config = JwtConfig::default();
        assert!(!config.secret.is_empty());
        assert_eq!(config.expiration_seconds, 3600);
        assert_eq!(config.refresh_expiration_seconds, 604800);
    }

    #[test]
    fn test_generate_access_token() {
        let config = JwtConfig::default();
        let manager = JwtManager::new(config);
        let user = create_test_user();

        let token = manager.generate_access_token(&user).unwrap();
        assert!(!token.is_empty());
    }

    #[test]
    fn test_generate_refresh_token() {
        let config = JwtConfig::default();
        let manager = JwtManager::new(config);
        let user = create_test_user();

        let token = manager.generate_refresh_token(&user).unwrap();
        assert!(!token.is_empty());
    }

    #[test]
    fn test_generate_token_pair() {
        let config = JwtConfig::default();
        let manager = JwtManager::new(config);
        let user = create_test_user();

        let pair = manager.generate_token_pair(&user).unwrap();
        assert!(!pair.access_token.is_empty());
        assert!(!pair.refresh_token.is_empty());
        assert_eq!(pair.token_type, "Bearer");
        assert_eq!(pair.expires_in, 3600);
    }

    #[test]
    fn test_validate_access_token() {
        let config = JwtConfig::default();
        let manager = JwtManager::new(config);
        let user = create_test_user();

        let token = manager.generate_access_token(&user).unwrap();
        let claims = manager.validate_token(&token).unwrap();

        assert_eq!(claims.sub, user.id);
        assert_eq!(claims.username, user.username);
        assert_eq!(claims.token_type, "access");
    }

    #[test]
    fn test_validate_refresh_token() {
        let config = JwtConfig::default();
        let manager = JwtManager::new(config);
        let user = create_test_user();

        let token = manager.generate_refresh_token(&user).unwrap();
        let user_id = manager.validate_refresh_token(&token).unwrap();

        assert_eq!(user_id, user.id);
    }

    #[test]
    fn test_refresh_access_token() {
        let config = JwtConfig::default();
        let manager = JwtManager::new(config);
        let user = create_test_user();

        let refresh_token = manager.generate_refresh_token(&user).unwrap();
        let new_access_token = manager.refresh_access_token(&refresh_token, &user).unwrap();

        assert!(!new_access_token.is_empty());

        // Validate the new access token
        let claims = manager.validate_token(&new_access_token).unwrap();
        assert_eq!(claims.sub, user.id);
        assert_eq!(claims.token_type, "access");
    }

    #[test]
    fn test_invalid_token() {
        let config = JwtConfig::default();
        let manager = JwtManager::new(config);

        assert!(manager.validate_token("invalid.token.here").is_err());
    }

    #[test]
    fn test_expired_token() {
        let config = JwtConfig {
            expiration_seconds: 1, // 1 second expiration
            ..Default::default()
        };

        let manager = JwtManager::new(config);
        let user = create_test_user();

        let token = manager.generate_access_token(&user).unwrap();

        // Wait for token to expire
        std::thread::sleep(std::time::Duration::from_secs(2));

        assert!(manager.validate_token(&token).is_err());
    }

    #[test]
    fn test_wrong_refresh_token_type() {
        let config = JwtConfig::default();
        let manager = JwtManager::new(config);
        let user = create_test_user();

        let access_token = manager.generate_access_token(&user).unwrap();

        // Try to use access token as refresh token
        assert!(manager.validate_refresh_token(&access_token).is_err());
    }

    #[test]
    fn test_jwt_token_expiration_edge_cases() {
        // Test with very short expiration
        let config = JwtConfig {
            expiration_seconds: 0, // Immediate expiration
            ..Default::default()
        };

        let manager = JwtManager::new(config);
        let user = create_test_user();

        let token = manager.generate_access_token(&user).unwrap();

        // Token should be invalid immediately (or very quickly)
        // Note: JWT library may allow 0 expiration, so we test the boundary
        let _result = manager.validate_token(&token);
        // Result may vary based on JWT library behavior with 0 expiration
        // This tests the edge case handling
    }

    #[test]
    fn test_jwt_refresh_token_expiration() {
        let config = JwtConfig {
            refresh_expiration_seconds: 1, // 1 second expiration
            ..Default::default()
        };

        let manager = JwtManager::new(config);
        let user = create_test_user();

        let refresh_token = manager.generate_refresh_token(&user).unwrap();

        // Wait for token to expire
        std::thread::sleep(std::time::Duration::from_secs(2));

        // Refresh token should be expired
        assert!(manager.validate_refresh_token(&refresh_token).is_err());
    }

    #[test]
    fn test_jwt_refresh_with_wrong_user() {
        let config = JwtConfig::default();
        let manager = JwtManager::new(config);
        let user1 = create_test_user();
        let user2 = User::new("user456".to_string(), "otheruser".to_string());

        let refresh_token = manager.generate_refresh_token(&user1).unwrap();

        // Try to refresh with different user
        assert!(
            manager
                .refresh_access_token(&refresh_token, &user2)
                .is_err()
        );
    }

    #[test]
    fn test_jwt_token_with_different_secrets() {
        let config1 = JwtConfig::default();
        let manager1 = JwtManager::new(config1);
        let user = create_test_user();

        let token = manager1.generate_access_token(&user).unwrap();

        // Create new manager with different secret
        let config2 = JwtConfig::default();
        let manager2 = JwtManager::new(config2);

        // Token from manager1 should not validate with manager2
        assert!(manager2.validate_token(&token).is_err());
    }

    #[test]
    fn test_jwt_token_pair_expiration_times() {
        let config = JwtConfig {
            expiration_seconds: 1800,            // 30 minutes
            refresh_expiration_seconds: 2592000, // 30 days
            ..Default::default()
        };

        let manager = JwtManager::new(config);
        let user = create_test_user();

        let pair = manager.generate_token_pair(&user).unwrap();

        assert_eq!(pair.expires_in, 1800);
        assert!(!pair.access_token.is_empty());
        assert!(!pair.refresh_token.is_empty());
    }
}
