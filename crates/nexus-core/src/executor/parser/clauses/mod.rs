//! Clause-level parsing: entry point `parse`, per-clause dispatchers, and
//! all the parse_*_clause methods (MATCH, CREATE, MERGE, SET, DELETE,
//! WHERE, RETURN, ORDER BY, WITH, UNWIND, FOREACH, UNION, database/index/
//! constraint/user/function/API-key admin clauses, CALL (procedure and
//! subquery), LOAD CSV, EXPLAIN, PROFILE, REVOKE/GRANT).
//!
//! Pattern parsing (nodes, relationships, labels, types, property maps,
//! quantifiers) also lives here.
//!
//! Submodule layout:
//! - `pattern`  — node/relationship patterns, QPP, quantifiers
//! - `read`     — MATCH, WHERE, RETURN, ORDER BY, LIMIT, SKIP, WITH, UNWIND, FOREACH, UNION
//! - `write`    — CREATE, MERGE, SET, DELETE, REMOVE
//! - `admin`    — schema/admin: database, index, constraint, user, function, API keys, grant/revoke
//! - `subquery` — CALL subquery, CALL procedure, LOAD CSV, EXPLAIN, PROFILE

mod admin;
mod pattern;
mod read;
mod subquery;
mod write;

use super::CypherParser;
use super::ast::*;
use crate::{Error, Result};
use std::collections::HashMap;

/// Extract the reserved `_id` property from the first node pattern in a
/// CREATE / MERGE clause, removing it from the property map and returning
/// the parsed expression. Only literal-string and parameter expressions
/// are accepted; anything else is a parse error.
///
/// Returns `Ok(None)` when no node carries `_id`.
pub(super) fn extract_underscore_id_from_pattern(
    pattern: &mut Pattern,
) -> Result<Option<Expression>> {
    let mut found: Option<Expression> = None;
    for element in pattern.elements.iter_mut() {
        if let PatternElement::Node(np) = element {
            if let Some(prop_map) = np.properties.as_mut() {
                if let Some(expr) = prop_map.properties.remove("_id") {
                    if found.is_some() {
                        return Err(Error::executor(
                            "Cypher: _id may only appear once in CREATE/MERGE pattern",
                        ));
                    }
                    match &expr {
                        Expression::Literal(Literal::String(_)) | Expression::Parameter(_) => {}
                        _ => {
                            return Err(Error::executor(
                                "Cypher: _id must be a string literal or parameter",
                            ));
                        }
                    }
                    found = Some(expr);
                }
            }
        }
    }
    Ok(found)
}

impl CypherParser {
    /// Create a new parser
    pub fn new(input: String) -> Self {
        Self {
            pos: 0,
            input,
            line: 1,
            column: 1,
        }
    }

    /// Parse a Cypher query
    pub fn parse(&mut self) -> Result<CypherQuery> {
        let mut clauses = Vec::new();
        let params = HashMap::new();

        // Skip whitespace
        self.skip_whitespace();

        // phase6_opencypher-advanced-types §6 — optional leading
        // `GRAPH[name]` clause scoping the whole query to a named
        // database. At most one such clause is permitted, which is why
        // this path fires before `is_clause_boundary()`: once we enter
        // the normal clause loop, `GRAPH[...]` is not recognised
        // anywhere else and any subsequent occurrence bails with a
        // syntax error on the unknown keyword.
        let graph_scope = if self.peek_keyword("GRAPH") {
            self.parse_keyword()?; // consume "GRAPH"
            self.skip_whitespace();
            self.expect_char('[')?;
            self.skip_whitespace();
            let name = self.parse_identifier()?;
            self.skip_whitespace();
            self.expect_char(']')?;
            self.skip_whitespace();
            Some(name)
        } else {
            None
        };

        // Parse clauses
        while self.pos < self.input.len() {
            // Check if we're at a clause boundary
            if self.is_clause_boundary() {
                let clause = self.parse_clause(clauses.last())?;
                clauses.push(clause);

                // Skip whitespace
                self.skip_whitespace();

                // Check for end of input
                if self.pos >= self.input.len() {
                    break;
                }
            } else {
                // No more clauses to parse
                break;
            }
        }

        // Allow empty queries (for EXPLAIN/PROFILE nested queries)
        // The planner will validate if needed
        Ok(CypherQuery {
            clauses,
            params,
            graph_scope,
        })
    }

