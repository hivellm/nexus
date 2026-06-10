//! `dbms.*` discovery and configuration procedures:
//! `dbms.components`, `dbms.procedures`, `dbms.functions`, `dbms.info`,
//! `dbms.listConfig`, `dbms.showCurrentUser`.
//! Also hosts the shared `current_rfc3339_utc` helper used by `db.info`.

use super::super::super::context::ExecutionContext;
use super::super::super::engine::Executor;
use super::super::super::types::Row;
use crate::Result;
use serde_json::Value;

impl Executor {
    // ─────────────────────────────────────────────────────────────────────
    // phase6_opencypher-system-procedures §6 — `dbms.*` discovery
    // ─────────────────────────────────────────────────────────────────────

    pub(in crate::executor) fn execute_dbms_components_procedure(
        &self,
        context: &mut ExecutionContext,
        yield_columns: Option<&Vec<String>>,
    ) -> Result<()> {
        let version = env!("CARGO_PKG_VERSION").to_string();
        let rows = vec![Row {
            values: vec![
                Value::String("Nexus Kernel".to_string()),
                Value::Array(vec![Value::String(version)]),
                Value::String("community".to_string()),
            ],
        }];
        let columns = if let Some(y) = yield_columns {
            y.clone()
        } else {
            vec![
                "name".to_string(),
                "versions".to_string(),
                "edition".to_string(),
            ]
        };
        context.set_columns_and_rows(columns, rows);
        Ok(())
    }

