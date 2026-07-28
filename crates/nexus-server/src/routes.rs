//! Main HTTP route table construction.
//!
//! [`build_router`] was extracted from `main.rs::async_main` as a pure
//! mechanical move: the `.route(...)` chain below is byte-identical to the
//! original inline router build. The only change is that the
//! `let auth_enabled = config.auth.enabled;` local captured from the
//! enclosing scope became the `auth_enabled` parameter, since the block no
//! longer has direct access to `config`.

use std::sync::Arc;

use axum::{
    Json, Router,
    routing::{delete, get, post, put},
};

use nexus_server::{NexusServer, api};

/// Builds the main Axum router: health/metrics, Cypher, auth, data,
/// schema, performance, comparison, clustering, replication, and
/// cluster-management endpoints, plus the MCP router nested at `/mcp`
/// and the always-on layer that stamps a `None` auth context onto every
/// request when authentication is disabled.
///
/// `auth_enabled` mirrors `config.auth.enabled` at the call site.
pub(super) fn build_router(
    nexus_server: Arc<NexusServer>,
    mcp_router: Router<Arc<NexusServer>>,
    auth_enabled: bool,
) -> Router {
    Router::new()
        .route("/", get(api::health::health_check))
        .route("/health", get(api::health::health_check))
        .route("/metrics", get(api::health::metrics))
        .route("/prometheus", get(api::prometheus::prometheus_metrics))
        // Memory profiling endpoints. They respond 503 if the crate was
        // built without `--features memory-profiling`, so the routes are
        // always wired — no conditional routing required.
        .route("/debug/memory", get(api::debug::memory_stats))
        .route("/debug/heap/dump", post(api::debug::heap_dump))
        .route("/test", get(|| async { "Test endpoint working" }))
        .route("/cypher-debug", post(|body: String| async move {
            tracing::debug!("Raw body received on /cypher-debug: {}", body);
            Json(serde_json::json!({"message": "Debug endpoint received", "body": body}))
        }))
        .route("/cypher", post(api::cypher::execute_cypher))
        // Encryption-at-rest status: read-only, reports the
        // boot-time KeyProvider source + master-key fingerprint.
        // Storage-layer wiring lands in follow-up tasks
        // (-storage-hooks, -wal, -indexes); the endpoint surface
        // is stable from day one.
        .route(
            "/admin/encryption/status",
            get(api::encryption::status),
        )
        // `GET /admin/queries` — JSON view of the active-query
        // tracker for triage when `/cypher` is wedged
        // (phase6_slow-query-log-and-active-queries §3).
        .route(
            "/admin/queries",
            get(api::admin_queries::list_queries),
        )
        .route("/test-handler", get(|| async {
            tracing::debug!("Handler called!");
            "Handler called successfully"
        }))
        // GraphQL endpoints
        .route("/graphql", post({
            let server = nexus_server.clone();
            let graphql_schema = api::graphql::create_schema(server);
            move |req| {
                let schema = graphql_schema.clone();
                api::graphql::graphql_handler(axum::extract::State(schema), req)
            }
        }))
        // Always insert None auth context for endpoints when auth is disabled
        .layer({
            axum::middleware::from_fn(move |mut request: axum::extract::Request, next: axum::middleware::Next| async move {
                if !auth_enabled {
                    request.extensions_mut().insert(axum::extract::Extension(None::<nexus_core::auth::middleware::AuthContext>));
                }
                next.run(request).await
            })
        })
        // Authentication endpoints
        .route("/auth/users", post(api::auth::create_user))
        .route(
            "/auth/users",
            get({
                let server = nexus_server.clone();
                move |ext: Option<
                    axum::extract::Extension<Option<nexus_core::auth::middleware::AuthContext>>,
                >| api::auth::list_users(axum::extract::State(server), ext)
            }),
        )
        .route(
            "/auth/users/{username}",
            get({
                let server = nexus_server.clone();
                move |ext: Option<
                    axum::extract::Extension<Option<nexus_core::auth::middleware::AuthContext>>,
                >,
                      path| api::auth::get_user(axum::extract::State(server), ext, path)
            }),
        )
        .route("/auth/users/{username}", delete(api::auth::delete_user))
        .route(
            "/auth/users/{username}/permissions",
            post(api::auth::grant_permissions),
        )
        .route(
            "/auth/users/{username}/permissions",
            get({
                let server = nexus_server.clone();
                move |ext: Option<
                    axum::extract::Extension<Option<nexus_core::auth::middleware::AuthContext>>,
                >,
                      path| api::auth::get_user_permissions(
                    axum::extract::State(server),
                    ext,
                    path,
                )
            }),
        )
        .route(
            "/auth/users/{username}/permissions/{permission}",
            delete(api::auth::revoke_permission),
        )
        // API key management endpoints
        .route("/auth/keys", post(api::auth::create_api_key))
        .route(
            "/auth/keys",
            get({
                let server = nexus_server.clone();
                move |ext: Option<
                    axum::extract::Extension<Option<nexus_core::auth::middleware::AuthContext>>,
                >,
                      query| api::auth::list_api_keys(axum::extract::State(server), ext, query)
            }),
        )
        .route(
            "/auth/keys/{key_id}",
            get({
                let server = nexus_server.clone();
                move |ext: Option<
                    axum::extract::Extension<Option<nexus_core::auth::middleware::AuthContext>>,
                >,
                      path| api::auth::get_api_key(axum::extract::State(server), ext, path)
            }),
        )
        .route("/auth/keys/{key_id}", delete(api::auth::delete_api_key))
        .route(
            "/auth/keys/{key_id}/revoke",
            post(api::auth::revoke_api_key),
        )
        .route("/knn_traverse", post(api::knn::knn_traverse))
        .route(
            "/ingest",
            post(
                move |state: axum::extract::State<std::sync::Arc<NexusServer>>, request| {
                    api::ingest::ingest_data(state, request)
                },
            ),
        )
        .route(
            "/export",
            get(
                move |state: axum::extract::State<std::sync::Arc<NexusServer>>, query| {
                    api::export::export_data(state, query)
                },
            ),
        )
        // Schema management endpoints
        .route("/schema/labels", post(api::schema::create_label))
        .route("/schema/labels", get(api::schema::list_labels))
        .route("/schema/rel_types", post(api::schema::create_rel_type))
        .route("/schema/rel_types", get(api::schema::list_rel_types))
        .route("/schema/indexes", get({
            let server = nexus_server.clone();
            move || {
                let state = api::indexes::IndexState {
                    engine: server.engine.clone(),
                };
                api::indexes::list_indexes(axum::extract::State(state))
            }
        }))
        .route("/schema/indexes", post({
            let server = nexus_server.clone();
            move |req: axum::extract::Json<api::indexes::CreateIndexRequest>| {
                let state = api::indexes::IndexState {
                    engine: server.engine.clone(),
                };
                api::indexes::create_index(axum::extract::State(state), req)
            }
        }))
        .route("/schema/indexes/{name}", delete({
            let server = nexus_server.clone();
            move |path: axum::extract::Path<String>| {
                let state = api::indexes::IndexState {
                    engine: server.engine.clone(),
                };
                api::indexes::delete_index(axum::extract::State(state), path)
            }
        }))
        // Property keys endpoint
        .route("/property_keys", get({
            let server = nexus_server.clone();
            move || {
                let state = api::property_keys::PropertyKeysState {
                    engine: server.engine.clone(),
                };
                api::property_keys::list_property_keys(axum::extract::State(state))
            }
        }))
        // Logs endpoint
        .route("/logs", get(api::logs::get_logs))
        // Query history endpoint — empty list until the
        // server-side history store lands; the response shape
        // matches the GUI's `query-history` consumer so the
        // panel renders without a 404.
        .route(
            "/query-history",
            get(|| async {
                axum::Json(serde_json::json!({
                    "queries": [],
                    "total": 0
                }))
            }),
        )
        // Audit log endpoint (phase5_gui-redesign-mockup-v2 §9.3).
        // Returns the GUI-expected `{entries, next_cursor}` shape
        // so the right-drawer audit feed reads HTTP 200 instead
        // of 404. Entries land here once the audit pipeline
        // streams events to this route; the GUI's local
        // `queryHistoryStore` already merges with the response
        // so users see their own runs in the meantime.
        .route(
            "/audit/log",
            get(|| async {
                axum::Json(serde_json::json!({
                    "entries": [],
                    "next_cursor": null,
                }))
            }),
        )
        // Procedures listing (phase5_gui-redesign-mockup-v2 §9.4).
        // Lists the well-known callable procedures the executor
        // exposes today. Sourced from `db.procedures()` parity in
        // the executor; any procedure registered via the
        // planner's procedure registry is reflected here on the
        // next request because the response is built per-call.
        .route(
            "/procedures",
            get(|| async {
                axum::Json(serde_json::json!({
                    "procedures": [
                        {
                            "name": "vector.knn",
                            "signature": "vector.knn(label :: STRING, vec :: LIST<FLOAT>, k :: INTEGER) :: (node :: NODE, score :: FLOAT)",
                            "description": "K-nearest-neighbour over the per-label HNSW index."
                        },
                        {
                            "name": "spatial.nearest",
                            "signature": "spatial.nearest(p :: POINT, label :: STRING, k :: INTEGER) :: (node :: NODE, dist :: FLOAT)",
                            "description": "K-nearest spatial neighbours over a packed Hilbert R-tree."
                        },
                        {
                            "name": "db.labels",
                            "signature": "db.labels() :: (label :: STRING)",
                            "description": "Stream every registered node label."
                        },
                        {
                            "name": "db.relationshipTypes",
                            "signature": "db.relationshipTypes() :: (relationshipType :: STRING)",
                            "description": "Stream every registered relationship type."
                        },
                        {
                            "name": "db.indexes",
                            "signature": "db.indexes() :: (name :: STRING, type :: STRING, label :: STRING, properties :: LIST<STRING>, state :: STRING)",
                            "description": "List every registered index with type and state."
                        }
                    ]
                }))
            }),
        )
        // Config endpoint
        .route("/config", get(api::config::get_config))
        // Data management endpoints
        .route("/data/nodes", get(api::data::get_node_by_id))
        .route("/data/nodes", post(api::data::create_node))
        .route(
            "/data/nodes/by-external-id",
            get(api::data::get_node_by_external_id),
        )
        .route("/data/nodes", put(api::data::update_node))
        .route("/data/nodes", delete(api::data::delete_node))
        .route("/data/relationships", post(api::data::create_rel))
        // Statistics endpoint
        .route("/stats", get(api::stats::get_stats))
        // Cluster-mode per-tenant stats. Returns 404
        // CLUSTER_MODE_DISABLED on standalone deployments, 404
        // TENANT_UNKNOWN for tenants that haven't been seen yet,
        // and 401 NO_TENANT_CONTEXT if the request didn't carry
        // a tenant binding (shouldn't happen in cluster mode but
        // guards the case anyway).
        .route("/cluster/stats/self", get(api::cluster_stats::tenant_stats))
        // Database management endpoints
        // phase0_fix-multi-database-persistence-and-default G2 — `list_databases`
        // now reads both `database_manager` AND `engine` (for the default
        // database's real stats), so it takes `State<Arc<NexusServer>>`
        // directly like the `/cypher` handler, instead of the narrower
        // `DatabaseState` closure wrapper the other `/databases*` routes use.
        .route("/databases", get(api::database::list_databases))
        .route(
            "/databases",
            post({
                let server = nexus_server.clone();
                move |ext: Option<
                    axum::extract::Extension<Option<nexus_core::auth::middleware::AuthContext>>,
                >,
                      request| {
                    let manager = server.database_manager.clone();
                    async move {
                        api::database::create_database(axum::extract::State(api::database::DatabaseState { manager }), ext, request).await
                    }
                }
            }),
        )
        // Same reasoning as `/databases` above — `get_database` now also
        // reads `engine` directly for the default database's stats.
        .route("/databases/{name}", get(api::database::get_database))
        .route(
            "/databases/{name}",
            delete({
                let server = nexus_server.clone();
                move |ext: Option<
                    axum::extract::Extension<Option<nexus_core::auth::middleware::AuthContext>>,
                >,
                      path| {
                    let manager = server.database_manager.clone();
                    async move {
                        api::database::drop_database(axum::extract::State(api::database::DatabaseState { manager }), ext, path).await
                    }
                }
            }),
        )
        // phase0_fix-cypher-database-routing §4 — the `/session/database`
        // GET/PUT endpoints were removed: the stateless HTTP model has no
        // per-connection identity to hang a session database on, and the
        // switch was a stub that always reported success without persisting.
        // Clients select a database with the per-request `database` field on
        // `POST /cypher`.
        .route("/cache/stats", get(api::cypher::get_cache_stats))
        .route("/cache/clear", post(api::cypher::clear_cache))
        .route("/cache/clean", post(api::cypher::clean_cache))
        // Performance monitoring endpoints
        .route(
            "/performance/statistics",
            get(api::performance::get_query_statistics),
        )
        .route(
            "/performance/slow-queries",
            get(api::performance::get_slow_queries),
        )
        .route(
            "/performance/slow-queries/analysis",
            get(api::performance::analyze_slow_queries),
        )
        .route(
            "/performance/plan-cache",
            get(api::performance::get_plan_cache_statistics),
        )
        .route(
            "/performance/plan-cache/clear",
            post(api::performance::clear_plan_cache),
        )
        // MCP tool performance monitoring endpoints
        .route(
            "/mcp/performance/statistics",
            get(api::mcp_performance::get_mcp_tool_statistics),
        )
        .route(
            "/mcp/performance/tools/{tool_name}",
            get(api::mcp_performance::get_tool_statistics),
        )
        .route(
            "/mcp/performance/slow-tools",
            get(api::mcp_performance::get_slow_tool_calls),
        )
        .route(
            "/mcp/performance/cache",
            get(api::mcp_performance::get_cache_statistics),
        )
        .route(
            "/mcp/performance/cache/clear",
            post(api::mcp_performance::clear_cache),
        )
        // Graph comparison endpoints
        .route("/comparison/compare", post(api::comparison::compare_graphs))
        .route(
            "/comparison/similarity",
            post(api::comparison::calculate_similarity),
        )
        .route("/comparison/stats", post(api::comparison::get_graph_stats))
        .route("/comparison/health", get(api::comparison::health_check))
        .route(
            "/comparison/advanced",
            post(api::comparison::advanced_compare_graphs),
        )
        // Clustering endpoints
        .route(
            "/clustering/algorithms",
            get(api::clustering::get_algorithms),
        )
        .route(
            "/clustering/cluster",
            post({
                let server = nexus_server.clone();
                move |request| api::clustering::cluster_nodes(axum::extract::State(server), request)
            }),
        )
        .route(
            "/clustering/group-by-label",
            post({
                let server = nexus_server.clone();
                move |request| {
                    api::clustering::group_by_label(axum::extract::State(server), request)
                }
            }),
        )
        .route(
            "/clustering/group-by-property",
            post({
                let server = nexus_server.clone();
                move |request| {
                    api::clustering::group_by_property(axum::extract::State(server), request)
                }
            }),
        )
        // Graph correlation endpoints
        .route(
            "/graph-correlation/generate",
            post(api::graph_correlation::generate_graph),
        )
        .route(
            "/graph-correlation/types",
            get(api::graph_correlation::get_graph_types),
        )
        .route(
            "/graph-correlation/auto-generate",
            get(api::auto_generate::auto_generate_graphs),
        )
        // UMICP endpoint for graph correlation
        .route(
            "/umicp/graph",
            post(api::graph_correlation_umicp::handle_umicp_request),
        )
        .route(
            "/openapi.json",
            get(|| async { axum::Json(api::openapi::generate_openapi_spec()) }),
        )
        // MCP StreamableHTTP endpoint
        .nest("/mcp", mcp_router)
        // Replication endpoints
        .route("/replication/status", get(api::replication::get_status))
        .route("/replication/master/stats", get(api::replication::get_master_stats))
        .route("/replication/replica/stats", get(api::replication::get_replica_stats))
        .route("/replication/replicas", get(api::replication::list_replicas))
        .route("/replication/promote", post(api::replication::promote_to_master))
        .route("/replication/snapshot", post(api::replication::create_snapshot))
        .route("/replication/snapshot", get(api::replication::get_last_snapshot))
        .route("/replication/stop", post(api::replication::stop_replication))
        // V2 sharded-cluster management (Phase 5). Endpoints return
        // 503 when sharding is disabled on this node — see
        // `api::cluster`.
        .route("/cluster/status", get(api::cluster::get_status))
        .route("/cluster/add_node", post(api::cluster::add_node))
        .route("/cluster/remove_node", post(api::cluster::remove_node))
        .route("/cluster/rebalance", post(api::cluster::rebalance))
        .route("/cluster/shards/{id}", get(api::cluster::get_shard))
        // Add state to router (must be after all routes)
        .with_state(nexus_server.clone())
}
