//! Read-side clause parsers: MATCH, WHERE, RETURN, ORDER BY, LIMIT,
//! SKIP, WITH, UNWIND, FOREACH, UNION.

use super::super::CypherParser;
use super::super::ast::*;
use crate::Result;

impl CypherParser {
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
}