    /// Neo4j-style syntax error for a `WHERE` appearing as a
    /// standalone top-level clause — i.e. not attached to a
    /// `MATCH` / `OPTIONAL MATCH` / `WITH`. Mirrors the shape Neo4j
    /// 2025.09.0 produces on the same input so the message is
    /// actionable to callers coming from a Neo4j background.
    ///
    /// Migration: insert a `WITH <vars>` projection before the
    /// predicate — e.g. `UNWIND [1,2,3] AS x WITH x WHERE x > 1
    /// RETURN x`.
    pub(super) fn reject_standalone_where(&self) -> Error {
        self.error(
            "Invalid input 'WHERE': expected 'ORDER BY', 'CALL', 'CREATE', \
             'LOAD CSV', 'DELETE', 'DETACH', 'FINISH', 'FOREACH', 'INSERT', \
             'LIMIT', 'MATCH', 'MERGE', 'NODETACH', 'OFFSET', 'OPTIONAL', \
             'REMOVE', 'RETURN', 'SET', 'UNION', 'UNWIND', 'USE', 'WITH' \
             or <EOF>",
        )
    }

    /// Whether a bare `WHERE` may appear at the current position.
    ///
    /// Nexus models WHERE as a top-level clause that attaches to the
    /// previous `MATCH` / `OPTIONAL MATCH` / `WITH` during execution
    /// (see `executor/mod.rs`'s `Clause::Where` handler). Cypher
    /// grammar only allows WHERE immediately after those three
    /// producers; anything else — `UNWIND … WHERE`, `CREATE … WHERE`,
    /// chained `WHERE … WHERE` — is a syntax error in Neo4j
    /// 2025.09.0 and must be in Nexus too.
    pub(super) fn where_is_valid_after(previous: Option<&Clause>) -> bool {
        // `Clause::Match` covers both plain `MATCH` and `OPTIONAL MATCH`
        // (differentiated by the `optional` flag on the AST node).
        matches!(previous, Some(Clause::Match(_)) | Some(Clause::With(_)))
    }

