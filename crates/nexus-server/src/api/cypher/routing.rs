//! Shared AST-predicate routing decision (write-path unification,
//! `docs/nexus/04-write-path-unification.md` Step 3).
//!
//! Both HTTP's `/cypher` handler and the RPC `CYPHER` command must decide,
//! from the SAME parsed AST, whether a query has to run through
//! `Engine::execute_cypher_with_params` — the one path that intercepts
//! `MATCH` / `CREATE` / `DELETE` / `MERGE` / `SET` / `REMOVE` / `FOREACH`
//! before the read-only [`nexus_core::executor::Executor`] ever sees the
//! query — or can go straight to the lock-free executor path.
//!
//! Before this module existed, the HTTP handler decided with
//! `query_upper.starts_with("CREATE")` / `.contains(" MATCH ")` string
//! heuristics, while the RPC dispatcher already used an AST predicate
//! (`needs_engine_interception`, formerly private to
//! `protocol/rpc/dispatch/cypher.rs`). String-prefix routing silently
//! misroutes any query whose write clause isn't the very first token: a
//! leading `//` comment, a lowercase `create`, or `MATCH (a),(b) CREATE
//! (a)-[r]->(b)` all start with something other than `CREATE`/`MERGE` and
//! fell through to the read-only executor — HTTP 200, nothing persisted
//! (bug L1, `docs/nexus/02-bug-inventory.md`). Routing on clause TYPES
//! from the already-parsed AST instead of query text closes that class of
//! bug for both transports at once, with a single shared definition of
//! "this query needs the engine".
//!
//! `EXPLAIN`/`PROFILE` queries parse into a single wrapping
//! `Clause::Explain`/`Clause::Profile` clause whose inner query is nested
//! inside (`ExplainClause::query` / `ProfileClause::query`), not spliced
//! into the outer `ast.clauses`. [`needs_engine_interception`]
//! intentionally does not look inside either wrapper, so it reports
//! `false` for `EXPLAIN CREATE ...` / `PROFILE MATCH ...` exactly as both
//! transports already behaved before this module existed — neither
//! transport special-cased EXPLAIN/PROFILE, and `Engine::
//! execute_cypher_dispatch` already unwraps and drives them internally
//! for the callers that DO reach the engine by some other route (e.g. the
//! `SHOW CONSTRAINTS`/`SHOW FUNCTIONS` branch's bare `engine.execute_cypher`
//! call).

use nexus_core::executor::parser::{Clause, CypherQuery};

/// True when `ast` contains a clause the engine must intercept before the
/// read-only executor runs — i.e. `Engine::execute_cypher_with_params` is
/// the correct dispatch target, not the lock-free `Executor::execute`
/// path. Checked anywhere in `ast.clauses`, not just the first clause, so
/// `MATCH ... CREATE`, `UNWIND ... MERGE`, and `... WITH ... SET` are all
/// caught regardless of which clause happens to come first in the query
/// text.
pub(crate) fn needs_engine_interception(ast: &CypherQuery) -> bool {
    ast.clauses.iter().any(is_engine_clause)
}

/// The clause variants [`needs_engine_interception`] treats as
/// engine-only.
///
/// `Clause::CallProcedure` / `Clause::CallSubquery` (bare `CALL ...`,
/// with no other write/match clause in the query) were added to close a
/// P1: without them a standalone `CALL db.labels()` /
/// `db.relationshipTypes()` / `db.propertyKeys()` never entered the
/// interception block below and fell all the way through to
/// `server.executor` — a boot-time `Executor::default()` backed by a
/// throwaway, empty temp-dir `Catalog` (`executor::dispatch::Executor`'s
/// `Default` impl) that can never see any label/type/key the live
/// engine catalog has recorded, so the three procedures silently
/// returned zero rows on every database. Routing them here instead
/// means [`is_read_only`]'s `READ_ONLY_PROCEDURES` allow-list carve-out
/// (see the interception block in `execute::handler` /
/// `protocol::rpc::dispatch::cypher`) runs them on a clone of the
/// resolved engine's own executor — which `Engine::refresh_executor`
/// keeps pointed at the live catalog, and which additionally has the
/// full-text/R-tree/property-index registries installed that
/// `Executor::default()` never gets — while every other procedure
/// (including anything the AST-only `READ_ONLY_PROCEDURES` predicate
/// cannot classify as read-only, e.g. `apoc.*`/GDS-style calls) now
/// takes the exclusive engine-write path, also on the correct catalog,
/// instead of silently misfiring on the disconnected default executor.
fn is_engine_clause(c: &Clause) -> bool {
    matches!(
        c,
        Clause::Match(_)
            | Clause::Create(_)
            | Clause::Delete(_)
            | Clause::Merge(_)
            | Clause::Set(_)
            | Clause::Remove(_)
            | Clause::Foreach(_)
            | Clause::CallProcedure(_)
            | Clause::CallSubquery(_)
    )
}