    pub(in crate::executor) fn execute_dbms_procedures_procedure(
        &self,
        context: &mut ExecutionContext,
        yield_columns: Option<&Vec<String>>,
    ) -> Result<()> {
        // Canonical procedure catalogue. Rows are generated deterministically
        // so `cypher-shell` autocomplete and Bloom's capability probe see a
        // stable ordering across calls.
        let entries: &[(&str, &str, &str, &str)] = &[
            (
                "db.labels",
                "db.labels() :: (label :: STRING)",
                "READ",
                "List all node labels in the current database.",
            ),
            (
                "db.relationshipTypes",
                "db.relationshipTypes() :: (relationshipType :: STRING)",
                "READ",
                "List all relationship types in the current database.",
            ),
            (
                "db.propertyKeys",
                "db.propertyKeys() :: (propertyKey :: STRING)",
                "READ",
                "List all property keys in the current database.",
            ),
            (
                "db.schema",
                "db.schema() :: (nodes :: LIST<MAP>, relationships :: LIST<MAP>)",
                "READ",
                "Return the schema graph of the current database.",
            ),
            (
                "db.indexes",
                "db.indexes() :: (id :: INTEGER, name :: STRING, state :: STRING, \
              populationPercent :: FLOAT, uniqueness :: STRING, type :: STRING, \
              entityType :: STRING, labelsOrTypes :: LIST<STRING>, properties :: LIST<STRING>, \
              indexProvider :: STRING)",
                "READ",
                "List all indexes in the current database.",
            ),
            (
                "db.indexDetails",
                "db.indexDetails(name :: STRING) :: (id :: INTEGER, name :: STRING, state :: STRING, \
              populationPercent :: FLOAT, uniqueness :: STRING, type :: STRING, \
              entityType :: STRING, labelsOrTypes :: LIST<STRING>, properties :: LIST<STRING>, \
              indexProvider :: STRING)",
                "READ",
                "Return detail for a single named index.",
            ),
            (
                "db.constraints",
                "db.constraints() :: (id :: INTEGER, name :: STRING, type :: STRING, \
              entityType :: STRING, labelsOrTypes :: LIST<STRING>, properties :: LIST<STRING>, \
              ownedIndex :: STRING)",
                "READ",
                "List all constraints in the current database.",
            ),
            (
                "db.info",
                "db.info() :: (id :: STRING, name :: STRING, creationDate :: STRING)",
                "READ",
                "Return metadata for the current database.",
            ),
            (
                "dbms.components",
                "dbms.components() :: (name :: STRING, versions :: LIST<STRING>, edition :: STRING)",
                "DBMS",
                "List the server's component versions.",
            ),
            (
                "dbms.procedures",
                "dbms.procedures() :: (name :: STRING, signature :: STRING, description :: STRING, \
              mode :: STRING, worksOnSystem :: BOOLEAN)",
                "DBMS",
                "List all procedures registered on the server.",
            ),
            (
                "dbms.functions",
                "dbms.functions() :: (name :: STRING, signature :: STRING, description :: STRING, \
              aggregating :: BOOLEAN)",
                "DBMS",
                "List all functions registered on the server.",
            ),
            (
                "dbms.info",
                "dbms.info() :: (id :: STRING, name :: STRING, creationDate :: STRING)",
                "DBMS",
                "Return the server's identity and boot time.",
            ),
            (
                "dbms.listConfig",
                "dbms.listConfig(search :: STRING) :: (name :: STRING, description :: STRING, \
              value :: STRING, dynamic :: BOOLEAN)",
                "DBMS",
                "List configuration keys matching a substring (Admin only).",
            ),
            (
                "dbms.showCurrentUser",
                "dbms.showCurrentUser() :: (username :: STRING, roles :: LIST<STRING>, \
              flags :: LIST<STRING>)",
                "DBMS",
                "Return the caller's identity and roles.",
            ),
            // phase6_opencypher-fulltext-search — Neo4j-compatible surface.
            (
                "db.index.fulltext.createNodeIndex",
                "db.index.fulltext.createNodeIndex(name :: STRING, labels :: LIST<STRING>, \
              properties :: LIST<STRING>, config :: MAP?) :: (name :: STRING, state :: STRING)",
                "SCHEMA",
                "Register a node-scope full-text index.",
            ),
            (
                "db.index.fulltext.createRelationshipIndex",
                "db.index.fulltext.createRelationshipIndex(name :: STRING, types :: LIST<STRING>, \
              properties :: LIST<STRING>, config :: MAP?) :: (name :: STRING, state :: STRING)",
                "SCHEMA",
                "Register a relationship-scope full-text index.",
            ),
            (
                "db.index.fulltext.queryNodes",
                "db.index.fulltext.queryNodes(name :: STRING, query :: STRING) :: \
              (node :: NODE, score :: FLOAT)",
                "READ",
                "Run a BM25 query against a node full-text index.",
            ),
            (
                "db.index.fulltext.queryRelationships",
                "db.index.fulltext.queryRelationships(name :: STRING, query :: STRING) :: \
              (relationship :: RELATIONSHIP, score :: FLOAT)",
                "READ",
                "Run a BM25 query against a relationship full-text index.",
            ),
            (
                "db.index.fulltext.drop",
                "db.index.fulltext.drop(name :: STRING) :: (name :: STRING, state :: STRING)",
                "SCHEMA",
                "Drop a full-text index and remove its directory.",
            ),
            (
                "db.index.fulltext.awaitEventuallyConsistentIndexRefresh",
                "db.index.fulltext.awaitEventuallyConsistentIndexRefresh() :: (status :: STRING)",
                "READ",
                "Block until every FTS index has refreshed at least once.",
            ),
            (
                "db.index.fulltext.listAvailableAnalyzers",
                "db.index.fulltext.listAvailableAnalyzers() :: (name :: STRING, description :: STRING)",
                "READ",
                "List analyzers accepted by the FTS config.analyzer option.",
            ),
        ];
        let mut rows: Vec<Row> = entries
            .iter()
            .map(|(name, sig, mode, desc)| Row {
                values: vec![
                    Value::String((*name).to_string()),
                    Value::String((*sig).to_string()),
                    Value::String((*desc).to_string()),
                    Value::String((*mode).to_string()),
                    Value::Bool(false),
                ],
            })
            .collect();

        // phase6_opencypher-apoc-ecosystem — append every apoc.*
        // procedure. Signatures are enumerated compactly; the full
        // per-procedure signature lives in `docs/procedures/
        // APOC_COMPATIBILITY.md`.
        for name in crate::apoc::list_procedures() {
            rows.push(Row {
                values: vec![
                    Value::String(name.to_string()),
                    Value::String(format!("{name}(...) :: ANY")),
                    Value::String("APOC-compatible procedure.".to_string()),
                    Value::String("READ".to_string()),
                    Value::Bool(false),
                ],
            });
        }

        // phase6_opencypher-geospatial-predicates §7 — append the
        // pure-value spatial.* surface plus the engine-aware
        // `spatial.nearest` so BI tools that introspect
        // `dbms.procedures()` see the full geo namespace.
        for name in crate::spatial::list_procedures() {
            rows.push(Row {
                values: vec![
                    Value::String(name.to_string()),
                    Value::String(format!("{name}(...) :: ANY")),
                    Value::String("Geospatial procedure.".to_string()),
                    Value::String("READ".to_string()),
                    Value::Bool(false),
                ],
            });
        }
        rows.push(Row {
            values: vec![
                Value::String("spatial.nearest".to_string()),
                Value::String(
                    "spatial.nearest(point :: POINT, label :: STRING, k :: INTEGER) :: \
                     (node :: NODE, dist :: FLOAT)"
                        .to_string(),
                ),
                Value::String("k nearest neighbours via the R-tree index for `label`.".to_string()),
                Value::String("READ".to_string()),
                Value::Bool(false),
            ],
        });

        let columns = if let Some(y) = yield_columns {
            y.clone()
        } else {
            vec![
                "name".to_string(),
                "signature".to_string(),
                "description".to_string(),
                "mode".to_string(),
                "worksOnSystem".to_string(),
            ]
        };
        context.set_columns_and_rows(columns, rows);
        Ok(())
    }