    /// Parse a single clause
    pub(super) fn parse_clause(&mut self, previous: Option<&Clause>) -> Result<Clause> {
        // Check for EXPLAIN or PROFILE first (must be at the beginning)
        if self.peek_keyword("EXPLAIN") {
            return self.parse_explain_clause();
        }
        if self.peek_keyword("PROFILE") {
            return self.parse_profile_clause();
        }

        // Check for OPTIONAL MATCH first
        if self.peek_keyword("OPTIONAL") {
            self.parse_keyword()?; // consume "OPTIONAL"
            self.expect_keyword("MATCH")?;
            let mut match_clause = self.parse_match_clause()?;
            match_clause.optional = true;
            return Ok(Clause::Match(match_clause));
        }

        let keyword = self.parse_keyword()?;

        // Check for DETACH DELETE after reading keyword
        if keyword.to_uppercase() == "DETACH" {
            self.expect_keyword("DELETE")?;
            self.skip_whitespace();

            // Parse list of variables to delete
            let mut items = Vec::new();
            loop {
                let variable = self.parse_identifier()?;
                items.push(variable);

                self.skip_whitespace();
                if self.peek_char() == Some(',') {
                    self.consume_char();
                    self.skip_whitespace();
                } else {
                    break;
                }
            }

            return Ok(Clause::Delete(DeleteClause {
                items,
                detach: true,
            }));
        }

        match keyword.to_uppercase().as_str() {
            "MATCH" => {
                let match_clause = self.parse_match_clause()?;
                Ok(Clause::Match(match_clause))
            }
            "CREATE" => {
                self.skip_whitespace();
                // Check for CREATE DATABASE/INDEX/CONSTRAINT/USER/API KEY before regular CREATE
                if self.peek_keyword("DATABASE") {
                    let create_db_clause = self.parse_create_database_clause()?;
                    Ok(Clause::CreateDatabase(create_db_clause))
                } else if self.peek_keyword("INDEX")
                    || self.peek_keyword("SPATIAL")
                    || self.peek_keyword("OR")
                {
                    // Check for CREATE INDEX (including CREATE SPATIAL INDEX and CREATE OR REPLACE INDEX)
                    let create_index_clause = self.parse_create_index_clause()?;
                    Ok(Clause::CreateIndex(create_index_clause))
                } else if self.peek_keyword("CONSTRAINT") {
                    let create_constraint_clause = self.parse_create_constraint_clause()?;
                    Ok(Clause::CreateConstraint(create_constraint_clause))
                } else if self.peek_keyword("USER") {
                    let create_user_clause = self.parse_create_user_clause()?;
                    Ok(Clause::CreateUser(create_user_clause))
                } else if self.peek_keyword("FUNCTION") {
                    let create_function_clause = self.parse_create_function_clause()?;
                    Ok(Clause::CreateFunction(create_function_clause))
                } else if self.peek_keyword("API") {
                    self.parse_keyword()?; // consume "API"
                    self.expect_keyword("KEY")?;
                    let create_api_key_clause = self.parse_create_api_key_clause()?;
                    Ok(Clause::CreateApiKey(create_api_key_clause))
                } else {
                    // Regular CREATE clause
                    let create_clause = self.parse_create_clause()?;
                    Ok(Clause::Create(create_clause))
                }
            }
            "MERGE" => {
                let merge_clause = self.parse_merge_clause()?;
                Ok(Clause::Merge(merge_clause))
            }
            "SET" => {
                let set_clause = self.parse_set_clause()?;
                Ok(Clause::Set(set_clause))
            }
            "DELETE" => {
                self.skip_whitespace();
                // Check if this is DELETE API KEY or regular DELETE
                if self.peek_keyword("API") {
                    self.parse_keyword()?; // consume "API"
                    self.expect_keyword("KEY")?;
                    let delete_api_key_clause = self.parse_delete_api_key_clause()?;
                    Ok(Clause::DeleteApiKey(delete_api_key_clause))
                } else {
                    let delete_clause = self.parse_delete_clause()?;
                    Ok(Clause::Delete(delete_clause))
                }
            }
            "REMOVE" => {
                let remove_clause = self.parse_remove_clause()?;
                Ok(Clause::Remove(remove_clause))
            }
            "WITH" => {
                let with_clause = self.parse_with_clause()?;
                Ok(Clause::With(with_clause))
            }
            "UNWIND" => {
                let unwind_clause = self.parse_unwind_clause()?;
                Ok(Clause::Unwind(unwind_clause))
            }
            "UNION" => {
                let union_clause = self.parse_union_clause()?;
                Ok(Clause::Union(union_clause))
            }
            "WHERE" => {
                if !Self::where_is_valid_after(previous) {
                    return Err(self.reject_standalone_where());
                }
                let where_clause = self.parse_where_clause()?;
                Ok(Clause::Where(where_clause))
            }
            "RETURN" => {
                let return_clause = self.parse_return_clause()?;
                Ok(Clause::Return(return_clause))
            }
            "ORDER" => {
                self.expect_keyword("BY")?;
                let order_by_clause = self.parse_order_by_clause()?;
                Ok(Clause::OrderBy(order_by_clause))
            }
            "LIMIT" => {
                let limit_clause = self.parse_limit_clause()?;
                Ok(Clause::Limit(limit_clause))
            }
            "SKIP" => {
                let skip_clause = self.parse_skip_clause()?;
                Ok(Clause::Skip(skip_clause))
            }
            "FOREACH" => {
                let foreach_clause = self.parse_foreach_clause()?;
                Ok(Clause::Foreach(foreach_clause))
            }
            "SHOW" => {
                self.skip_whitespace();
                if self.peek_keyword("DATABASES") {
                    self.parse_keyword()?; // consume "DATABASES"
                    Ok(Clause::ShowDatabases)
                } else if self.peek_keyword("USERS") {
                    self.parse_keyword()?; // consume "USERS"
                    Ok(Clause::ShowUsers)
                } else if self.peek_keyword("USER") {
                    let show_user_clause = self.parse_show_user_clause()?;
                    Ok(Clause::ShowUser(show_user_clause))
                } else if self.peek_keyword("FUNCTIONS") {
                    self.parse_keyword()?; // consume "FUNCTIONS"
                    Ok(Clause::ShowFunctions)
                } else if self.peek_keyword("CONSTRAINTS") {
                    self.parse_keyword()?; // consume "CONSTRAINTS"
                    Ok(Clause::ShowConstraints)
                } else if self.peek_keyword("QUERIES") {
                    self.parse_keyword()?; // consume "QUERIES"
                    Ok(Clause::ShowQueries)
                } else if self.peek_keyword("API") {
                    self.parse_keyword()?; // consume "API"
                    self.expect_keyword("KEYS")?;
                    let show_api_keys_clause = self.parse_show_api_keys_clause()?;
                    Ok(Clause::ShowApiKeys(show_api_keys_clause))
                } else {
                    Err(self.error(
                        "SHOW must be followed by DATABASES, USERS, USER, FUNCTIONS, CONSTRAINTS, QUERIES, or API KEYS",
                    ))
                }
            }
            "TERMINATE" => {
                self.skip_whitespace();

                // TERMINATE QUERY 'query-id'
                self.expect_keyword("QUERY")?;
                self.skip_whitespace();

                // Parse query ID (string literal)
                let query_id_expr =
                    if self.peek_char() == Some('\'') || self.peek_char() == Some('"') {
                        self.parse_string_literal()?
                    } else {
                        return Err(self.error("Expected string literal for query ID"));
                    };

                // Extract string value from expression
                let query_id = match query_id_expr {
                    Expression::Literal(Literal::String(s)) => s,
                    _ => return Err(self.error("Expected string literal for query ID")),
                };

                Ok(Clause::TerminateQuery(TerminateQueryClause { query_id }))
            }
            "USE" => {
                self.skip_whitespace();
                if self.peek_keyword("DATABASE") {
                    let use_db_clause = self.parse_use_database_clause()?;
                    Ok(Clause::UseDatabase(use_db_clause))
                } else {
                    Err(self.error("USE must be followed by DATABASE"))
                }
            }
            "CALL" => {
                self.skip_whitespace();
                // Three shapes:
                //   CALL { … }            → subquery (legacy form)
                //   CALL (vars) { … }     → Cypher 25 scoped subquery
                //                           with import list
                //   CALL proc.name(args)  → procedure call
                //
                // The two ambiguous starts are `{` (always subquery)
                // and `(` (scoped subquery — procedure calls always
                // start with a name identifier, never an open paren).
                let next = self.peek_char();
                if next == Some('{') || next == Some('(') {
                    let call_subquery_clause = self.parse_call_subquery_clause()?;
                    Ok(Clause::CallSubquery(call_subquery_clause))
                } else {
                    // This is a procedure call, not a subquery
                    let call_procedure_clause = self.parse_call_procedure_clause()?;
                    Ok(Clause::CallProcedure(call_procedure_clause))
                }
            }
            "LOAD" => {
                self.skip_whitespace();
                if self.peek_keyword("CSV") {
                    let load_csv = self.parse_load_csv_clause()?;
                    Ok(Clause::LoadCsv(load_csv))
                } else {
                    Err(self.error("LOAD must be followed by CSV"))
                }
            }
            "BEGIN" => {
                self.skip_whitespace();
                if self.peek_keyword("TRANSACTION") {
                    self.parse_keyword()?; // consume "TRANSACTION"
                }
                Ok(Clause::BeginTransaction)
            }
            "COMMIT" => {
                self.skip_whitespace();
                if self.peek_keyword("TRANSACTION") {
                    self.parse_keyword()?; // consume "TRANSACTION"
                }
                Ok(Clause::CommitTransaction)
            }
            "ROLLBACK" => {
                self.skip_whitespace();
                // phase6_opencypher-advanced-types §5 —
                // `ROLLBACK TO SAVEPOINT <name>` peels work back to a
                // named marker; the bare `ROLLBACK [TRANSACTION]` form
                // rolls the whole tx as before.
                if self.peek_keyword("TO") {
                    self.parse_keyword()?; // consume "TO"
                    self.skip_whitespace();
                    self.expect_keyword("SAVEPOINT")?;
                    self.skip_whitespace();
                    let name = self.parse_identifier()?;
                    return Ok(Clause::RollbackToSavepoint(SavepointClause { name }));
                }
                if self.peek_keyword("TRANSACTION") {
                    self.parse_keyword()?; // consume "TRANSACTION"
                }
                Ok(Clause::RollbackTransaction)
            }
            "SAVEPOINT" => {
                self.skip_whitespace();
                let name = self.parse_identifier()?;
                Ok(Clause::Savepoint(SavepointClause { name }))
            }
            "RELEASE" => {
                self.skip_whitespace();
                self.expect_keyword("SAVEPOINT")?;
                self.skip_whitespace();
                let name = self.parse_identifier()?;
                Ok(Clause::ReleaseSavepoint(SavepointClause { name }))
            }
            "DROP" => {
                self.skip_whitespace();
                if self.peek_keyword("DATABASE") {
                    let drop_db_clause = self.parse_drop_database_clause()?;
                    Ok(Clause::DropDatabase(drop_db_clause))
                } else if self.peek_keyword("USER") {
                    let drop_user_clause = self.parse_drop_user_clause()?;
                    Ok(Clause::DropUser(drop_user_clause))
                } else if self.peek_keyword("INDEX") {
                    let drop_index_clause = self.parse_drop_index_clause()?;
                    Ok(Clause::DropIndex(drop_index_clause))
                } else if self.peek_keyword("CONSTRAINT") {
                    let drop_constraint_clause = self.parse_drop_constraint_clause()?;
                    Ok(Clause::DropConstraint(drop_constraint_clause))
                } else if self.peek_keyword("FUNCTION") {
                    let drop_function_clause = self.parse_drop_function_clause()?;
                    Ok(Clause::DropFunction(drop_function_clause))
                } else {
                    Err(self.error(
                        "DROP must be followed by DATABASE, USER, INDEX, CONSTRAINT, or FUNCTION",
                    ))
                }
            }
            "GRANT" => {
                let grant_clause = self.parse_grant_clause()?;
                Ok(Clause::Grant(grant_clause))
            }
            "REVOKE" => {
                self.skip_whitespace();
                // Check if this is REVOKE API KEY or regular REVOKE
                if self.peek_keyword("API") {
                    self.parse_keyword()?; // consume "API"
                    self.expect_keyword("KEY")?;
                    let revoke_api_key_clause = self.parse_revoke_api_key_clause()?;
                    Ok(Clause::RevokeApiKey(revoke_api_key_clause))
                } else {
                    let revoke_clause = self.parse_revoke_clause()?;
                    Ok(Clause::Revoke(revoke_clause))
                }
            }
            "ALTER" => {
                self.skip_whitespace();
                if self.peek_keyword("DATABASE") {
                    let alter_db_clause = self.parse_alter_database_clause()?;
                    Ok(Clause::AlterDatabase(alter_db_clause))
                } else {
                    Err(self.error("ALTER must be followed by DATABASE"))
                }
            }
            _ => Err(self.error(&format!("Unexpected keyword: {}", keyword))),
        }
    }
}