/// True when `ast` is a pure autocommit read — safe to run through the
/// lock-free `Executor` clone + `spawn_blocking` path
/// (phase5_lock-free-read-path) instead of taking the exclusive
/// `engine.write().await` lock that every clause in [`is_engine_clause`]
/// otherwise requires.
///
/// `false` whenever `ast` contains, anywhere in `ast.clauses`:
/// - a write clause (`CREATE` / `MERGE` / `SET` / `DELETE` / `REMOVE` /
///   `FOREACH` / `LOAD CSV`),
/// - a DDL, admin, user, API-key, or transaction-control command (each
///   already has its own dedicated dispatch branch earlier in
///   `handler.rs` that must keep running against the engine), or
/// - a `SHOW ...` introspection command — conservatively excluded even
///   though several of them (`SHOW DATABASES`, `SHOW FUNCTIONS`, ...)
///   are logically pure reads, because they too have their own
///   dedicated dispatch branches this predicate has no need to shadow.
///
/// `CALL { ... }` subqueries and `PROFILE` (which actually *executes*
/// its wrapped query, unlike `EXPLAIN`) recurse into the nested
/// [`CypherQuery`] so a write buried inside either is still caught.
/// `EXPLAIN` never executes anything — it is always read-only
/// regardless of what it wraps. `CALL procedure(...)` is read-only only
/// when the procedure name is on the explicit [`READ_ONLY_PROCEDURES`]
/// allow-list; an unrecognized name (including every `apoc.*` and
/// `spatial.addPoint`-style write, and anything served by the generic
/// `ProcedureRegistry` fallback this AST-only predicate cannot see
/// into) is conservatively treated as NOT read-only.
pub(crate) fn is_read_only(ast: &CypherQuery) -> bool {
    ast.clauses.iter().all(is_read_only_clause)
}

/// Per-clause half of [`is_read_only`]. Exhaustive match (no wildcard
/// arm) so a future new [`Clause`] variant fails to compile here until
/// someone consciously decides which bucket it belongs in, rather than
/// silently defaulting to "read-only" or "not read-only".
fn is_read_only_clause(c: &Clause) -> bool {
    match c {
        // Plain read-side clauses.
        Clause::Match(_)
        | Clause::With(_)
        | Clause::Unwind(_)
        | Clause::Union(_)
        | Clause::Where(_)
        | Clause::Return(_)
        | Clause::OrderBy(_)
        | Clause::Limit(_)
        | Clause::Skip(_) => true,

        // EXPLAIN only plans — it never executes the wrapped query, so
        // it is always safe regardless of what that query contains.
        Clause::Explain(_) => true,
        // PROFILE, unlike EXPLAIN, actually executes the wrapped query
        // (`Engine::execute_profile_with_string` calls
        // `execute_cypher_internal`), so its classification must
        // inherit the inner query's.
        Clause::Profile(p) => is_read_only(&p.query),
        // `CALL { ... }` subquery: read-only iff every clause of the
        // nested query is read-only.
        Clause::CallSubquery(sub) => is_read_only(&sub.query),
        // `CALL procedure(...)`: read-only only on the explicit
        // allow-list; conservative default otherwise.
        Clause::CallProcedure(call) => is_read_only_procedure(&call.procedure_name),

        // Definite write clauses.
        Clause::Create(_)
        | Clause::Merge(_)
        | Clause::Set(_)
        | Clause::Delete(_)
        | Clause::Remove(_)
        | Clause::Foreach(_)
        | Clause::LoadCsv(_) => false,

        // DDL / admin / user / API-key / transaction / SHOW commands.
        // Each has its own dedicated dispatch branch earlier in
        // `handler.rs` (and the RPC dispatcher) that must keep running
        // against the engine; conservatively excluded here even where
        // some (e.g. `SHOW DATABASES`) are logically pure reads.
        Clause::CreateDatabase(_)
        | Clause::DropDatabase(_)
        | Clause::AlterDatabase(_)
        | Clause::ShowDatabases
        | Clause::UseDatabase(_)
        | Clause::BeginTransaction
        | Clause::CommitTransaction
        | Clause::RollbackTransaction
        | Clause::Savepoint(_)
        | Clause::RollbackToSavepoint(_)
        | Clause::ReleaseSavepoint(_)
        | Clause::CreateIndex(_)
        | Clause::DropIndex(_)
        | Clause::CreateConstraint(_)
        | Clause::DropConstraint(_)
        | Clause::ShowUsers
        | Clause::ShowUser(_)
        | Clause::CreateUser(_)
        | Clause::DropUser(_)
        | Clause::Grant(_)
        | Clause::Revoke(_)
        | Clause::CreateApiKey(_)
        | Clause::ShowApiKeys(_)
        | Clause::RevokeApiKey(_)
        | Clause::DeleteApiKey(_)
        | Clause::ShowFunctions
        | Clause::ShowConstraints
        | Clause::ShowQueries
        | Clause::TerminateQuery(_)
        | Clause::CreateFunction(_)
        | Clause::DropFunction(_) => false,
    }
}

