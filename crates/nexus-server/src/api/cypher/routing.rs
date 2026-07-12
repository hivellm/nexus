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
    )
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
}