    pub(in crate::executor) fn execute_dbms_functions_procedure(
        &self,
        context: &mut ExecutionContext,
        yield_columns: Option<&Vec<String>>,
    ) -> Result<()> {
        // Canonical function catalogue matching the scalar / aggregation
        // surface the executor dispatches at runtime (see
        // `evaluate_projection_expression` in `eval/projection.rs`).
        let entries: &[(&str, &str, &str, bool)] = &[
            ("count", "count(x :: ANY) :: INTEGER", "Count rows.", true),
            (
                "sum",
                "sum(x :: NUMBER) :: NUMBER",
                "Sum numeric column.",
                true,
            ),
            (
                "avg",
                "avg(x :: NUMBER) :: FLOAT",
                "Average of numeric column.",
                true,
            ),
            ("min", "min(x :: ANY) :: ANY", "Minimum of column.", true),
            ("max", "max(x :: ANY) :: ANY", "Maximum of column.", true),
            (
                "collect",
                "collect(x :: ANY) :: LIST",
                "Collect column into a list.",
                true,
            ),
            (
                "stdev",
                "stdev(x :: NUMBER) :: FLOAT",
                "Sample standard deviation.",
                true,
            ),
            (
                "stdevp",
                "stdevp(x :: NUMBER) :: FLOAT",
                "Population standard deviation.",
                true,
            ),
            (
                "percentileCont",
                "percentileCont(x :: NUMBER, p :: FLOAT) :: FLOAT",
                "Continuous percentile.",
                true,
            ),
            (
                "percentileDisc",
                "percentileDisc(x :: NUMBER, p :: FLOAT) :: NUMBER",
                "Discrete percentile.",
                true,
            ),
            (
                "labels",
                "labels(n :: NODE) :: LIST<STRING>",
                "Labels of a node.",
                false,
            ),
            (
                "type",
                "type(r :: RELATIONSHIP) :: STRING",
                "Type of a relationship.",
                false,
            ),
            (
                "keys",
                "keys(x :: ANY) :: LIST<STRING>",
                "Property keys of a node / relationship / map.",
                false,
            ),
            (
                "id",
                "id(x :: NODE) :: INTEGER",
                "Internal id of a node / relationship.",
                false,
            ),
            (
                "size",
                "size(x :: ANY) :: INTEGER",
                "Length of a string or list.",
                false,
            ),
            (
                "length",
                "length(path :: PATH) :: INTEGER",
                "Number of relationships in a path.",
                false,
            ),
            (
                "toUpper",
                "toUpper(s :: STRING) :: STRING",
                "Uppercase string.",
                false,
            ),
            (
                "toLower",
                "toLower(s :: STRING) :: STRING",
                "Lowercase string.",
                false,
            ),
            (
                "substring",
                "substring(s :: STRING, start :: INTEGER, length :: INTEGER) :: STRING",
                "Substring of a string.",
                false,
            ),
            (
                "left",
                "left(s :: STRING, n :: INTEGER) :: STRING",
                "First n characters.",
                false,
            ),
            (
                "right",
                "right(s :: STRING, n :: INTEGER) :: STRING",
                "Last n characters.",
                false,
            ),
            (
                "toString",
                "toString(x :: ANY) :: STRING",
                "Convert to string.",
                false,
            ),
            (
                "toInteger",
                "toInteger(x :: ANY) :: INTEGER",
                "Convert to integer.",
                false,
            ),
            (
                "toFloat",
                "toFloat(x :: ANY) :: FLOAT",
                "Convert to float.",
                false,
            ),
            (
                "toBoolean",
                "toBoolean(x :: ANY) :: BOOLEAN",
                "Convert to boolean.",
                false,
            ),
            (
                "toIntegerList",
                "toIntegerList(xs :: LIST) :: LIST<INTEGER>",
                "Per-element integer coercion.",
                false,
            ),
            (
                "toFloatList",
                "toFloatList(xs :: LIST) :: LIST<FLOAT>",
                "Per-element float coercion.",
                false,
            ),
            (
                "toStringList",
                "toStringList(xs :: LIST) :: LIST<STRING>",
                "Per-element string coercion.",
                false,
            ),
            (
                "toBooleanList",
                "toBooleanList(xs :: LIST) :: LIST<BOOLEAN>",
                "Per-element boolean coercion.",
                false,
            ),
            (
                "isEmpty",
                "isEmpty(x :: ANY) :: BOOLEAN",
                "Empty string / list / map.",
                false,
            ),
            (
                "isInteger",
                "isInteger(x :: ANY) :: BOOLEAN",
                "Runtime type check.",
                false,
            ),
            (
                "isFloat",
                "isFloat(x :: ANY) :: BOOLEAN",
                "Runtime type check.",
                false,
            ),
            (
                "isString",
                "isString(x :: ANY) :: BOOLEAN",
                "Runtime type check.",
                false,
            ),
            (
                "isBoolean",
                "isBoolean(x :: ANY) :: BOOLEAN",
                "Runtime type check.",
                false,
            ),
            (
                "isList",
                "isList(x :: ANY) :: BOOLEAN",
                "Runtime type check.",
                false,
            ),
            (
                "isMap",
                "isMap(x :: ANY) :: BOOLEAN",
                "Runtime type check.",
                false,
            ),
            (
                "isNode",
                "isNode(x :: ANY) :: BOOLEAN",
                "Runtime type check.",
                false,
            ),
            (
                "isRelationship",
                "isRelationship(x :: ANY) :: BOOLEAN",
                "Runtime type check.",
                false,
            ),
            (
                "isPath",
                "isPath(x :: ANY) :: BOOLEAN",
                "Runtime type check.",
                false,
            ),
            (
                "exists",
                "exists(x :: ANY) :: BOOLEAN",
                "Property / expression presence.",
                false,
            ),
        ];
        let rows: Vec<Row> = entries
            .iter()
            .map(|(name, sig, desc, agg)| Row {
                values: vec![
                    Value::String((*name).to_string()),
                    Value::String((*sig).to_string()),
                    Value::String((*desc).to_string()),
                    Value::Bool(*agg),
                ],
            })
            .collect();
        let columns = if let Some(y) = yield_columns {
            y.clone()
        } else {
            vec![
                "name".to_string(),
                "signature".to_string(),
                "description".to_string(),
                "aggregating".to_string(),
            ]
        };
        context.set_columns_and_rows(columns, rows);
        Ok(())
    }