/// Built-in procedure names (see
/// `crates/nexus-core/src/executor/operators/procedures/call.rs`) that
/// are pure introspection / query reads with no observable effect on
/// graph state. Anything not on this list — including every
/// `db.index.fulltext.createNodeIndex` / `.createRelationshipIndex` /
/// `.drop` / `.awaitEventuallyConsistentIndexRefresh` write-or-write-
/// adjacent form, `spatial.addPoint`, the whole `apoc.*` family, and
/// any procedure served by the generic `ProcedureRegistry` fallback —
/// is conservatively treated as NOT read-only by
/// [`is_read_only_procedure`].
const READ_ONLY_PROCEDURES: &[&str] = &[
    "db.labels",
    "db.propertyKeys",
    "db.relationshipTypes",
    "db.schema",
    "db.indexes",
    "db.indexDetails",
    "db.constraints",
    "db.info",
    "dbms.components",
    "dbms.procedures",
    "dbms.functions",
    "dbms.info",
    "dbms.listConfig",
    "dbms.showCurrentUser",
    "db.index.fulltext.queryNodes",
    "db.index.fulltext.queryRelationships",
    "db.index.fulltext.listAvailableAnalyzers",
    "spatial.nearest",
];

fn is_read_only_procedure(name: &str) -> bool {
    READ_ONLY_PROCEDURES.contains(&name)
}

