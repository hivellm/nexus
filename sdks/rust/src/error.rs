//! Error types for Nexus SDK

use thiserror::Error;

/// Result type alias for Nexus SDK operations
pub type Result<T> = std::result::Result<T, NexusError>;

/// Errors that can occur when using the Nexus SDK
#[derive(Debug, Error)]
pub enum NexusError {
    /// HTTP request error
    #[error("HTTP error: {0}")]
    Http(#[from] reqwest::Error),

    /// JSON serialization/deserialization error
    #[error("JSON error: {0}")]
    Json(#[from] serde_json::Error),

    /// Invalid URL
    #[error("Invalid URL: {0}")]
    Url(#[from] url::ParseError),

    /// API error response
    #[error("API error: {message} (status: {status})")]
    Api {
        /// Error message from API
        message: String,
        /// HTTP status code
        status: u16,
    },

    /// Authentication error
    #[error("Authentication failed: {0}")]
    Authentication(String),

    /// Invalid configuration
    #[error("Invalid configuration: {0}")]
    Configuration(String),

    /// Connection error
    #[error("Connection error: {0}")]
    Connection(String),

    /// Network error
    #[error("Network error: {0}")]
    Network(String),

    /// Timeout error
    #[error("Request timeout")]
    Timeout,

    /// Invalid response format
    #[error("Invalid response format: {0}")]
    InvalidResponse(String),

    /// Validation error
    #[error("Validation error: {0}")]
    Validation(String),
}