    pub(in crate::executor) fn execute_dbms_info_procedure(
        &self,
        context: &mut ExecutionContext,
        yield_columns: Option<&Vec<String>>,
    ) -> Result<()> {
        let rows = vec![Row {
            values: vec![
                Value::String("nexus-1".to_string()),
                Value::String("Nexus".to_string()),
                Value::String(Self::current_rfc3339_utc()),
            ],
        }];
        let columns = if let Some(y) = yield_columns {
            y.clone()
        } else {
            vec![
                "id".to_string(),
                "name".to_string(),
                "creationDate".to_string(),
            ]
        };
        context.set_columns_and_rows(columns, rows);
        Ok(())
    }

    pub(in crate::executor) fn execute_dbms_list_config_procedure(
        &self,
        context: &mut ExecutionContext,
        yield_columns: Option<&Vec<String>>,
        search: &str,
    ) -> Result<()> {
        // Sources configuration from the `NEXUS_*` environment variables —
        // these are the same keys the server consults during `Config::load`.
        // A full config-registry surface will ship with the config
        // refactor task; for now the env pass gives drivers the common
        // `server.*` keys Cypher Shell expects.
        let config: &[(&str, &str, &str)] = &[
            (
                "server.default_listen_address",
                "Default server HTTP bind address",
                "NEXUS_ADDR",
            ),
            (
                "server.default_rpc_address",
                "Default server RPC bind address",
                "NEXUS_RPC_ADDR",
            ),
            (
                "server.data_dir",
                "Directory for catalog + record stores + WAL",
                "NEXUS_DATA_DIR",
            ),
            (
                "server.rpc_enabled",
                "Whether RPC transport is active",
                "NEXUS_RPC_ENABLED",
            ),
            (
                "server.rpc_require_auth",
                "Whether RPC handshakes require AUTH",
                "NEXUS_RPC_REQUIRE_AUTH",
            ),
            (
                "server.auth_enabled",
                "HTTP authentication on/off",
                "NEXUS_AUTH_ENABLED",
            ),
            (
                "server.rpc_max_frame_bytes",
                "Maximum RPC frame size",
                "NEXUS_RPC_MAX_FRAME_BYTES",
            ),
            (
                "server.rpc_max_in_flight",
                "Concurrent in-flight RPC requests",
                "NEXUS_RPC_MAX_IN_FLIGHT",
            ),
            (
                "server.rpc_slow_threshold_ms",
                "Slow-query threshold in milliseconds",
                "NEXUS_RPC_SLOW_MS",
            ),
        ];
        let lower_search = search.to_lowercase();
        let rows: Vec<Row> = config
            .iter()
            .filter(|(name, _, _)| {
                lower_search.is_empty() || name.to_lowercase().contains(&lower_search)
            })
            .map(|(name, desc, env)| Row {
                values: vec![
                    Value::String((*name).to_string()),
                    Value::String((*desc).to_string()),
                    Value::String(std::env::var(*env).unwrap_or_else(|_| String::new())),
                    Value::Bool(false),
                ],
            })
            .collect();
        let columns = if let Some(y) = yield_columns {
            y.clone()
        } else {
            vec![
                "name".to_string(),
                "description".to_string(),
                "value".to_string(),
                "dynamic".to_string(),
            ]
        };
        context.set_columns_and_rows(columns, rows);
        Ok(())
    }