/// The audit-log operation label (`"CREATE"` or `"MERGE"`) for the first
/// `CREATE`/`MERGE` clause found in `ast.clauses`, in document order, or
/// `None` if neither is present.
///
/// Mirrors the pre-AST-routing behaviour of picking whichever keyword the
/// query text started with, generalized to "whichever comes first in
/// clause order" — the two coincide for any well-formed query, and the
/// AST-based version additionally gets comment-prefixed and
/// lowercase-keyword queries right (bug L1), which the old
/// `query_upper.starts_with(...)` check did not.
pub(crate) fn first_write_kind(ast: &CypherQuery) -> Option<&'static str> {
    ast.clauses.iter().find_map(|c| match c {
        Clause::Merge(_) => Some("MERGE"),
        Clause::Create(_) => Some("CREATE"),
        _ => None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use nexus_core::executor::parser::CypherParser;

    fn parse(query: &str) -> CypherQuery {
        CypherParser::new(query.to_string())
            .parse()
            .unwrap_or_else(|e| panic!("failed to parse `{query}`: {e}"))
    }

    #[test]
    fn match_then_create_needs_engine() {
        let ast = parse("MATCH (a:X {id: 1}), (b:Y {id: 2}) CREATE (a)-[r:T]->(b)");
        assert!(needs_engine_interception(&ast));
    }

    #[test]
    fn unwind_then_merge_needs_engine() {
        let ast = parse("UNWIND $rows AS row MERGE (n:X {id: row.id})");
        assert!(needs_engine_interception(&ast));
    }

    #[test]
    fn with_then_merge_needs_engine() {
        let ast = parse("MATCH (a:X {id: 1}) WITH a MERGE (b:Y {id: a.id})");
        assert!(needs_engine_interception(&ast));
    }

    #[test]
    fn leading_comment_then_create_needs_engine() {
        // Bug L1's exact repro: the query text does not start with
        // "CREATE" — the old `query_upper.starts_with("CREATE")` check
        // missed this entirely.
        let ast = parse("// a leading comment\nCREATE (n:X {id: 1})");
        assert!(needs_engine_interception(&ast));
    }

    #[test]
    fn lowercase_create_needs_engine() {
        let ast = parse("create (n:X {id: 1})");
        assert!(needs_engine_interception(&ast));
    }

    #[test]
    fn explain_prefixed_write_is_not_flagged() {
        // EXPLAIN wraps the inner query in a single `Clause::Explain`;
        // the CREATE clause is nested, not spliced into `ast.clauses`, so
        // the predicate intentionally reports `false` here — matching
        // both transports' behaviour before this module existed (see
        // module docs).
        let ast = parse("EXPLAIN CREATE (n:X {id: 1}) RETURN n");
        assert!(!needs_engine_interception(&ast));
    }

    #[test]
    fn profile_prefixed_read_is_not_flagged() {
        let ast = parse("PROFILE MATCH (n:X) RETURN n");
        assert!(!needs_engine_interception(&ast));
    }

    #[test]
    fn begin_transaction_is_not_flagged() {
        // Transaction commands have their own dedicated dispatch branch
        // in `handler.rs` — this predicate never sees them.
        let ast = parse("BEGIN TRANSACTION");
        assert!(!needs_engine_interception(&ast));
    }

    #[test]
    fn show_databases_is_not_flagged() {
        let ast = parse("SHOW DATABASES");
        assert!(!needs_engine_interception(&ast));
    }

    #[test]
    fn plain_match_read_needs_engine() {
        // Matches today's HTTP behaviour: MATCH-only reads already route
        // through the engine (the old `is_match_query` check), not the
        // lock-free executor.
        let ast = parse("MATCH (n:X) RETURN n");
        assert!(needs_engine_interception(&ast));
    }

    #[test]
    fn standalone_return_does_not_need_engine() {
        let ast = parse("RETURN 1");
        assert!(!needs_engine_interception(&ast));
    }

    #[test]
    fn first_write_kind_prefers_first_clause_in_document_order() {
        assert_eq!(
            first_write_kind(&parse("CREATE (a:X {id:1}) MERGE (b:Y {id:2})")),
            Some("CREATE")
        );
        assert_eq!(
            first_write_kind(&parse("MERGE (a:X {id:1}) CREATE (b:Y {id:2})")),
            Some("MERGE")
        );
        assert_eq!(first_write_kind(&parse("MATCH (n:X) RETURN n")), None);
    }

    // ── `is_read_only` classifier table (phase5_lock-free-read-path §2.2) ──

    #[test]
    fn plain_match_is_read_only() {
        assert!(is_read_only(&parse("MATCH (n:X) RETURN n")));
    }

    #[test]
    fn optional_match_is_read_only() {
        assert!(is_read_only(&parse(
            "MATCH (a:X) OPTIONAL MATCH (a)-[:T]->(b:Y) RETURN a, b"
        )));
    }

    #[test]
    fn with_only_is_read_only() {
        assert!(is_read_only(&parse(
            "MATCH (n:X) WITH n, n.age AS age WHERE age > 10 RETURN n"
        )));
    }

    #[test]
    fn unwind_read_is_read_only() {
        assert!(is_read_only(&parse("UNWIND [1, 2, 3] AS x RETURN x")));
    }

    #[test]
    fn union_of_reads_is_read_only() {
        assert!(is_read_only(&parse(
            "MATCH (n:X) RETURN n.id AS id UNION MATCH (n:Y) RETURN n.id AS id"
        )));
    }

    #[test]
    fn standalone_return_is_read_only() {
        assert!(is_read_only(&parse("RETURN 1")));
    }

    #[test]
    fn create_is_not_read_only() {
        assert!(!is_read_only(&parse("CREATE (n:X {id: 1})")));
    }

    #[test]
    fn merge_is_not_read_only() {
        assert!(!is_read_only(&parse("MERGE (n:X {id: 1})")));
    }

    #[test]
    fn match_set_is_not_read_only() {
        assert!(!is_read_only(&parse("MATCH (n:X) SET n.p = 1")));
    }

    #[test]
    fn match_delete_is_not_read_only() {
        assert!(!is_read_only(&parse("MATCH (n:X) DELETE n")));
    }

    #[test]
    fn match_remove_is_not_read_only() {
        assert!(!is_read_only(&parse("MATCH (n:X) REMOVE n.p")));
    }

    #[test]
    fn foreach_is_not_read_only() {
        assert!(!is_read_only(&parse(
            "MATCH (n:X) FOREACH (x IN [1,2] | SET n.p = x)"
        )));
    }

    #[test]
    fn load_csv_is_not_read_only() {
        assert!(!is_read_only(&parse(
            "LOAD CSV FROM 'file:///x.csv' AS row RETURN row"
        )));
    }

    #[test]
    fn begin_transaction_is_not_read_only() {
        assert!(!is_read_only(&parse("BEGIN TRANSACTION")));
    }

    #[test]
    fn show_databases_is_not_read_only() {
        // Conservative exclusion — SHOW DATABASES is logically a pure
        // read but has its own dedicated dispatch branch in
        // `handler.rs` that this predicate does not need to shadow.
        assert!(!is_read_only(&parse("SHOW DATABASES")));
    }

    #[test]
    fn create_index_is_not_read_only() {
        assert!(!is_read_only(&parse("CREATE INDEX FOR (n:X) ON (n.id)")));
    }

    #[test]
    fn explain_of_write_is_read_only() {
        // EXPLAIN never executes the wrapped query — always safe,
        // regardless of what it wraps.
        assert!(is_read_only(&parse("EXPLAIN CREATE (n:X {id: 1})")));
    }

    #[test]
    fn explain_of_read_is_read_only() {
        assert!(is_read_only(&parse("EXPLAIN MATCH (n:X) RETURN n")));
    }

    #[test]
    fn profile_of_read_is_read_only() {
        assert!(is_read_only(&parse("PROFILE MATCH (n:X) RETURN n")));
    }

    #[test]
    fn profile_of_write_is_not_read_only() {
        // PROFILE actually executes its wrapped query, unlike EXPLAIN —
        // a write buried inside must still be classified as a write.
        assert!(!is_read_only(&parse("PROFILE CREATE (n:X {id: 1})")));
    }

    #[test]
    fn call_subquery_read_is_read_only() {
        assert!(is_read_only(&parse(
            "MATCH (n:X) CALL { MATCH (m:Y) RETURN m } RETURN n"
        )));
    }

    #[test]
    fn call_subquery_write_is_not_read_only() {
        assert!(!is_read_only(&parse(
            "MATCH (n:X) CALL { CREATE (:Tmp) } RETURN n"
        )));
    }

    #[test]
    fn call_read_only_procedure_is_read_only() {
        assert!(is_read_only(&parse("CALL db.labels()")));
    }

    #[test]
    fn call_procedure_that_writes_is_not_read_only() {
        assert!(!is_read_only(&parse(
            "CALL db.index.fulltext.createNodeIndex('docs', ['Doc'], ['body'])"
        )));
    }

    #[test]
    fn call_unknown_procedure_is_conservatively_not_read_only() {
        // Anything off the explicit allow-list — including the whole
        // `apoc.*` family and any GDS-style algorithm served by the
        // generic `ProcedureRegistry` fallback — is conservatively
        // treated as a write since this AST-only predicate cannot see
        // what the procedure actually does.
        assert!(!is_read_only(&parse("CALL apoc.create.node(['X'], {})")));
        assert!(!is_read_only(&parse(
            "CALL gds.pageRank.stream('myGraph') YIELD nodeId, score RETURN nodeId, score"
        )));
    }
}
