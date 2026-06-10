//! Subquery and procedure clause parsers: CALL subquery, CALL procedure,
//! LOAD CSV, EXPLAIN, PROFILE.

use super::super::CypherParser;
use super::super::ast::*;
use crate::Result;

impl CypherParser {
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
                    // EXPLAIN/PROFILE CALL { … } / CALL proc(...) — same
                    // dual shape as the top-level parser. Previously
                    // missing, so EXPLAIN/PROFILE over a CALL failed to
                    // parse.
                    "CALL" => {
                        self.skip_whitespace();
                        let next = self.peek_char();
                        if next == Some('{') || next == Some('(') {
                            Clause::CallSubquery(self.parse_call_subquery_clause()?)
                        } else {
                            Clause::CallProcedure(self.parse_call_procedure_clause()?)
                        }
                    }
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
                    // EXPLAIN/PROFILE CALL { … } / CALL proc(...) — same
                    // dual shape as the top-level parser. Previously
                    // missing, so EXPLAIN/PROFILE over a CALL failed to
                    // parse.
                    "CALL" => {
                        self.skip_whitespace();
                        let next = self.peek_char();
                        if next == Some('{') || next == Some('(') {
                            Clause::CallSubquery(self.parse_call_subquery_clause()?)
                        } else {
                            Clause::CallProcedure(self.parse_call_procedure_clause()?)
                        }
                    }
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
}
