//! Clause-level parsing: entry point `parse`, per-clause dispatchers, and
//! all the parse_*_clause methods (MATCH, CREATE, MERGE, SET, DELETE,
//! WHERE, RETURN, ORDER BY, WITH, UNWIND, FOREACH, UNION, database/index/
//! constraint/user/function/API-key admin clauses, CALL (procedure and
//! subquery), LOAD CSV, EXPLAIN, PROFILE, REVOKE/GRANT).
//!
//! Pattern parsing (nodes, relationships, labels, types, property maps,
//! quantifiers) also lives here.

use super::CypherParser;
use super::ast::*;
use crate::{Error, Result};
use std::collections::HashMap;

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
    fn reject_standalone_where(&self) -> Error {
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
    fn where_is_valid_after(previous: Option<&Clause>) -> bool {
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

    /// Parse MATCH clause
    pub(super) fn parse_match_clause(&mut self) -> Result<MatchClause> {
        self.skip_whitespace();

        // Check for path variable assignment: p = (pattern)
        let path_variable = if self.is_identifier_start() {
            let saved_pos = self.pos;
            let var_name = self.parse_identifier()?;
            self.skip_whitespace();

            if self.peek_char() == Some('=') {
                // This is a path variable assignment
                self.consume_char(); // consume '='
                self.skip_whitespace();
                Some(var_name)
            } else {
                // Not a path variable, restore position
                self.pos = saved_pos;
                None
            }
        } else {
            None
        };

        let mut pattern = self.parse_pattern()?;

        // Set path variable if detected
        if let Some(path_var) = path_variable {
            pattern.path_variable = Some(path_var);
        }

        // Parse hints after pattern: USING INDEX, USING SCAN, USING JOIN
        let mut hints = Vec::new();
        self.skip_whitespace();

        while self.peek_keyword("USING") {
            self.parse_keyword()?; // consume "USING"
            self.skip_whitespace();

            if self.peek_keyword("INDEX") {
                self.parse_keyword()?; // consume "INDEX"
                self.skip_whitespace();

                // Parse: variable:Label(property)
                let variable = self.parse_identifier()?;
                self.skip_whitespace();
                self.expect_char(':')?;
                let label = self.parse_identifier()?;
                self.skip_whitespace();
                self.expect_char('(')?;
                let property = self.parse_identifier()?;
                self.expect_char(')')?;

                hints.push(QueryHint::UsingIndex {
                    variable,
                    label,
                    property,
                });
            } else if self.peek_keyword("SCAN") {
                self.parse_keyword()?; // consume "SCAN"
                self.skip_whitespace();

                // Parse: variable:Label
                let variable = self.parse_identifier()?;
                self.skip_whitespace();
                self.expect_char(':')?;
                let label = self.parse_identifier()?;

                hints.push(QueryHint::UsingScan { variable, label });
            } else if self.peek_keyword("JOIN") {
                self.parse_keyword()?; // consume "JOIN"
                self.skip_whitespace();

                if self.peek_keyword("ON") {
                    self.parse_keyword()?; // consume "ON"
                    self.skip_whitespace();
                }

                // Parse: variable
                let variable = self.parse_identifier()?;

                hints.push(QueryHint::UsingJoin { variable });
            } else {
                return Err(self.error("USING must be followed by INDEX, SCAN, or JOIN"));
            }

            self.skip_whitespace();
        }

        Ok(MatchClause {
            pattern,
            where_clause: None, // WHERE is now a separate clause
            optional: false,    // Set by caller if this is OPTIONAL MATCH
            hints,
        })
    }

    /// Parse CREATE clause
    pub(super) fn parse_create_clause(&mut self) -> Result<CreateClause> {
        self.skip_whitespace();
        let pattern = self.parse_pattern()?;

        Ok(CreateClause { pattern })
    }

    /// Parse MERGE clause
    pub(super) fn parse_merge_clause(&mut self) -> Result<MergeClause> {
        self.skip_whitespace();
        let pattern = self.parse_pattern()?;

        // Check for ON CREATE clause
        let on_create = if self.peek_keyword("ON") && self.peek_keyword_at(1, "CREATE") {
            self.skip_whitespace();
            self.parse_keyword()?; // "ON"
            self.skip_whitespace();
            self.parse_keyword()?; // "CREATE"
            self.skip_whitespace();
            // Parse SET keyword before parsing SET clause
            if self.peek_keyword("SET") {
                self.parse_keyword()?; // "SET"
                Some(self.parse_set_clause()?)
            } else {
                None
            }
        } else {
            None
        };

        // Check for ON MATCH clause
        let on_match = if self.peek_keyword("ON") && self.peek_keyword_at(1, "MATCH") {
            self.skip_whitespace();
            self.parse_keyword()?; // "ON"
            self.skip_whitespace();
            self.parse_keyword()?; // "MATCH"
            self.skip_whitespace();
            // Parse SET keyword before parsing SET clause
            if self.peek_keyword("SET") {
                self.parse_keyword()?; // "SET"
                Some(self.parse_set_clause()?)
            } else {
                None
            }
        } else {
            None
        };

        Ok(MergeClause {
            pattern,
            on_create,
            on_match,
        })
    }

    /// Parse SET clause
    pub(super) fn parse_set_clause(&mut self) -> Result<SetClause> {
        self.skip_whitespace();
        let mut items = Vec::new();

        loop {
            // Parse identifier (variable name)
            let target = self.parse_identifier()?;
            self.skip_whitespace();

            // Check if we have a property assignment (node.property = value)
            if self.peek_char() == Some('.') {
                self.consume_char();
                let property = self.parse_identifier()?;
                self.skip_whitespace();
                self.expect_char('=')?;
                self.skip_whitespace();
                let value = self.parse_expression()?;
                items.push(SetItem::Property {
                    target,
                    property,
                    value,
                });
            } else if self.peek_char() == Some(':') {
                // Label addition (node:Label) — accepts `:$param` for
                // write-side dynamic labels (advanced-types §2).
                // Chained labels on a single SET item (`SET n:A:B`) push
                // one `SetItem::Label` per segment so the engine can
                // resolve and apply them one-at-a-time, mirroring the
                // READ path and keeping error localisation sharp.
                let mut any = false;
                while self.peek_char() == Some(':') {
                    self.consume_char();
                    let label = if self.peek_char() == Some('$') {
                        self.consume_char();
                        format!("${}", self.parse_identifier()?)
                    } else {
                        self.parse_identifier()?
                    };
                    items.push(SetItem::Label {
                        target: target.clone(),
                        label,
                    });
                    self.skip_whitespace();
                    any = true;
                }
                if !any {
                    return Err(Error::storage(
                        "SET clause: expected label after ':'".to_string(),
                    ));
                }
            } else if self.peek_char() == Some('+') && self.peek_char_at(1) == Some('=') {
                // phase6_opencypher-quickwins §6 — `SET lhs += mapExpr`
                // merge semantics. Distinct from `SET lhs = mapExpr`
                // which replaces the entire bag.
                self.consume_char(); // '+'
                self.consume_char(); // '='
                self.skip_whitespace();
                let map = self.parse_expression()?;
                items.push(SetItem::MapMerge { target, map });
            } else {
                return Err(Error::storage(
                    "SET clause: expected property assignment or label".to_string(),
                ));
            }

            // Check for more items
            self.skip_whitespace();
            if self.peek_char() == Some(',') {
                self.consume_char();
                self.skip_whitespace();
            } else {
                break;
            }
        }

        Ok(SetClause { items })
    }

    /// Parse DELETE clause
    pub(super) fn parse_delete_clause(&mut self) -> Result<DeleteClause> {
        self.skip_whitespace();

        // Check for DETACH keyword
        let detach = if self.peek_keyword("DETACH") {
            self.parse_keyword()?;
            self.skip_whitespace();
            true
        } else {
            false
        };

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

        Ok(DeleteClause { items, detach })
    }

    /// Parse REMOVE clause
    pub(super) fn parse_remove_clause(&mut self) -> Result<RemoveClause> {
        self.skip_whitespace();
        let mut items = Vec::new();

        loop {
            // Parse identifier (variable name)
            let target = self.parse_identifier()?;
            self.skip_whitespace();

            // Check if we have a property removal (node.property)
            if self.peek_char() == Some('.') {
                self.consume_char();
                let property = self.parse_identifier()?;
                items.push(RemoveItem::Property { target, property });
            } else if self.peek_char() == Some(':') {
                // Label removal (node:Label) — accepts `:$param` for
                // write-side dynamic labels (advanced-types §2). Chained
                // labels on a single REMOVE item (`REMOVE n:A:B`) push
                // one `RemoveItem::Label` per segment.
                let mut any = false;
                while self.peek_char() == Some(':') {
                    self.consume_char();
                    let label = if self.peek_char() == Some('$') {
                        self.consume_char();
                        format!("${}", self.parse_identifier()?)
                    } else {
                        self.parse_identifier()?
                    };
                    items.push(RemoveItem::Label {
                        target: target.clone(),
                        label,
                    });
                    self.skip_whitespace();
                    any = true;
                }
                if !any {
                    return Err(Error::storage(
                        "REMOVE clause: expected label after ':'".to_string(),
                    ));
                }
            } else {
                return Err(Error::storage(
                    "REMOVE clause: expected property or label removal".to_string(),
                ));
            }

            // Check for more items
            self.skip_whitespace();
            if self.peek_char() == Some(',') {
                self.consume_char();
                self.skip_whitespace();
            } else {
                break;
            }
        }

        Ok(RemoveClause { items })
    }

    /// Parse pattern
    pub(super) fn parse_pattern(&mut self) -> Result<Pattern> {
        let mut elements = Vec::new();

        // Parse first node
        let node = self.parse_node_pattern()?;
        elements.push(PatternElement::Node(node));

        // Parse relationships and nodes, or comma-separated nodes
        while self.pos < self.input.len() {
            // Check if there's a relationship pattern by looking ahead
            let saved_pos = self.pos;
            let saved_line = self.line;
            let saved_column = self.column;

            // Skip whitespace
            self.skip_whitespace();

            // Check for comma (multiple independent node patterns)
            if self.peek_char() == Some(',') {
                self.consume_char(); // consume ','
                self.skip_whitespace();

                // Parse next node pattern as independent node
                let node = self.parse_node_pattern()?;
                elements.push(PatternElement::Node(node));
                continue;
            }

            // Check if we have a relationship pattern
            if self.peek_char() == Some('-')
                || self.peek_char() == Some('<')
                || self.peek_char() == Some('>')
            {
                // Parse relationship
                let rel = self.parse_relationship_pattern()?;
                elements.push(PatternElement::Relationship(rel));

                // Parse next node
                let node = self.parse_node_pattern()?;
                elements.push(PatternElement::Node(node));
                continue;
            }

            // QPP: `( subPattern ) quantifier` — Cypher 25 / GQL.
            // Triggered by a parenthesis directly after a node, with
            // no intervening `-` / `<` / `>` rel-operator. The closing
            // paren must be followed by a quantifier token (`{m,n}`,
            // `*`, `+`, `?`). Anything else is not a QPP — we restore
            // position and let the caller terminate the pattern.
            if self.peek_char() == Some('(') {
                match self.try_parse_qpp_group()? {
                    Some(group) => {
                        // Slice-1 QPP normalisation
                        // (`phase6_opencypher-quantified-path-patterns`):
                        // when the group is the textbook
                        // `( ()-[:T]->() ){m,n}` shape, push it as a
                        // plain quantified Relationship so every
                        // downstream consumer (planner, projection,
                        // EXISTS subqueries, …) treats it exactly
                        // like a legacy `*m..n` form. Groups that
                        // carry inner state survive as
                        // QuantifiedGroup and the planner surfaces
                        // a clean ERR_QPP_NOT_IMPLEMENTED for them.
                        if let Some(rel) = group.try_lower_to_var_length_rel() {
                            elements.push(PatternElement::Relationship(rel));
                        } else {
                            elements.push(PatternElement::QuantifiedGroup(group));
                        }
                        // The textbook QPP shape `(a)( body ){m,n}(b)`
                        // is followed by a trailing boundary node.
                        // Without parsing it here the outer pattern
                        // ends at the group and `(b)` gets dropped,
                        // which leaves the planner without a target
                        // variable and silently breaks projections
                        // (`RETURN b` returns whatever happened to
                        // be in the last expand slot).
                        self.skip_whitespace();
                        if self.peek_char() == Some('(') {
                            let node = self.parse_node_pattern()?;
                            elements.push(PatternElement::Node(node));
                        }
                        continue;
                    }
                    None => {
                        self.pos = saved_pos;
                        self.line = saved_line;
                        self.column = saved_column;
                        break;
                    }
                }
            }

            // Restore position if no relationship, comma, or QPP found
            self.pos = saved_pos;
            self.line = saved_line;
            self.column = saved_column;
            break;
        }

        Ok(Pattern {
            elements,
            path_variable: None, // Set by caller if path variable assignment detected
        })
    }

    /// Attempt to parse a quantified path pattern group starting at
    /// the current position. Returns `Ok(None)` when the lookahead
    /// does not form a valid QPP (caller should backtrack); returns
    /// `Ok(Some(group))` on success. Rejects nested QPP (one level
    /// deep — Cypher 25 restriction) and empty bodies.
    fn try_parse_qpp_group(&mut self) -> Result<Option<QuantifiedGroup>> {
        debug_assert_eq!(self.peek_char(), Some('('));
        let restore_pos = self.pos;
        let restore_line = self.line;
        let restore_column = self.column;

        self.consume_char(); // '('
        self.skip_whitespace();

        // Body must start with a node pattern. Anything else fails
        // the QPP match and the caller backtracks.
        if self.peek_char() != Some('(') {
            self.pos = restore_pos;
            self.line = restore_line;
            self.column = restore_column;
            return Ok(None);
        }

        // Detect nested QPP before entering the recursive body parser:
        // `( ( ( ... ) ){..} ... )` starts `(((`. Since QPP's recursive
        // descent cannot parse a body whose first element is itself a
        // QPP, we intercept the shape explicitly and surface the
        // Cypher 25 restriction with a clean error.
        {
            let mut probe = self.pos + 1; // skip the inner `(`
            while probe < self.input.len() {
                let c = self.input.as_bytes()[probe] as char;
                if c == ' ' || c == '\t' || c == '\r' || c == '\n' {
                    probe += 1;
                } else {
                    break;
                }
            }
            if probe < self.input.len() && self.input.as_bytes()[probe] as char == '(' {
                return Err(Error::CypherSyntax(
                    "ERR_QPP_NESTING_TOO_DEEP: quantified path patterns \
                     cannot nest (Cypher 25 restriction)"
                        .to_string(),
                ));
            }
        }

        let inner = match self.parse_pattern() {
            Ok(pattern) => pattern,
            Err(_) => {
                self.pos = restore_pos;
                self.line = restore_line;
                self.column = restore_column;
                return Ok(None);
            }
        };

        // Reject nested QPP (one level deep — Cypher 25).
        if inner
            .elements
            .iter()
            .any(|e| matches!(e, PatternElement::QuantifiedGroup(_)))
        {
            return Err(Error::CypherSyntax(
                "ERR_QPP_NESTING_TOO_DEEP: quantified path patterns \
                 cannot nest (Cypher 25 restriction)"
                    .to_string(),
            ));
        }

        self.skip_whitespace();

        // Optional inner `WHERE` clause inside the QPP body
        // (Cypher 25 §1.4): `( body WHERE predicate ){m,n}`. The
        // predicate gets evaluated against the per-iteration
        // bindings, so an iteration that fails it is dropped
        // before the row is emitted. We only consume `WHERE` if
        // it sits right before the closing `)` — anything else
        // means the body itself never closed and we should
        // backtrack so the outer pattern terminates normally.
        let where_clause = if self.peek_keyword("WHERE") {
            self.parse_keyword()?;
            self.skip_whitespace();
            let expr = self.parse_expression()?;
            self.skip_whitespace();
            Some(expr)
        } else {
            None
        };

        if self.peek_char() != Some(')') {
            self.pos = restore_pos;
            self.line = restore_line;
            self.column = restore_column;
            return Ok(None);
        }
        self.consume_char(); // ')'

        // Quantifier is mandatory. A bare `( subPattern )` without a
        // quantifier is not a QPP — backtrack so the caller can
        // terminate the outer pattern normally.
        let quantifier = match self.parse_relationship_quantifier()? {
            Some(q) => q,
            None => {
                self.pos = restore_pos;
                self.line = restore_line;
                self.column = restore_column;
                return Ok(None);
            }
        };

        // Reject `{n,m}` where n > m.
        if let RelationshipQuantifier::Range(lo, hi) = &quantifier {
            if lo > hi {
                return Err(Error::CypherSyntax(format!(
                    "ERR_QPP_INVALID_QUANTIFIER: lower bound {lo} \
                     exceeds upper bound {hi}"
                )));
            }
        }

        Ok(Some(QuantifiedGroup {
            inner: inner.elements,
            quantifier,
            where_clause,
        }))
    }

    /// Parse node pattern
    pub(super) fn parse_node_pattern(&mut self) -> Result<NodePattern> {
        self.expect_char('(')?;
        self.skip_whitespace();

        let variable = if self.is_identifier_start() {
            Some(self.parse_identifier()?)
        } else {
            None
        };

        self.skip_whitespace();
        let labels = if self.peek_char() == Some(':') {
            self.parse_labels()?
        } else {
            Vec::new()
        };

        self.skip_whitespace();
        let properties = if self.peek_char() == Some('{') {
            Some(self.parse_property_map()?)
        } else {
            None
        };

        self.skip_whitespace();
        self.expect_char(')')?;

        Ok(NodePattern {
            variable,
            labels,
            properties,
        })
    }

    /// Parse relationship pattern
    pub(super) fn parse_relationship_pattern(&mut self) -> Result<RelationshipPattern> {
        // Parse initial direction: "-" or "<-"
        let left_arrow = if self.peek_char() == Some('<') {
            self.consume_char();
            self.expect_char('-')?;
            true
        } else if self.peek_char() == Some('-') {
            self.consume_char();
            false
        } else {
            return Err(Error::CypherSyntax(format!(
                "Expected relationship direction at line 1, column {}",
                self.pos + 1
            )));
        };

        self.expect_char('[')?;
        self.skip_whitespace();

        let variable = if self.is_identifier_start() {
            Some(self.parse_identifier()?)
        } else {
            None
        };

        self.skip_whitespace();
        let types = if self.peek_char() == Some(':') {
            self.parse_types()?
        } else {
            Vec::new()
        };

        self.skip_whitespace();

        // Check if next token is a quantifier (starts with *, +, ?, or { followed by digit/comma/})
        // or a property map (starts with { followed by identifier)
        let (properties, quantifier) = if self.peek_char() == Some('{') {
            // Peek ahead to see if it's a quantifier or property map
            // Check character after '{' (skip whitespace)
            let mut peek_offset = 1;
            let mut is_quantifier = false;
            while peek_offset < self.input.len() - self.pos {
                if let Some(c) = self.peek_char_at(peek_offset) {
                    if c.is_whitespace() {
                        peek_offset += 1;
                        continue;
                    }
                    // If next char is digit, comma, or '}', it's a quantifier
                    is_quantifier = c.is_ascii_digit() || c == ',' || c == '}';
                    break;
                } else {
                    break;
                }
            }

            if is_quantifier {
                // It's a quantifier, not properties
                (None, self.parse_relationship_quantifier()?)
            } else {
                // It's a property map
                (
                    Some(self.parse_property_map()?),
                    self.parse_relationship_quantifier()?,
                )
            }
        } else {
            // No properties, check for quantifier
            (None, self.parse_relationship_quantifier()?)
        };

        self.skip_whitespace();
        self.expect_char(']')?;

        // Parse final direction: "->" or "-"
        self.expect_char('-')?;
        let right_arrow = if self.peek_char() == Some('>') {
            self.consume_char();
            true
        } else {
            false
        };

        // Determine final direction
        let direction = match (left_arrow, right_arrow) {
            (true, false) => RelationshipDirection::Incoming, // <-[r]-
            (false, true) => RelationshipDirection::Outgoing, // -[r]->
            (false, false) => RelationshipDirection::Both,    // -[r]-
            (true, true) => {
                return Err(Error::CypherSyntax(format!(
                    "Invalid relationship direction <-[]-> at line 1, column {}",
                    self.pos + 1
                )));
            }
        };

        Ok(RelationshipPattern {
            variable,
            types,
            direction,
            properties,
            quantifier,
        })
    }

    /// Parse relationship direction
    pub(super) fn parse_relationship_direction(&mut self) -> Result<RelationshipDirection> {
        match self.peek_char() {
            Some('-') => {
                self.consume_char();
                if self.peek_char() == Some('>') {
                    self.consume_char();
                    Ok(RelationshipDirection::Outgoing)
                } else {
                    Ok(RelationshipDirection::Both)
                }
            }
            Some('<') => {
                self.consume_char();
                if self.peek_char() == Some('-') {
                    self.consume_char();
                    Ok(RelationshipDirection::Incoming)
                } else {
                    Err(self.error("Invalid relationship direction"))
                }
            }
            _ => Err(self.error("Expected relationship direction")),
        }
    }

    /// Parse labels.
    ///
    /// phase6_opencypher-advanced-types §2 — parameter-valued labels.
    /// A `:$param` label is encoded as the sentinel string `"$param"`
    /// (leading `$` is never a valid identifier character, so downstream
    /// writers can unambiguously recognise and resolve it against the
    /// execution-time parameter map via
    /// [`crate::engine::dynamic_labels::resolve_labels`]).
    pub(super) fn parse_labels(&mut self) -> Result<Vec<String>> {
        let mut labels = Vec::new();

        while self.peek_char() == Some(':') {
            self.consume_char(); // consume ':'
            if self.peek_char() == Some('$') {
                self.consume_char(); // consume '$'
                let param = self.parse_identifier()?;
                labels.push(format!("${param}"));
            } else {
                let label = self.parse_identifier()?;
                labels.push(label);
            }
        }

        Ok(labels)
    }

    /// Parse types
    pub(super) fn parse_types(&mut self) -> Result<Vec<String>> {
        let mut types = Vec::new();

        // First type must be preceded by ':'
        if self.peek_char() == Some(':') {
            self.consume_char(); // consume ':'
            let r#type = self.parse_identifier()?;
            types.push(r#type);

            // Additional types can be separated by '|' (e.g., :TYPE1|TYPE2)
            self.skip_whitespace();
            while self.peek_char() == Some('|') {
                self.consume_char(); // consume '|'
                self.skip_whitespace();
                let r#type = self.parse_identifier()?;
                types.push(r#type);
                self.skip_whitespace();
            }
        }

        Ok(types)
    }

    /// Parse property map
    pub(super) fn parse_property_map(&mut self) -> Result<PropertyMap> {
        self.expect_char('{')?;
        self.skip_whitespace();

        let mut properties = HashMap::new();

        while self.peek_char() != Some('}') {
            let key = self.parse_identifier()?;
            self.skip_whitespace();
            self.expect_char(':')?;
            self.skip_whitespace();
            let value = self.parse_expression()?;
            properties.insert(key, value);

            self.skip_whitespace();
            if self.peek_char() == Some(',') {
                self.consume_char();
                self.skip_whitespace();
            }
        }

        self.expect_char('}')?;

        Ok(PropertyMap { properties })
    }

    /// Parse relationship quantifier
    pub(super) fn parse_relationship_quantifier(
        &mut self,
    ) -> Result<Option<RelationshipQuantifier>> {
        match self.peek_char() {
            Some('*') => {
                self.consume_char();
                // Check if there's a number after * (e.g., *1..3 or *5)
                // Skip whitespace first
                self.skip_whitespace();
                if self.is_digit() {
                    // Parse range quantifier without braces: *1..3 or *5
                    self.parse_range_quantifier_without_braces()
                } else {
                    // Just * means zero or more
                    Ok(Some(RelationshipQuantifier::ZeroOrMore))
                }
            }
            Some('+') => {
                self.consume_char();
                Ok(Some(RelationshipQuantifier::OneOrMore))
            }
            Some('?') => {
                self.consume_char();
                Ok(Some(RelationshipQuantifier::ZeroOrOne))
            }
            Some('{') => self.parse_range_quantifier(),
            _ => Ok(None),
        }
    }

    /// Parse range quantifier without braces: *1..3 or *5
    pub(super) fn parse_range_quantifier_without_braces(
        &mut self,
    ) -> Result<Option<RelationshipQuantifier>> {
        let start = if self.is_digit() {
            Some(self.parse_number()?)
        } else {
            None
        };

        // Check for range separator: ',' or '..'
        if self.peek_char() == Some(',')
            || (self.peek_char() == Some('.') && self.peek_char_at(1) == Some('.'))
        {
            if self.peek_char() == Some(',') {
                self.consume_char();
            } else {
                // Consume '..'
                self.consume_char();
                self.consume_char();
            }
            let end = if self.is_digit() {
                Some(self.parse_number()?)
            } else {
                None
            };

            match (start, end) {
                (Some(n), Some(m)) => {
                    Ok(Some(RelationshipQuantifier::Range(n as usize, m as usize)))
                }
                (Some(n), None) => Ok(Some(RelationshipQuantifier::Range(n as usize, usize::MAX))),
                (None, Some(m)) => Ok(Some(RelationshipQuantifier::Range(0, m as usize))),
                (None, None) => Ok(Some(RelationshipQuantifier::ZeroOrMore)),
            }
        } else {
            // No range separator, just a number means exact count
            if let Some(n) = start {
                Ok(Some(RelationshipQuantifier::Exact(n as usize)))
            } else {
                Ok(Some(RelationshipQuantifier::ZeroOrMore))
            }
        }
    }

    /// Parse range quantifier
    pub(super) fn parse_range_quantifier(&mut self) -> Result<Option<RelationshipQuantifier>> {
        self.expect_char('{')?;

        let start = if self.is_digit() {
            Some(self.parse_number()?)
        } else {
            None
        };

        // Check for range separator: ',' or '..'
        if self.peek_char() == Some(',')
            || (self.peek_char() == Some('.') && self.peek_char_at(1) == Some('.'))
        {
            if self.peek_char() == Some(',') {
                self.consume_char();
            } else {
                // Consume '..'
                self.consume_char();
                self.consume_char();
            }
            let end = if self.is_digit() {
                Some(self.parse_number()?)
            } else {
                None
            };

            self.expect_char('}')?;

            match (start, end) {
                (Some(n), Some(m)) => {
                    Ok(Some(RelationshipQuantifier::Range(n as usize, m as usize)))
                }
                (Some(n), None) => Ok(Some(RelationshipQuantifier::Range(n as usize, usize::MAX))),
                (None, Some(m)) => Ok(Some(RelationshipQuantifier::Range(0, m as usize))),
                (None, None) => Ok(Some(RelationshipQuantifier::ZeroOrMore)),
            }
        } else {
            self.expect_char('}')?;

            if let Some(n) = start {
                Ok(Some(RelationshipQuantifier::Exact(n as usize)))
            } else {
                Ok(Some(RelationshipQuantifier::ZeroOrMore))
            }
        }
    }

    /// Parse WHERE clause
    pub(super) fn parse_where_clause(&mut self) -> Result<WhereClause> {
        self.skip_whitespace();
        let expression = self.parse_expression()?;
        Ok(WhereClause { expression })
    }

    /// Parse RETURN clause
    pub(super) fn parse_return_clause(&mut self) -> Result<ReturnClause> {
        self.skip_whitespace();

        let distinct = if self.peek_keyword("DISTINCT") {
            self.parse_keyword()?;
            true
        } else {
            false
        };

        self.skip_whitespace();

        let mut items = Vec::new();

        loop {
            let item = self.parse_return_item()?;
            items.push(item);

            if self.peek_char() == Some(',') {
                self.consume_char();
                self.skip_whitespace();
            } else {
                break;
            }
        }

        Ok(ReturnClause { items, distinct })
    }

    /// Parse return item
    pub(super) fn parse_return_item(&mut self) -> Result<ReturnItem> {
        let expression = self.parse_expression()?;

        let alias = if self.peek_keyword("AS") {
            self.parse_keyword()?;
            Some(self.parse_identifier()?)
        } else {
            None
        };

        Ok(ReturnItem { expression, alias })
    }

    /// Parse ORDER BY clause
    pub(super) fn parse_order_by_clause(&mut self) -> Result<OrderByClause> {
        self.skip_whitespace();

        let mut items = Vec::new();

        loop {
            let expression = self.parse_expression()?;

            let direction = if self.peek_keyword("ASC") {
                self.parse_keyword()?;
                SortDirection::Ascending
            } else if self.peek_keyword("DESC") {
                self.parse_keyword()?;
                SortDirection::Descending
            } else {
                SortDirection::Ascending
            };

            items.push(SortItem {
                expression,
                direction,
            });

            if self.peek_char() == Some(',') {
                self.consume_char();
                self.skip_whitespace();
            } else {
                break;
            }
        }

        Ok(OrderByClause { items })
    }

    /// Parse LIMIT clause
    pub(super) fn parse_limit_clause(&mut self) -> Result<LimitClause> {
        self.skip_whitespace();
        let count = self.parse_expression()?;
        Ok(LimitClause { count })
    }

    /// Parse SKIP clause
    pub(super) fn parse_skip_clause(&mut self) -> Result<SkipClause> {
        self.skip_whitespace();
        let count = self.parse_expression()?;
        Ok(SkipClause { count })
    }

    /// Parse WITH clause
    pub(super) fn parse_with_clause(&mut self) -> Result<WithClause> {
        self.skip_whitespace();

        let distinct = if self.peek_keyword("DISTINCT") {
            self.parse_keyword()?;
            true
        } else {
            false
        };

        self.skip_whitespace();

        let mut items = Vec::new();

        loop {
            let item = self.parse_return_item()?;
            items.push(item);

            if self.peek_char() == Some(',') {
                self.consume_char();
                self.skip_whitespace();
            } else {
                break;
            }
        }

        // Check for WHERE clause in WITH
        let where_clause = if self.peek_keyword("WHERE") {
            self.parse_keyword()?;
            Some(self.parse_where_clause()?.clone())
        } else {
            None
        };

        Ok(WithClause {
            items,
            distinct,
            where_clause,
        })
    }

    /// Parse UNWIND clause
    pub(super) fn parse_unwind_clause(&mut self) -> Result<UnwindClause> {
        self.skip_whitespace();

        // Parse the expression (list to unwind)
        let expression = self.parse_expression()?;

        self.skip_whitespace();

        // Expect AS keyword
        self.expect_keyword("AS")?;

        self.skip_whitespace();

        // Parse the variable name
        let variable = self.parse_identifier()?;

        Ok(UnwindClause {
            expression,
            variable,
        })
    }

    /// Parse FOREACH clause
    pub(super) fn parse_foreach_clause(&mut self) -> Result<ForeachClause> {
        self.skip_whitespace();

        // Expect opening parenthesis
        self.expect_char('(')?;
        self.skip_whitespace();

        // Parse variable name
        let variable = self.parse_identifier()?;
        self.skip_whitespace();

        // Expect IN keyword
        self.expect_keyword("IN")?;
        self.skip_whitespace();

        // Parse list expression
        let list_expression = self.parse_expression()?;
        self.skip_whitespace();

        // Expect pipe separator |
        self.expect_char('|')?;
        self.skip_whitespace();

        // Parse update clauses (SET or DELETE) until closing parenthesis
        let mut update_clauses = Vec::new();
        loop {
            self.skip_whitespace();

            // Check for closing parenthesis
            if self.peek_char() == Some(')') {
                self.consume_char();
                break;
            }

            // Parse SET or DELETE clause
            let keyword = self.parse_keyword()?;
            match keyword.to_uppercase().as_str() {
                "SET" => {
                    let set_clause = self.parse_set_clause()?;
                    update_clauses.push(ForeachUpdateClause::Set(set_clause));
                }
                "DELETE" => {
                    // Check for DETACH DELETE
                    let detach = if self.peek_keyword("DETACH") {
                        self.parse_keyword()?;
                        self.skip_whitespace();
                        true
                    } else {
                        false
                    };

                    self.skip_whitespace();
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

                    let delete_clause = DeleteClause { items, detach };
                    update_clauses.push(ForeachUpdateClause::Delete(delete_clause));
                }
                _ => {
                    return Err(self.error(&format!(
                        "Expected SET or DELETE in FOREACH, found: {}",
                        keyword
                    )));
                }
            }

            self.skip_whitespace();
        }

        Ok(ForeachClause {
            variable,
            list_expression,
            update_clauses,
        })
    }

    /// Parse UNION clause
    pub(super) fn parse_union_clause(&mut self) -> Result<UnionClause> {
        self.skip_whitespace();

        // Check for ALL keyword
        let union_type = if self.peek_keyword("ALL") {
            self.parse_keyword()?;
            UnionType::All
        } else {
            UnionType::Distinct
        };

        Ok(UnionClause { union_type })
    }

    /// Parse CREATE DATABASE clause
    /// Syntax: CREATE DATABASE name [IF NOT EXISTS]
    pub(super) fn parse_create_database_clause(&mut self) -> Result<CreateDatabaseClause> {
        self.expect_keyword("DATABASE")?;
        self.skip_whitespace();

        // Check for IF NOT EXISTS
        let if_not_exists = if self.peek_keyword("IF") {
            self.parse_keyword()?; // consume "IF"
            self.expect_keyword("NOT")?;
            self.expect_keyword("EXISTS")?;
            self.skip_whitespace();
            true
        } else {
            false
        };

        let name = self.parse_identifier()?;
        Ok(CreateDatabaseClause {
            name,
            if_not_exists,
        })
    }

    /// Parse DROP DATABASE clause
    /// Syntax: DROP DATABASE name [IF EXISTS]
    pub(super) fn parse_drop_database_clause(&mut self) -> Result<DropDatabaseClause> {
        self.expect_keyword("DATABASE")?;
        self.skip_whitespace();

        // Check for IF EXISTS
        let if_exists = if self.peek_keyword("IF") {
            self.parse_keyword()?; // consume "IF"
            self.expect_keyword("EXISTS")?;
            self.skip_whitespace();
            true
        } else {
            false
        };

        let name = self.parse_identifier()?;
        Ok(DropDatabaseClause { name, if_exists })
    }

    /// Parse ALTER DATABASE clause
    /// Syntax: ALTER DATABASE name SET ACCESS {READ WRITE | READ ONLY}
    ///         ALTER DATABASE name SET OPTION key value
    pub(super) fn parse_alter_database_clause(&mut self) -> Result<AlterDatabaseClause> {
        self.expect_keyword("DATABASE")?;
        self.skip_whitespace();

        let name = self.parse_identifier()?;
        self.skip_whitespace();

        self.expect_keyword("SET")?;
        self.skip_whitespace();

        // Parse alteration type
        let alteration = if self.peek_keyword("ACCESS") {
            self.parse_keyword()?; // consume "ACCESS"
            self.skip_whitespace();

            // Parse READ WRITE or READ ONLY
            self.expect_keyword("READ")?;
            self.skip_whitespace();

            let read_only = if self.peek_keyword("ONLY") {
                self.parse_keyword()?;
                true
            } else if self.peek_keyword("WRITE") {
                self.parse_keyword()?;
                false
            } else {
                return Err(self.error("Expected ONLY or WRITE after READ in ALTER DATABASE"));
            };

            DatabaseAlteration::SetAccess { read_only }
        } else if self.peek_keyword("OPTION") {
            self.parse_keyword()?; // consume "OPTION"
            self.skip_whitespace();

            let key = self.parse_identifier()?;
            self.skip_whitespace();

            // Parse value - can be identifier or number
            let value = if self.peek_char().map_or(false, |c| c.is_ascii_digit()) {
                // Parse as number and convert to string
                self.parse_number()?.to_string()
            } else {
                // Parse as identifier
                self.parse_identifier()?
            };

            DatabaseAlteration::SetOption { key, value }
        } else {
            return Err(self.error("Expected ACCESS or OPTION after SET in ALTER DATABASE"));
        };

        Ok(AlterDatabaseClause { name, alteration })
    }

    /// Parse USE DATABASE clause
    /// Syntax: USE DATABASE name
    pub(super) fn parse_use_database_clause(&mut self) -> Result<UseDatabaseClause> {
        self.expect_keyword("DATABASE")?;
        self.skip_whitespace();
        let name = self.parse_identifier()?;
        Ok(UseDatabaseClause { name })
    }

    /// Parse LOAD CSV clause
    /// Syntax: LOAD CSV FROM 'file:///path/to/file.csv' [WITH HEADERS] [FIELDTERMINATOR ','] AS row
    pub(super) fn parse_load_csv_clause(&mut self) -> Result<LoadCsvClause> {
        self.parse_keyword()?; // consume "CSV"
        self.skip_whitespace();

        // Parse FROM keyword
        self.expect_keyword("FROM")?;
        self.skip_whitespace();

        // Parse URL (string literal)
        let url_expr = self.parse_string_literal()?;
        let url = if let Expression::Literal(Literal::String(s)) = url_expr {
            s
        } else {
            return Err(self.error("Expected string literal for CSV file URL"));
        };

        self.skip_whitespace();

        // Parse optional WITH HEADERS
        let mut with_headers = false;
        if self.peek_keyword("WITH") {
            self.parse_keyword()?; // consume "WITH"
            self.skip_whitespace();
            if self.peek_keyword("HEADERS") {
                self.parse_keyword()?; // consume "HEADERS"
                with_headers = true;
                self.skip_whitespace();
            } else {
                return Err(self.error("WITH must be followed by HEADERS"));
            }
        }

        // Parse optional FIELDTERMINATOR
        let mut field_terminator = None;
        if self.peek_keyword("FIELDTERMINATOR") {
            self.parse_keyword()?; // consume "FIELDTERMINATOR"
            self.skip_whitespace();

            // Parse terminator character (string literal)
            let term_expr = self.parse_string_literal()?;
            let term_str = if let Expression::Literal(Literal::String(s)) = term_expr {
                s
            } else {
                return Err(self.error("Expected string literal for FIELDTERMINATOR"));
            };

            if term_str.len() == 1 {
                field_terminator = Some(term_str);
            } else {
                return Err(self.error("FIELDTERMINATOR must be a single character"));
            }

            self.skip_whitespace();
        }

        // Parse AS variable
        self.expect_keyword("AS")?;
        self.skip_whitespace();
        let variable = self.parse_identifier()?;

        Ok(LoadCsvClause {
            url,
            variable,
            with_headers,
            field_terminator,
        })
    }

    /// Parse CALL subquery clause
    /// Syntax:
    ///   `CALL [(import_var [, …])] { subquery } [IN TRANSACTIONS [OF n ROWS]]`
    ///
    /// The optional parenthesised list is the Cypher 25
    /// "scoped subquery" import list (phase6 §8): when present, only
    /// the listed outer variables are visible inside the inner
    /// subquery. The empty form `CALL () { … }` declares an inner
    /// scope with NO imports.
    pub(super) fn parse_call_subquery_clause(&mut self) -> Result<CallSubqueryClause> {
        // Optional import list — `CALL (a, b) { … }`. Note: the
        // `CALL` keyword has already been consumed by the caller.
        let import_list: Option<Vec<String>> = if self.peek_char() == Some('(') {
            self.consume_char();
            self.skip_whitespace();
            let mut vars: Vec<String> = Vec::new();
            // `()` is permitted (no imports).
            if self.peek_char() != Some(')') {
                loop {
                    self.skip_whitespace();
                    let ident = self.parse_identifier()?;
                    vars.push(ident);
                    self.skip_whitespace();
                    if self.peek_char() == Some(',') {
                        self.consume_char();
                        continue;
                    }
                    break;
                }
            }
            self.skip_whitespace();
            self.expect_char(')')?;
            self.skip_whitespace();
            Some(vars)
        } else {
            None
        };

        // Expect opening brace
        self.expect_char('{')?;
        self.skip_whitespace();

        // Parse the subquery (nested CypherQuery)
        let mut clauses = Vec::new();

        // Parse clauses until we find the closing brace
        while self.pos < self.input.len() {
            self.skip_whitespace();

            // Check for closing brace
            if self.peek_char() == Some('}') {
                self.consume_char();
                break;
            }

            // Check if this is a clause boundary
            if self.is_clause_boundary() {
                let clause = self.parse_clause(clauses.last())?;
                clauses.push(clause);
            } else {
                // No more clauses
                break;
            }
        }

        if clauses.is_empty() {
            return Err(self.error("CALL subquery must contain at least one clause"));
        }

        let query = CypherQuery {
            clauses,
            params: std::collections::HashMap::new(),
            graph_scope: None,
        };

        // Check for IN TRANSACTIONS / IN CONCURRENT TRANSACTIONS
        // with optional OF N ROWS / REPORT STATUS AS var / ON ERROR
        // clauses (phase6_opencypher-subquery-transactions).
        self.skip_whitespace();
        let (in_transactions, batch_size, concurrency, status_var, on_error) =
            if self.peek_keyword("IN") {
                self.parse_keyword()?; // consume "IN"
                self.skip_whitespace();
                // `IN CONCURRENT TRANSACTIONS` is parsed the same way as
                // `IN TRANSACTIONS` but sets `concurrency` to a default
                // of 1 until `ON ERROR` / worker-count extensions land.
                let concurrent = if self.peek_keyword("CONCURRENT") {
                    self.parse_keyword()?;
                    self.skip_whitespace();
                    true
                } else {
                    false
                };
                self.expect_keyword("TRANSACTIONS")?;
                self.skip_whitespace();

                let mut batch: Option<usize> = None;
                let mut status: Option<String> = None;
                let mut err_policy: OnErrorPolicy = OnErrorPolicy::Fail;

                // Parse the OF / REPORT STATUS / ON ERROR suffix
                // clauses in any order. Neo4j accepts arbitrary
                // ordering so we accept the same.
                loop {
                    self.skip_whitespace();
                    if self.peek_keyword("OF") {
                        self.parse_keyword()?; // OF
                        self.skip_whitespace();
                        let raw = self.parse_number()?;
                        if raw <= 0 {
                            return Err(self.error(
                                "ERR_CALL_IN_TX_INVALID_BATCH: OF <N> ROWS requires \
                                 a positive integer",
                            ));
                        }
                        batch = Some(raw as usize);
                        self.skip_whitespace();
                        if self.peek_keyword("ROWS") {
                            self.parse_keyword()?;
                        } else if self.peek_keyword("ROW") {
                            self.parse_keyword()?;
                        } else {
                            return Err(self.error(
                                "ERR_CALL_IN_TX_INVALID_BATCH: OF <N> must be \
                                 followed by ROWS / ROW",
                            ));
                        }
                        continue;
                    }
                    if self.peek_keyword("REPORT") {
                        self.parse_keyword()?; // REPORT
                        self.skip_whitespace();
                        self.expect_keyword("STATUS")?;
                        self.skip_whitespace();
                        self.expect_keyword("AS")?;
                        self.skip_whitespace();
                        status = Some(self.parse_identifier()?);
                        continue;
                    }
                    if self.peek_keyword("ON") {
                        self.parse_keyword()?; // ON
                        self.skip_whitespace();
                        self.expect_keyword("ERROR")?;
                        self.skip_whitespace();
                        if self.peek_keyword("CONTINUE") {
                            self.parse_keyword()?;
                            err_policy = OnErrorPolicy::Continue;
                        } else if self.peek_keyword("BREAK") {
                            self.parse_keyword()?;
                            err_policy = OnErrorPolicy::Break;
                        } else if self.peek_keyword("FAIL") {
                            self.parse_keyword()?;
                            err_policy = OnErrorPolicy::Fail;
                        } else if self.peek_keyword("RETRY") {
                            self.parse_keyword()?;
                            self.skip_whitespace();
                            let raw = self.parse_number()?;
                            if raw <= 0 {
                                return Err(self.error(
                                    "ERR_CALL_IN_TX_INVALID_RETRY: RETRY <N> \
                                     requires a positive integer",
                                ));
                            }
                            err_policy = OnErrorPolicy::Retry {
                                max_attempts: raw as usize,
                            };
                        } else {
                            return Err(self.error(
                                "ERR_CALL_IN_TX_UNKNOWN_ON_ERROR: expected \
                                 CONTINUE / BREAK / FAIL / RETRY",
                            ));
                        }
                        continue;
                    }
                    break;
                }

                // `IN CONCURRENT TRANSACTIONS` flag → `Some(0)` sentinel
                // meaning "use the executor's `cypher_concurrency`
                // config knob (default 4)". The serial variant remains
                // `None`. The executor resolves the sentinel at
                // dispatch time so the planner does not need to read
                // ExecutorConfig.
                let concurrency = if concurrent { Some(0) } else { None };
                (true, batch, concurrency, status, err_policy)
            } else {
                (false, None, None, None, OnErrorPolicy::Fail)
            };

        // Validation — §2 of the task spec.
        // §2.2 inner subquery must be non-empty (already checked
        // above), §2.3 RETURN in inner is forbidden when REPORT
        // STATUS is set.
        if status_var.is_some() {
            let has_return = query.clauses.iter().any(|c| matches!(c, Clause::Return(_)));
            if has_return {
                return Err(self.error(
                    "ERR_CALL_IN_TX_RETURN_WITH_STATUS: the inner subquery \
                     cannot declare RETURN when REPORT STATUS AS <var> is set",
                ));
            }
        }

        Ok(CallSubqueryClause {
            query,
            in_transactions,
            batch_size,
            concurrency,
            on_error,
            status_var,
            import_list,
        })
    }

    /// Parse CALL procedure clause
    /// Syntax: CALL procedure.name(arg1, arg2, ...) [YIELD column1, column2, ...]
    pub(super) fn parse_call_procedure_clause(&mut self) -> Result<CallProcedureClause> {
        // Parse procedure name (can contain dots, e.g., "gds.shortestPath.dijkstra")
        let mut procedure_name = String::new();

        // Parse first identifier part
        if !self.is_identifier_start() {
            return Err(self.error("Invalid procedure name"));
        }

        let start = self.pos;
        self.consume_char();
        while self.pos < self.input.len() && self.is_identifier_char() {
            self.consume_char();
        }
        procedure_name.push_str(&self.input[start..self.pos]);

        // Continue parsing dots and identifiers
        while self.pos < self.input.len() {
            if self.peek_char() == Some('.') {
                procedure_name.push('.');
                self.consume_char(); // consume '.'

                // Parse next identifier part
                if !self.is_identifier_start() {
                    return Err(self.error("Invalid procedure name after dot"));
                }
                let part_start = self.pos;
                self.consume_char();
                while self.pos < self.input.len() && self.is_identifier_char() {
                    self.consume_char();
                }
                procedure_name.push_str(&self.input[part_start..self.pos]);
            } else if self.peek_char() == Some('(')
                || matches!(self.peek_char(), Some(c) if c.is_whitespace())
            {
                break;
            } else {
                return Err(self.error("Invalid character in procedure name"));
            }
        }

        if procedure_name.is_empty() {
            return Err(self.error("Procedure name cannot be empty"));
        }

        // Parse arguments
        self.skip_whitespace();
        self.expect_char('(')?;
        self.skip_whitespace();

        let mut arguments = Vec::new();

        // Check for empty argument list
        if self.peek_char() != Some(')') {
            loop {
                // Parse argument expression
                let arg = self.parse_expression()?;
                arguments.push(arg);

                self.skip_whitespace();

                if self.peek_char() == Some(',') {
                    self.consume_char();
                    self.skip_whitespace();
                } else if self.peek_char() == Some(')') {
                    break;
                } else {
                    return Err(self.error("Expected ',' or ')' in procedure arguments"));
                }
            }
        }

        self.expect_char(')')?;
        self.skip_whitespace();

        // Parse optional YIELD clause
        let yield_columns = if self.peek_keyword("YIELD") {
            self.parse_keyword()?; // consume "YIELD"
            self.skip_whitespace();

            // phase6 §3.4 — `YIELD *` means "project every column the
            // procedure declares". The downstream `execute_call_procedure`
            // already treats `yield_columns = None` as "use all columns",
            // so we short-circuit to None instead of listing them.
            if self.peek_char() == Some('*') {
                self.consume_char();
                self.skip_whitespace();
                None
            } else {
                let mut columns = Vec::new();
                loop {
                    let column = self.parse_identifier()?;
                    columns.push(column);

                    self.skip_whitespace();

                    if self.peek_char() == Some(',') {
                        self.consume_char();
                        self.skip_whitespace();
                    } else {
                        break;
                    }
                }
                Some(columns)
            }
        } else {
            None
        };

        Ok(CallProcedureClause {
            procedure_name,
            arguments,
            yield_columns,
        })
    }

    /// Parse CREATE INDEX clause
    /// Syntax: CREATE [OR REPLACE] [SPATIAL] INDEX [IF NOT EXISTS] ON :Label(property)
    pub(super) fn parse_create_index_clause(&mut self) -> Result<CreateIndexClause> {
        // Check for OR REPLACE before INDEX
        let or_replace = if self.peek_keyword("OR") {
            self.parse_keyword()?; // consume "OR"
            self.expect_keyword("REPLACE")?;
            self.skip_whitespace();
            true
        } else {
            false
        };

        // Check for SPATIAL keyword
        let index_type = if self.peek_keyword("SPATIAL") {
            self.parse_keyword()?; // consume "SPATIAL"
            self.skip_whitespace();
            Some("spatial".to_string())
        } else {
            None
        };

        self.expect_keyword("INDEX")?;
        self.skip_whitespace();

        // phase6_opencypher-advanced-types §3 — optional index name
        // between `INDEX` and `FOR`, e.g.
        // `CREATE INDEX person_id FOR (p:Person) ON (p.tenantId, p.id)`.
        let name = if self.is_identifier_start()
            && !self.peek_keyword("IF")
            && !self.peek_keyword("FOR")
            && !self.peek_keyword("ON")
        {
            Some(self.parse_identifier()?)
        } else {
            None
        };
        self.skip_whitespace();

        // Check for IF NOT EXISTS
        let if_not_exists = if self.peek_keyword("IF") {
            self.parse_keyword()?; // consume "IF"
            self.expect_keyword("NOT")?;
            self.expect_keyword("EXISTS")?;
            self.skip_whitespace();
            true
        } else {
            false
        };

        // Two grammar shapes:
        //   legacy : ON :Label(property)
        //   modern : FOR (var:Label) ON (var.p1, var.p2, ...)
        // The modern form is the only one that supports composite
        // property lists (§3.6). The legacy form is kept for every
        // existing test and SDK call site that already emits it.
        let (label, properties) = if self.peek_keyword("FOR") {
            self.parse_keyword()?; // consume "FOR"
            self.skip_whitespace();
            self.expect_char('(')?;
            self.skip_whitespace();
            let var = self.parse_identifier()?;
            self.skip_whitespace();
            self.expect_char(':')?;
            let lbl = self.parse_identifier()?;
            self.skip_whitespace();
            self.expect_char(')')?;
            self.skip_whitespace();
            self.expect_keyword("ON")?;
            self.skip_whitespace();
            self.expect_char('(')?;
            let mut props = Vec::new();
            loop {
                self.skip_whitespace();
                let p_var = self.parse_identifier()?;
                if p_var != var {
                    return Err(self.error(&format!(
                        "CREATE INDEX: property prefix {p_var:?} does not match pattern variable \
                         {var:?}"
                    )));
                }
                self.expect_char('.')?;
                let prop = self.parse_identifier()?;
                props.push(prop);
                self.skip_whitespace();
                if self.peek_char() == Some(',') {
                    self.consume_char();
                    continue;
                }
                break;
            }
            self.skip_whitespace();
            self.expect_char(')')?;
            (lbl, props)
        } else {
            self.expect_keyword("ON")?;
            self.skip_whitespace();
            self.expect_char(':')?;
            let lbl = self.parse_identifier()?;
            self.skip_whitespace();
            self.expect_char('(')?;
            let prop = self.parse_identifier()?;
            self.expect_char(')')?;
            (lbl, vec![prop])
        };

        let property = properties.first().cloned().unwrap_or_default();

        Ok(CreateIndexClause {
            name,
            label,
            property,
            properties,
            if_not_exists,
            or_replace,
            index_type,
        })
    }

    /// Parse DROP INDEX clause
    /// Syntax: DROP INDEX [IF EXISTS] ON :Label(property)
    pub(super) fn parse_drop_index_clause(&mut self) -> Result<DropIndexClause> {
        self.expect_keyword("INDEX")?;
        self.skip_whitespace();

        // Check for IF EXISTS
        let if_exists = if self.peek_keyword("IF") {
            self.parse_keyword()?; // consume "IF"
            self.expect_keyword("EXISTS")?;
            self.skip_whitespace();
            true
        } else {
            false
        };

        self.expect_keyword("ON")?;
        self.skip_whitespace();
        self.expect_char(':')?;
        let label = self.parse_identifier()?;
        self.skip_whitespace();
        self.expect_char('(')?;
        let property = self.parse_identifier()?;
        self.expect_char(')')?;

        Ok(DropIndexClause {
            label,
            property,
            if_exists,
        })
    }

    /// Parse CREATE CONSTRAINT clause.
    ///
    /// Accepted forms:
    ///
    /// ```text
    /// // Legacy (Cypher 4.x):
    /// CREATE CONSTRAINT [IF NOT EXISTS] ON (n:Label) ASSERT n.p IS UNIQUE
    /// CREATE CONSTRAINT [IF NOT EXISTS] ON (n:Label) ASSERT n.p IS NOT NULL
    /// CREATE CONSTRAINT [IF NOT EXISTS] ON (n:Label) ASSERT EXISTS(n.p)
    ///
    /// // Cypher 25 — node scope:
    /// CREATE CONSTRAINT [<name>] [IF NOT EXISTS]
    ///     FOR (n:Label) REQUIRE n.p IS UNIQUE
    /// CREATE CONSTRAINT [<name>] [IF NOT EXISTS]
    ///     FOR (n:Label) REQUIRE n.p IS NOT NULL
    /// CREATE CONSTRAINT [<name>] [IF NOT EXISTS]
    ///     FOR (n:Label) REQUIRE (n.p1, n.p2, ...) IS NODE KEY
    /// CREATE CONSTRAINT [<name>] [IF NOT EXISTS]
    ///     FOR (n:Label) REQUIRE n.p IS :: INTEGER   // or FLOAT / STRING / BOOLEAN / BYTES / LIST / MAP
    ///
    /// // Cypher 25 — relationship scope:
    /// CREATE CONSTRAINT [<name>] [IF NOT EXISTS]
    ///     FOR ()-[r:TYPE]-() REQUIRE r.p IS NOT NULL
    /// CREATE CONSTRAINT [<name>] [IF NOT EXISTS]
    ///     FOR ()-[r:TYPE]-() REQUIRE r.p IS :: INTEGER
    /// ```
    pub(super) fn parse_create_constraint_clause(&mut self) -> Result<CreateConstraintClause> {
        self.expect_keyword("CONSTRAINT")?;
        self.skip_whitespace();

        // Optional constraint name (`CREATE CONSTRAINT <name> [IF NOT EXISTS] FOR ...`).
        // Only legal before `IF`, `FOR`, or `ON`. Identifiers that
        // collide with a keyword are handled by the keyword checks.
        let name = if self.is_identifier_start()
            && !self.peek_keyword("IF")
            && !self.peek_keyword("FOR")
            && !self.peek_keyword("ON")
        {
            Some(self.parse_identifier()?)
        } else {
            None
        };
        self.skip_whitespace();

        // IF NOT EXISTS
        let if_not_exists = if self.peek_keyword("IF") {
            self.parse_keyword()?; // IF
            self.expect_keyword("NOT")?;
            self.expect_keyword("EXISTS")?;
            self.skip_whitespace();
            true
        } else {
            false
        };

        if self.peek_keyword("FOR") {
            self.parse_create_constraint_for_form(name, if_not_exists)
        } else {
            // Legacy `ON (n:L) ASSERT ...` form — every output here
            // is a node-scope constraint.
            self.expect_keyword("ON")?;
            self.skip_whitespace();
            self.expect_char('(')?;
            let _variable = self.parse_identifier()?;
            self.skip_whitespace();
            self.expect_char(':')?;
            let label = self.parse_identifier()?;
            self.expect_char(')')?;
            self.skip_whitespace();
            self.expect_keyword("ASSERT")?;
            self.skip_whitespace();
            let (constraint_type, property) = self.parse_legacy_constraint_body()?;
            Ok(CreateConstraintClause {
                name,
                constraint_type,
                label,
                property: property.clone(),
                properties: vec![property],
                entity: ConstraintEntity::Node,
                property_type: None,
                if_not_exists,
            })
        }
    }

    /// Legacy `ASSERT n.p IS UNIQUE / IS NOT NULL / EXISTS(n.p)` body.
    fn parse_legacy_constraint_body(&mut self) -> Result<(ConstraintType, String)> {
        if self.peek_keyword("EXISTS") {
            self.parse_keyword()?;
            self.expect_char('(')?;
            let _var = self.parse_identifier()?;
            self.expect_char('.')?;
            let prop = self.parse_identifier()?;
            self.expect_char(')')?;
            return Ok((ConstraintType::Exists, prop));
        }
        let _var = self.parse_identifier()?;
        self.expect_char('.')?;
        let prop = self.parse_identifier()?;
        self.skip_whitespace();
        self.expect_keyword("IS")?;
        self.skip_whitespace();
        if self.peek_keyword("NOT") {
            self.parse_keyword()?;
            self.skip_whitespace();
            self.expect_keyword("NULL")?;
            Ok((ConstraintType::Exists, prop))
        } else {
            self.expect_keyword("UNIQUE")?;
            Ok((ConstraintType::Unique, prop))
        }
    }

    /// Cypher 25 `FOR (n:L) REQUIRE ...` and
    /// `FOR ()-[r:T]-() REQUIRE ...` forms.
    fn parse_create_constraint_for_form(
        &mut self,
        name: Option<String>,
        if_not_exists: bool,
    ) -> Result<CreateConstraintClause> {
        self.expect_keyword("FOR")?;
        self.skip_whitespace();

        // Entity scope: node pattern `(n:L)` or rel pattern `()-[r:T]-()`.
        let (entity, var_name, label_or_type) =
            if self.peek_char() == Some('(') && !self.peek_is_rel_after_lparen() {
                // Actually look at next char to decide. Both forms start with `(`:
                //   node pattern:  (n:L)
                //   rel pattern:   ()-[r:T]-()
                // We disambiguate by peeking past `(` for `)-[`.
                self.parse_constraint_node_pattern()?
            } else {
                self.parse_constraint_rel_pattern()?
            };
        let _ = var_name;

        self.skip_whitespace();
        self.expect_keyword("REQUIRE")?;
        self.skip_whitespace();

        // Body: `(p1, p2, ...) IS NODE KEY` | `n.p IS UNIQUE` |
        //       `n.p IS NOT NULL` | `n.p IS :: TYPE`.
        let (constraint_type, properties, property_type) =
            if self.peek_char() == Some('(') && self.peek_is_node_key_tuple() {
                self.parse_require_node_key_body()?
            } else {
                let _var = self.parse_identifier()?;
                self.expect_char('.')?;
                let prop = self.parse_identifier()?;
                self.skip_whitespace();
                self.expect_keyword("IS")?;
                self.skip_whitespace();
                if self.peek_keyword("NOT") {
                    self.parse_keyword()?;
                    self.skip_whitespace();
                    self.expect_keyword("NULL")?;
                    (ConstraintType::Exists, vec![prop], None)
                } else if self.peek_char() == Some(':') && self.peek_char_at(1) == Some(':') {
                    self.consume_char();
                    self.consume_char();
                    self.skip_whitespace();
                    let ty = self.parse_identifier()?;
                    (ConstraintType::PropertyType, vec![prop], Some(ty))
                } else {
                    self.expect_keyword("UNIQUE")?;
                    (ConstraintType::Unique, vec![prop], None)
                }
            };

        let property = properties.first().cloned().unwrap_or_default();
        Ok(CreateConstraintClause {
            name,
            constraint_type,
            label: label_or_type,
            property,
            properties,
            entity,
            property_type,
            if_not_exists,
        })
    }

    /// Look past `(` to decide if the pattern is a node `(n:L)` or a
    /// relationship `()-[r:T]-()`. Stateless — `self.pos` is
    /// unchanged on return.
    fn peek_is_rel_after_lparen(&self) -> bool {
        let mut pos = self.pos + 1;
        // Skip whitespace inside `(`.
        while pos < self.input.len() {
            if !self.input.as_bytes()[pos].is_ascii_whitespace() {
                break;
            }
            pos += 1;
        }
        // Rel pattern shape: `()-[...`
        pos < self.input.len() && self.input.as_bytes()[pos] == b')'
    }

    /// Look past `(` to decide if we're at a NODE KEY tuple
    /// `(n.p1, n.p2)` vs a single `n.p` wrapped in parens. Heuristic:
    /// after the first `.`, a comma before the closing paren implies
    /// a tuple.
    fn peek_is_node_key_tuple(&self) -> bool {
        let mut depth = 0i32;
        for i in self.pos..self.input.len() {
            let b = self.input.as_bytes()[i];
            match b {
                b'(' => depth += 1,
                b')' => {
                    depth -= 1;
                    if depth == 0 {
                        return false;
                    }
                }
                b',' if depth == 1 => return true,
                _ => {}
            }
        }
        false
    }

    fn parse_constraint_node_pattern(&mut self) -> Result<(ConstraintEntity, String, String)> {
        self.expect_char('(')?;
        self.skip_whitespace();
        let var = self.parse_identifier()?;
        self.skip_whitespace();
        self.expect_char(':')?;
        let label = self.parse_identifier()?;
        self.skip_whitespace();
        self.expect_char(')')?;
        Ok((ConstraintEntity::Node, var, label))
    }

    fn parse_constraint_rel_pattern(&mut self) -> Result<(ConstraintEntity, String, String)> {
        // Accepts `()-[r:TYPE]-()` and `()-[r:TYPE]->()`.
        self.expect_char('(')?;
        self.skip_whitespace();
        self.expect_char(')')?;
        self.skip_whitespace();
        self.expect_char('-')?;
        self.skip_whitespace();
        self.expect_char('[')?;
        self.skip_whitespace();
        let var = self.parse_identifier()?;
        self.skip_whitespace();
        self.expect_char(':')?;
        let rel_type = self.parse_identifier()?;
        self.skip_whitespace();
        self.expect_char(']')?;
        self.skip_whitespace();
        self.expect_char('-')?;
        self.skip_whitespace();
        if self.peek_char() == Some('>') {
            self.consume_char();
            self.skip_whitespace();
        }
        self.expect_char('(')?;
        self.skip_whitespace();
        self.expect_char(')')?;
        Ok((ConstraintEntity::Relationship, var, rel_type))
    }

    fn parse_require_node_key_body(
        &mut self,
    ) -> Result<(ConstraintType, Vec<String>, Option<String>)> {
        self.expect_char('(')?;
        let mut props = Vec::new();
        loop {
            self.skip_whitespace();
            let _var = self.parse_identifier()?;
            self.expect_char('.')?;
            props.push(self.parse_identifier()?);
            self.skip_whitespace();
            if self.peek_char() == Some(',') {
                self.consume_char();
                continue;
            }
            break;
        }
        self.skip_whitespace();
        self.expect_char(')')?;
        self.skip_whitespace();
        self.expect_keyword("IS")?;
        self.skip_whitespace();
        self.expect_keyword("NODE")?;
        self.skip_whitespace();
        self.expect_keyword("KEY")?;
        Ok((ConstraintType::NodeKey, props, None))
    }

    /// Parse DROP CONSTRAINT clause
    /// Syntax: DROP CONSTRAINT [IF EXISTS] ON (n:Label) ASSERT n.property IS UNIQUE
    pub(super) fn parse_drop_constraint_clause(&mut self) -> Result<DropConstraintClause> {
        self.expect_keyword("CONSTRAINT")?;
        self.skip_whitespace();

        // Check for IF EXISTS
        let if_exists = if self.peek_keyword("IF") {
            self.parse_keyword()?; // consume "IF"
            self.expect_keyword("EXISTS")?;
            self.skip_whitespace();
            true
        } else {
            false
        };

        self.expect_keyword("ON")?;
        self.skip_whitespace();
        self.expect_char('(')?;
        let _variable = self.parse_identifier()?;
        self.skip_whitespace();
        self.expect_char(':')?;
        let label = self.parse_identifier()?;
        self.expect_char(')')?;
        self.skip_whitespace();
        self.expect_keyword("ASSERT")?;
        self.skip_whitespace();

        // Parse constraint type and extract property name (same as CREATE).
        // Accepts `IS UNIQUE`, `IS NOT NULL`, and the legacy `EXISTS(n.p)`.
        let (constraint_type, property) = if self.peek_keyword("EXISTS") {
            self.parse_keyword()?;
            self.expect_char('(')?;
            let _var = self.parse_identifier()?;
            self.expect_char('.')?;
            let prop = self.parse_identifier()?;
            self.expect_char(')')?;
            (ConstraintType::Exists, prop)
        } else {
            self.parse_identifier()?; // variable
            self.expect_char('.')?;
            let prop = self.parse_identifier()?;
            self.skip_whitespace();
            self.expect_keyword("IS")?;
            self.skip_whitespace();
            if self.peek_keyword("NOT") {
                self.parse_keyword()?;
                self.skip_whitespace();
                self.expect_keyword("NULL")?;
                (ConstraintType::Exists, prop)
            } else {
                self.expect_keyword("UNIQUE")?;
                (ConstraintType::Unique, prop)
            }
        };

        Ok(DropConstraintClause {
            constraint_type,
            label,
            property,
            if_exists,
        })
    }

    /// Parse CREATE USER clause
    /// Syntax: CREATE USER username [SET PASSWORD 'password'] [IF NOT EXISTS]
    pub(super) fn parse_create_user_clause(&mut self) -> Result<CreateUserClause> {
        self.expect_keyword("USER")?;
        self.skip_whitespace();

        // Check for IF NOT EXISTS first (it can come before username in some dialects)
        let mut if_not_exists = false;
        if self.peek_keyword("IF") {
            self.parse_keyword()?;
            self.expect_keyword("NOT")?;
            self.expect_keyword("EXISTS")?;
            self.skip_whitespace();
            if_not_exists = true;
        }

        let username = self.parse_identifier()?;
        self.skip_whitespace();

        // Check for SET PASSWORD
        let password = if self.peek_keyword("SET") {
            self.parse_keyword()?;
            self.expect_keyword("PASSWORD")?;
            self.skip_whitespace();
            let pwd_expr = self.parse_string_literal()?;
            // Extract string value from Expression::Literal(Literal::String)
            if let Expression::Literal(crate::executor::parser::Literal::String(s)) = pwd_expr {
                Some(s)
            } else {
                return Err(self.error("PASSWORD must be a string literal"));
            }
        } else {
            None
        };

        // Check for IF NOT EXISTS after username
        if !if_not_exists && self.peek_keyword("IF") {
            self.parse_keyword()?;
            self.expect_keyword("NOT")?;
            self.expect_keyword("EXISTS")?;
            if_not_exists = true;
        }

        Ok(CreateUserClause {
            username,
            password,
            if_not_exists,
        })
    }

    /// Parse DROP USER clause
    /// Syntax: DROP USER username [IF EXISTS]
    pub(super) fn parse_drop_user_clause(&mut self) -> Result<DropUserClause> {
        self.expect_keyword("USER")?;
        self.skip_whitespace();

        // Check for IF EXISTS first
        let mut if_exists = false;
        if self.peek_keyword("IF") {
            self.parse_keyword()?;
            self.expect_keyword("EXISTS")?;
            self.skip_whitespace();
            if_exists = true;
        }

        let username = self.parse_identifier()?;
        self.skip_whitespace();

        // Check for IF EXISTS after username
        if !if_exists && self.peek_keyword("IF") {
            self.parse_keyword()?;
            self.expect_keyword("EXISTS")?;
            if_exists = true;
        }

        Ok(DropUserClause {
            username,
            if_exists,
        })
    }

    /// Parse SHOW USER clause
    /// Syntax: SHOW USER username
    pub(super) fn parse_show_user_clause(&mut self) -> Result<ShowUserClause> {
        self.expect_keyword("USER")?;
        self.skip_whitespace();
        let username = self.parse_identifier()?;
        Ok(ShowUserClause { username })
    }

    /// Parse CREATE FUNCTION clause
    /// Syntax: CREATE FUNCTION name(param1: Type1, param2: Type2) [IF NOT EXISTS] RETURNS Type [AS expression]
    /// Note: For MVP, we'll use a simplified syntax that stores the signature only
    /// The actual function implementation must be registered via API/plugin system
    pub(super) fn parse_create_function_clause(&mut self) -> Result<CreateFunctionClause> {
        self.expect_keyword("FUNCTION")?;
        self.skip_whitespace();

        // Check for IF NOT EXISTS
        let mut if_not_exists = false;
        if self.peek_keyword("IF") {
            self.parse_keyword()?;
            self.expect_keyword("NOT")?;
            self.expect_keyword("EXISTS")?;
            self.skip_whitespace();
            if_not_exists = true;
        }

        // Parse function name
        let name = self.parse_identifier()?;
        self.skip_whitespace();

        // Parse parameters: (param1: Type1, param2: Type2)
        let mut parameters = Vec::new();
        self.expect_char('(')?;
        self.skip_whitespace();

        if self.peek_char() != Some(')') {
            loop {
                let param_name = self.parse_identifier()?;
                self.skip_whitespace();
                self.expect_char(':')?;
                self.skip_whitespace();

                // Parse parameter type
                let type_str = self.parse_identifier()?;
                let param_type = match type_str.to_lowercase().as_str() {
                    "integer" | "int" => crate::udf::UdfReturnType::Integer,
                    "float" | "double" => crate::udf::UdfReturnType::Float,
                    "string" | "str" => crate::udf::UdfReturnType::String,
                    "boolean" | "bool" => crate::udf::UdfReturnType::Boolean,
                    "any" => crate::udf::UdfReturnType::Any,
                    _ => {
                        return Err(self.error(&format!("Unknown parameter type: {}", type_str)));
                    }
                };

                parameters.push(UdfParameter {
                    name: param_name,
                    param_type: param_type.clone(),
                    required: true, // For MVP, all parameters are required
                    default: None,
                });

                self.skip_whitespace();
                if self.peek_char() == Some(',') {
                    self.consume_char();
                    self.skip_whitespace();
                } else if self.peek_char() == Some(')') {
                    break;
                } else {
                    return Err(self.error("Expected ',' or ')' in function parameters"));
                }
            }
        }

        self.expect_char(')')?;
        self.skip_whitespace();

        // Parse RETURNS type
        self.expect_keyword("RETURNS")?;
        self.skip_whitespace();
        let return_type_str = self.parse_identifier()?;
        let return_type = match return_type_str.to_lowercase().as_str() {
            "integer" | "int" => crate::udf::UdfReturnType::Integer,
            "float" | "double" => crate::udf::UdfReturnType::Float,
            "string" | "str" => crate::udf::UdfReturnType::String,
            "boolean" | "bool" => crate::udf::UdfReturnType::Boolean,
            "any" => crate::udf::UdfReturnType::Any,
            _ => {
                return Err(self.error(&format!("Unknown return type: {}", return_type_str)));
            }
        };

        self.skip_whitespace();

        // Parse optional description (AS 'description')
        let mut description = None;
        if self.peek_keyword("AS") {
            self.parse_keyword()?;
            self.skip_whitespace();
            if self.peek_char() == Some('\'') || self.peek_char() == Some('"') {
                let desc_str = self.parse_string_literal()?;
                if let Expression::Literal(crate::executor::parser::Literal::String(s)) = desc_str {
                    description = Some(s);
                }
            }
        }

        Ok(CreateFunctionClause {
            name,
            parameters,
            return_type,
            if_not_exists,
            description,
        })
    }

    /// Parse DROP FUNCTION clause
    /// Syntax: DROP FUNCTION name [IF EXISTS]
    pub(super) fn parse_drop_function_clause(&mut self) -> Result<DropFunctionClause> {
        self.expect_keyword("FUNCTION")?;
        self.skip_whitespace();

        // Check for IF EXISTS
        let mut if_exists = false;
        if self.peek_keyword("IF") {
            self.parse_keyword()?;
            self.expect_keyword("EXISTS")?;
            self.skip_whitespace();
            if_exists = true;
        }

        let name = self.parse_identifier()?;
        self.skip_whitespace();

        // Check for IF EXISTS after function name
        if !if_exists && self.peek_keyword("IF") {
            self.parse_keyword()?;
            self.expect_keyword("EXISTS")?;
            if_exists = true;
        }

        Ok(DropFunctionClause { name, if_exists })
    }

    /// Parse CREATE API KEY clause
    /// Syntax: CREATE API KEY name [FOR username] [WITH PERMISSIONS ...] [EXPIRES IN 'duration']
    pub(super) fn parse_create_api_key_clause(&mut self) -> Result<CreateApiKeyClause> {
        self.skip_whitespace();
        let name = self.parse_identifier()?;
        self.skip_whitespace();

        let mut user_id = None;
        let mut permissions = Vec::new();
        let mut expires_in = None;

        // Parse optional FOR username
        if self.peek_keyword("FOR") {
            self.parse_keyword()?;
            self.skip_whitespace();
            user_id = Some(self.parse_identifier()?);
            self.skip_whitespace();
        }

        // Parse optional WITH PERMISSIONS
        if self.peek_keyword("WITH") {
            self.parse_keyword()?;
            self.expect_keyword("PERMISSIONS")?;
            self.skip_whitespace();
            loop {
                let permission = self.parse_identifier()?;
                permissions.push(permission);
                self.skip_whitespace();
                if self.peek_char() == Some(',') {
                    self.consume_char();
                    self.skip_whitespace();
                } else {
                    break;
                }
            }
        }

        // Parse optional EXPIRES IN
        if self.peek_keyword("EXPIRES") {
            self.parse_keyword()?;
            self.expect_keyword("IN")?;
            self.skip_whitespace();
            let duration_str = self.parse_string_literal()?;
            if let Expression::Literal(crate::executor::parser::Literal::String(s)) = duration_str {
                expires_in = Some(s);
            }
        }

        Ok(CreateApiKeyClause {
            name,
            user_id,
            permissions,
            expires_in,
        })
    }

    /// Parse SHOW API KEYS clause
    /// Syntax: SHOW API KEYS [FOR username]
    pub(super) fn parse_show_api_keys_clause(&mut self) -> Result<ShowApiKeysClause> {
        self.skip_whitespace();
        let mut user_id = None;

        if self.peek_keyword("FOR") {
            self.parse_keyword()?;
            self.skip_whitespace();
            user_id = Some(self.parse_identifier()?);
        }

        Ok(ShowApiKeysClause { user_id })
    }

    /// Parse REVOKE API KEY clause
    /// Syntax: REVOKE API KEY 'key_id' [REASON 'reason']
    pub(super) fn parse_revoke_api_key_clause(&mut self) -> Result<RevokeApiKeyClause> {
        self.skip_whitespace();
        let key_id_str = self.parse_string_literal()?;
        let key_id =
            if let Expression::Literal(crate::executor::parser::Literal::String(s)) = key_id_str {
                s
            } else {
                return Err(self.error("API key ID must be a string literal"));
            };

        self.skip_whitespace();
        let mut reason = None;

        if self.peek_keyword("REASON") {
            self.parse_keyword()?;
            self.skip_whitespace();
            let reason_str = self.parse_string_literal()?;
            if let Expression::Literal(crate::executor::parser::Literal::String(s)) = reason_str {
                reason = Some(s);
            }
        }

        Ok(RevokeApiKeyClause { key_id, reason })
    }

    /// Parse DELETE API KEY clause
    /// Syntax: DELETE API KEY 'key_id'
    pub(super) fn parse_delete_api_key_clause(&mut self) -> Result<DeleteApiKeyClause> {
        self.skip_whitespace();
        let key_id_str = self.parse_string_literal()?;
        let key_id =
            if let Expression::Literal(crate::executor::parser::Literal::String(s)) = key_id_str {
                s
            } else {
                return Err(self.error("API key ID must be a string literal"));
            };

        Ok(DeleteApiKeyClause { key_id })
    }

    /// Parse GRANT clause
    /// Syntax: GRANT permission [, permission ...] TO target
    pub(super) fn parse_grant_clause(&mut self) -> Result<GrantClause> {
        self.skip_whitespace();

        // Parse permissions
        let mut permissions = Vec::new();
        loop {
            let permission = self.parse_identifier()?;
            permissions.push(permission);
            self.skip_whitespace();
            if self.peek_char() == Some(',') {
                self.consume_char();
                self.skip_whitespace();
            } else {
                break;
            }
        }

        self.expect_keyword("TO")?;
        self.skip_whitespace();
        let target = self.parse_identifier()?;

        Ok(GrantClause {
            permissions,
            target,
        })
    }

    /// Parse EXPLAIN clause
    /// Syntax: EXPLAIN [query]
    pub(super) fn parse_explain_clause(&mut self) -> Result<Clause> {
        self.parse_keyword()?; // consume "EXPLAIN"
        self.skip_whitespace();

        // Save current position to extract query string
        let start_pos = self.pos;

        // Parse the remaining query directly using the current parser state
        // This is more reliable than creating a temporary parser
        let mut clauses = Vec::new();

        // Parse clauses until end of input
        // We need to parse clauses without checking for EXPLAIN/PROFILE again
        // since we're already inside an EXPLAIN clause
        while self.pos < self.input.len() {
            if self.is_clause_boundary() {
                // Parse clause but skip EXPLAIN/PROFILE detection
                // We'll manually check for the clause type
                let keyword = self.parse_keyword()?;
                let clause = match keyword.to_uppercase().as_str() {
                    "MATCH" => {
                        let match_clause = self.parse_match_clause()?;
                        Clause::Match(match_clause)
                    }
                    "CREATE" => {
                        self.skip_whitespace();
                        // Check for CREATE DATABASE/INDEX/CONSTRAINT/USER
                        if self.peek_keyword("DATABASE") {
                            Clause::CreateDatabase(self.parse_create_database_clause()?)
                        } else if self.peek_keyword("INDEX") {
                            Clause::CreateIndex(self.parse_create_index_clause()?)
                        } else if self.peek_keyword("CONSTRAINT") {
                            Clause::CreateConstraint(self.parse_create_constraint_clause()?)
                        } else if self.peek_keyword("USER") {
                            Clause::CreateUser(self.parse_create_user_clause()?)
                        } else {
                            Clause::Create(self.parse_create_clause()?)
                        }
                    }
                    "MERGE" => Clause::Merge(self.parse_merge_clause()?),
                    "SET" => Clause::Set(self.parse_set_clause()?),
                    "DELETE" => Clause::Delete(self.parse_delete_clause()?),
                    "DETACH" => {
                        self.expect_keyword("DELETE")?;
                        let mut delete_clause = self.parse_delete_clause()?;
                        delete_clause.detach = true;
                        Clause::Delete(delete_clause)
                    }
                    "REMOVE" => Clause::Remove(self.parse_remove_clause()?),
                    "WITH" => Clause::With(self.parse_with_clause()?),
                    "UNWIND" => Clause::Unwind(self.parse_unwind_clause()?),
                    "UNION" => {
                        self.skip_whitespace();
                        let union_type = if self.peek_keyword("ALL") {
                            self.parse_keyword()?;
                            UnionType::All
                        } else {
                            UnionType::Distinct
                        };
                        Clause::Union(UnionClause { union_type })
                    }
                    "WHERE" => {
                        if !Self::where_is_valid_after(clauses.last()) {
                            return Err(self.reject_standalone_where());
                        }
                        Clause::Where(self.parse_where_clause()?)
                    }
                    "RETURN" => Clause::Return(self.parse_return_clause()?),
                    "ORDER" => {
                        self.expect_keyword("BY")?;
                        Clause::OrderBy(self.parse_order_by_clause()?)
                    }
                    "LIMIT" => Clause::Limit(self.parse_limit_clause()?),
                    "SKIP" => Clause::Skip(self.parse_skip_clause()?),
                    "FOREACH" => Clause::Foreach(self.parse_foreach_clause()?),
                    "OPTIONAL" => {
                        self.expect_keyword("MATCH")?;
                        let mut match_clause = self.parse_match_clause()?;
                        match_clause.optional = true;
                        Clause::Match(match_clause)
                    }
                    _ => return Err(self.error(&format!("Unexpected clause: {}", keyword))),
                };
                clauses.push(clause);
                self.skip_whitespace();
                if self.pos >= self.input.len() {
                    break;
                }
            } else {
                break;
            }
        }

        // Extract the query string that was parsed
        let query_str = self.input[start_pos..self.pos].trim().to_string();

        if clauses.is_empty() {
            return Err(self.error("EXPLAIN requires a query with at least one clause"));
        }

        let query = CypherQuery {
            clauses,
            params: std::collections::HashMap::new(),
            graph_scope: None,
        };

        Ok(Clause::Explain(ExplainClause {
            query,
            query_string: Some(query_str),
        }))
    }

    /// Parse PROFILE clause
    /// Syntax: PROFILE [query]
    pub(super) fn parse_profile_clause(&mut self) -> Result<Clause> {
        self.parse_keyword()?; // consume "PROFILE"
        self.skip_whitespace();

        // Save current position to extract query string
        let start_pos = self.pos;

        // Parse the remaining query directly using the current parser state
        // This is more reliable than creating a temporary parser
        let mut clauses = Vec::new();

        // Parse clauses until end of input
        // We need to parse clauses without checking for EXPLAIN/PROFILE again
        // since we're already inside a PROFILE clause
        while self.pos < self.input.len() {
            if self.is_clause_boundary() {
                // Parse clause but skip EXPLAIN/PROFILE detection
                // We'll manually check for the clause type
                let keyword = self.parse_keyword()?;
                let clause = match keyword.to_uppercase().as_str() {
                    "MATCH" => {
                        let match_clause = self.parse_match_clause()?;
                        Clause::Match(match_clause)
                    }
                    "CREATE" => {
                        self.skip_whitespace();
                        // Check for CREATE DATABASE/INDEX/CONSTRAINT/USER
                        if self.peek_keyword("DATABASE") {
                            Clause::CreateDatabase(self.parse_create_database_clause()?)
                        } else if self.peek_keyword("INDEX") {
                            Clause::CreateIndex(self.parse_create_index_clause()?)
                        } else if self.peek_keyword("CONSTRAINT") {
                            Clause::CreateConstraint(self.parse_create_constraint_clause()?)
                        } else if self.peek_keyword("USER") {
                            Clause::CreateUser(self.parse_create_user_clause()?)
                        } else {
                            Clause::Create(self.parse_create_clause()?)
                        }
                    }
                    "MERGE" => Clause::Merge(self.parse_merge_clause()?),
                    "SET" => Clause::Set(self.parse_set_clause()?),
                    "DELETE" => Clause::Delete(self.parse_delete_clause()?),
                    "DETACH" => {
                        self.expect_keyword("DELETE")?;
                        let mut delete_clause = self.parse_delete_clause()?;
                        delete_clause.detach = true;
                        Clause::Delete(delete_clause)
                    }
                    "REMOVE" => Clause::Remove(self.parse_remove_clause()?),
                    "WITH" => Clause::With(self.parse_with_clause()?),
                    "UNWIND" => Clause::Unwind(self.parse_unwind_clause()?),
                    "UNION" => {
                        self.skip_whitespace();
                        let union_type = if self.peek_keyword("ALL") {
                            self.parse_keyword()?;
                            UnionType::All
                        } else {
                            UnionType::Distinct
                        };
                        Clause::Union(UnionClause { union_type })
                    }
                    "WHERE" => {
                        if !Self::where_is_valid_after(clauses.last()) {
                            return Err(self.reject_standalone_where());
                        }
                        Clause::Where(self.parse_where_clause()?)
                    }
                    "RETURN" => Clause::Return(self.parse_return_clause()?),
                    "ORDER" => {
                        self.expect_keyword("BY")?;
                        Clause::OrderBy(self.parse_order_by_clause()?)
                    }
                    "LIMIT" => Clause::Limit(self.parse_limit_clause()?),
                    "SKIP" => Clause::Skip(self.parse_skip_clause()?),
                    "FOREACH" => Clause::Foreach(self.parse_foreach_clause()?),
                    "OPTIONAL" => {
                        self.expect_keyword("MATCH")?;
                        let mut match_clause = self.parse_match_clause()?;
                        match_clause.optional = true;
                        Clause::Match(match_clause)
                    }
                    _ => return Err(self.error(&format!("Unexpected clause: {}", keyword))),
                };
                clauses.push(clause);
                self.skip_whitespace();
                if self.pos >= self.input.len() {
                    break;
                }
            } else {
                break;
            }
        }

        // Extract the query string that was parsed
        let query_str = self.input[start_pos..self.pos].trim().to_string();

        if clauses.is_empty() {
            return Err(self.error("PROFILE requires a query with at least one clause"));
        }

        let query = CypherQuery {
            clauses,
            params: std::collections::HashMap::new(),
            graph_scope: None,
        };

        Ok(Clause::Profile(ProfileClause {
            query,
            query_string: Some(query_str),
        }))
    }

    /// Parse REVOKE clause
    /// Syntax: REVOKE permission [, permission ...] FROM target
    pub(super) fn parse_revoke_clause(&mut self) -> Result<RevokeClause> {
        self.skip_whitespace();

        // Parse permissions
        let mut permissions = Vec::new();
        loop {
            let permission = self.parse_identifier()?;
            permissions.push(permission);
            self.skip_whitespace();
            if self.peek_char() == Some(',') {
                self.consume_char();
                self.skip_whitespace();
            } else {
                break;
            }
        }

        self.expect_keyword("FROM")?;
        self.skip_whitespace();
        let target = self.parse_identifier()?;

        Ok(RevokeClause {
            permissions,
            target,
        })
    }
}
