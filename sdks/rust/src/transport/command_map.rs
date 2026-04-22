//! SDK dotted-name → wire-command mapping.
//!
//! The Rust SDK exposes methods like `client.execute_cypher(...)`,
//! `client.list_databases()`, `client.ping()`. The transport layer
//! translates those into the wire commands the server dispatcher
//! understands (`CYPHER`, `DB_LIST`, `PING`, ...).
//!
//! This module centralises that translation so:
//!
//! 1. Every method goes through the same indirection, making
//!    "add a new SDK method" a matter of one entry here plus the
//!    client wrapper.
//! 2. The table doubles as the canonical list for
//!    `docs/specs/sdk-transport.md` — cross-SDK consistency comes
//!    from eyeballing the mapping rather than from a shared runtime
//!    package.
//!
//! See `docs/specs/sdk-transport.md` §6 for the full contract.

use nexus_protocol::rpc::types::NexusValue;
use serde_json::Value;

use super::http::json_to_nexus;

/// Outcome of mapping a dotted name to a wire-level command.
pub struct CommandMapping {
    pub command: &'static str,
    pub args: Vec<NexusValue>,
}

/// Try to translate `dotted` into a wire command + argument vector.
/// Returns `None` for names this SDK does not yet route natively —
/// the client layer falls back to the HTTP transport in that case.
///
/// `payload` carries the method-specific JSON shape the caller hands
/// in. Some commands ignore it (`PING`, `STATS`), others require
/// specific fields (`CYPHER` needs `query` + optional `parameters`).
pub fn map_command(dotted: &str, payload: &Value) -> Option<CommandMapping> {
    match dotted {
        // ── Admin ────────────────────────────────────────────────
        "graph.cypher" => {
            let query = payload.get("query")?.as_str()?.to_string();
            let mut args = vec![NexusValue::Str(query)];
            if let Some(params) = payload.get("parameters")
                && !params.is_null()
            {
                args.push(json_to_nexus(params.clone()));
            }
            Some(CommandMapping {
                command: "CYPHER",
                args,
            })
        }
        "graph.ping" => Some(CommandMapping {
            command: "PING",
            args: vec![],
        }),
        "graph.hello" => Some(CommandMapping {
            command: "HELLO",
            args: vec![NexusValue::Int(1)],
        }),
        "graph.stats" => Some(CommandMapping {
            command: "STATS",
            args: vec![],
        }),
        "graph.health" => Some(CommandMapping {
            command: "HEALTH",
            args: vec![],
        }),
        "graph.quit" => Some(CommandMapping {
            command: "QUIT",
            args: vec![],
        }),
        "auth.login" => {
            if let Some(key) = payload.get("api_key").and_then(|v| v.as_str()) {
                return Some(CommandMapping {
                    command: "AUTH",
                    args: vec![NexusValue::Str(key.to_string())],
                });
            }
            let user = payload.get("username")?.as_str()?.to_string();
            let pass = payload.get("password")?.as_str()?.to_string();
            Some(CommandMapping {
                command: "AUTH",
                args: vec![NexusValue::Str(user), NexusValue::Str(pass)],
            })
        }

        // ── Database management ──────────────────────────────────
        "db.list" => Some(CommandMapping {
            command: "DB_LIST",
            args: vec![],
        }),
        "db.create" => Some(CommandMapping {
            command: "DB_CREATE",
            args: vec![NexusValue::Str(payload.get("name")?.as_str()?.to_string())],
        }),
        "db.drop" => Some(CommandMapping {
            command: "DB_DROP",
            args: vec![NexusValue::Str(payload.get("name")?.as_str()?.to_string())],
        }),
        "db.use" => Some(CommandMapping {
            command: "DB_USE",
            args: vec![NexusValue::Str(payload.get("name")?.as_str()?.to_string())],
        }),

        // ── Schema inspection ────────────────────────────────────
        "schema.labels" => Some(CommandMapping {
            command: "LABELS",
            args: vec![],
        }),
        "schema.rel_types" => Some(CommandMapping {
            command: "REL_TYPES",
            args: vec![],
        }),
        "schema.property_keys" => Some(CommandMapping {
            command: "PROPERTY_KEYS",
            args: vec![],
        }),
        "schema.indexes" => Some(CommandMapping {
            command: "INDEXES",
            args: vec![],
        }),

        // ── Data import/export ───────────────────────────────────
        "data.export" => {
            let format = payload.get("format")?.as_str()?.to_string();
            let mut args = vec![NexusValue::Str(format)];
            if let Some(query) = payload.get("query").and_then(|v| v.as_str()) {
                args.push(NexusValue::Str(query.to_string()));
            }
            Some(CommandMapping {
                command: "EXPORT",
                args,
            })
        }
        "data.import" => {
            let format = payload.get("format")?.as_str()?.to_string();
            let data = payload.get("data")?.as_str()?.to_string();
            Some(CommandMapping {
                command: "IMPORT",
                args: vec![NexusValue::Str(format), NexusValue::Str(data)],
            })
        }

        // ── Unmapped ─────────────────────────────────────────────
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn cypher_simple_query_maps_to_cypher_verb() {
        let m = map_command("graph.cypher", &json!({"query": "RETURN 1"})).unwrap();
        assert_eq!(m.command, "CYPHER");
        assert_eq!(m.args.len(), 1);
        assert_eq!(m.args[0].as_str(), Some("RETURN 1"));
    }

    #[test]
    fn cypher_with_params_appends_map() {
        let m = map_command(
            "graph.cypher",
            &json!({"query": "MATCH (n {name:$n}) RETURN n", "parameters": {"n": "Alice"}}),
        )
        .unwrap();
        assert_eq!(m.command, "CYPHER");
        assert_eq!(m.args.len(), 2);
        assert!(matches!(m.args[1], NexusValue::Map(_)));
    }

    #[test]
    fn ping_stats_health_take_no_args() {
        for name in ["graph.ping", "graph.stats", "graph.health", "graph.quit"] {
            let m = map_command(name, &json!({})).unwrap();
            assert!(m.args.is_empty(), "{name} must not produce args");
        }
    }

    #[test]
    fn db_create_requires_name() {
        assert!(map_command("db.create", &json!({})).is_none());
        let m = map_command("db.create", &json!({"name": "mydb"})).unwrap();
        assert_eq!(m.command, "DB_CREATE");
        assert_eq!(m.args[0].as_str(), Some("mydb"));
    }

    #[test]
    fn auth_api_key_takes_precedence_over_user_pass() {
        let m = map_command(
            "auth.login",
            &json!({"api_key": "nx_1", "username": "u", "password": "p"}),
        )
        .unwrap();
        assert_eq!(m.args.len(), 1);
        assert_eq!(m.args[0].as_str(), Some("nx_1"));
    }

    #[test]
    fn auth_falls_back_to_user_pass() {
        let m = map_command("auth.login", &json!({"username": "u", "password": "p"})).unwrap();
        assert_eq!(m.args.len(), 2);
    }

    #[test]
    fn data_export_default_no_query() {
        let m = map_command("data.export", &json!({"format": "json"})).unwrap();
        assert_eq!(m.command, "EXPORT");
        assert_eq!(m.args.len(), 1);
    }

    #[test]
    fn data_export_with_custom_query() {
        let m = map_command(
            "data.export",
            &json!({"format": "csv", "query": "MATCH (n:Person) RETURN n"}),
        )
        .unwrap();
        assert_eq!(m.args.len(), 2);
    }

    #[test]
    fn data_import_requires_format_and_data() {
        assert!(map_command("data.import", &json!({"format": "json"})).is_none());
        assert!(map_command("data.import", &json!({"data": "[]"})).is_none());
        let m = map_command("data.import", &json!({"format": "json", "data": "[]"})).unwrap();
        assert_eq!(m.command, "IMPORT");
        assert_eq!(m.args.len(), 2);
    }

    #[test]
    fn unknown_dotted_name_returns_none() {
        assert!(map_command("graph.nonsense", &json!({})).is_none());
    }
}
