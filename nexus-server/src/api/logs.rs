//! Server logs API endpoint

use axum::response::{IntoResponse, Json, Response};
use serde::Serialize;
use std::fs;
use std::path::PathBuf;

/// Log entry
#[derive(Debug, Serialize)]
pub struct LogEntry {
    pub timestamp: String,
    pub level: String,
    pub message: String,
}

/// List logs response
#[derive(Debug, Serialize)]
pub struct LogsResponse {
    pub logs: Vec<LogEntry>,
    pub total: usize,
}

/// Get server logs
pub async fn get_logs() -> Response {
    // Try to read from common log locations
    let log_paths = vec![
        PathBuf::from("./logs/nexus.log"),
        PathBuf::from("./nexus.log"),
        PathBuf::from("./logs/server.log"),
    ];

    let mut logs = Vec::new();

    for log_path in log_paths {
        if log_path.exists() {
            if let Ok(content) = fs::read_to_string(&log_path) {
                // Parse log lines (simple implementation)
                for line in content.lines().rev().take(1000) {
                    // Simple log parsing - adjust based on your log format
                    if line.trim().is_empty() {
                        continue;
                    }

                    // Try to extract timestamp and level
                    let parts: Vec<&str> = line.splitn(3, ' ').collect();
                    let (timestamp, level, message) = if parts.len() >= 3 {
                        (
                            parts[0].to_string(),
                            parts[1].to_string(),
                            parts[2..].join(" "),
                        )
                    } else {
                        ("".to_string(), "INFO".to_string(), line.to_string())
                    };

                    logs.push(LogEntry {
                        timestamp,
                        level,
                        message,
                    });
                }
                break;
            }
        }
    }

    // If no logs found, return empty list
    if logs.is_empty() {
        logs.push(LogEntry {
            timestamp: "".to_string(),
            level: "INFO".to_string(),
            message: "No log file found".to_string(),
        });
    }

    Json(LogsResponse {
        total: logs.len(),
        logs,
    })
    .into_response()
}