    pub(in crate::executor) fn execute_dbms_show_current_user_procedure(
        &self,
        context: &mut ExecutionContext,
        yield_columns: Option<&Vec<String>>,
    ) -> Result<()> {
        // The engine-level Executor has no direct auth-session handle — the
        // server's `/cypher` handler is where the AuthContext lives. When
        // called through the engine we surface a sentinel unauthenticated
        // row so tools like Cypher Shell don't break during startup
        // discovery; the server-side route will override this with the
        // real session identity in a follow-up.
        let rows = vec![Row {
            values: vec![
                Value::String("anonymous".to_string()),
                Value::Array(Vec::new()),
                Value::Array(Vec::new()),
            ],
        }];
        let columns = if let Some(y) = yield_columns {
            y.clone()
        } else {
            vec![
                "username".to_string(),
                "roles".to_string(),
                "flags".to_string(),
            ]
        };
        context.set_columns_and_rows(columns, rows);
        Ok(())
    }

    /// Shared helper — render the current UTC time as an RFC3339 string.
    /// Used by `db.info()` and `dbms.info()` so drivers can deserialise
    /// the column back into a DATETIME.
    pub(in crate::executor) fn current_rfc3339_utc() -> String {
        chrono::Utc::now().to_rfc3339()
    }
}
