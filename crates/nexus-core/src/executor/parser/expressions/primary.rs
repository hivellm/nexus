//! Simple and primary expression parsers: parenthesised forms, literals,
//! parameters, and the top-level dispatch (`parse_simple_expression`,
//! `parse_primary_expression`).

use super::super::CypherParser;
use super::super::ast::*;
use crate::Result;

impl CypherParser {
    /// Parse simple expression (no binary operators)
    pub(super) fn parse_simple_expression(&mut self) -> Result<Expression> {
        self.skip_whitespace();

        match self.peek_char() {
            Some('(') => self.parse_parenthesized_expression(),
            Some('$') => self.parse_parameter(),
            Some('"') | Some('\'') => self.parse_string_literal(),
            Some(c) if c.is_ascii_digit() => self.parse_numeric_literal(),
            Some(_c) if self.is_identifier_start() => {
                // Check if it's a keyword first
                if self.peek_keyword("CASE") {
                    self.parse_case_expression()
                } else if self.peek_keyword("COLLECT") {
                    // phase6_opencypher-subquery-transactions §9 —
                    // disambiguate `COLLECT { … }` (subquery) vs
                    // `collect(expr)` (aggregation function). The
                    // latter is parsed as a regular FunctionCall so we
                    // only commit to the subquery branch when the next
                    // non-whitespace token after the keyword is `{`.
                    let saved_pos = self.pos;
                    let saved_line = self.line;
                    let saved_col = self.column;
                    self.parse_keyword()?; // consume COLLECT
                    self.skip_whitespace();
                    let next_is_brace = self.peek_char() == Some('{');
                    self.pos = saved_pos;
                    self.line = saved_line;
                    self.column = saved_col;
                    if next_is_brace {
                        self.parse_collect_subquery_expression()
                    } else {
                        self.parse_identifier_expression()
                    }
                } else if self.peek_keyword("EXISTS") {
                    // phase6_opencypher-quickwins §7 — disambiguate:
                    //   EXISTS { pattern }     → pattern-existence predicate
                    //   exists(expr)           → scalar function, routed
                    //                             through parse_identifier_expression
                    //                             which emits a FunctionCall.
                    // The saved position lets us commit to the pattern-
                    // exists branch only after confirming the next
                    // non-whitespace token is `{`.
                    let saved_pos = self.pos;
                    let saved_line = self.line;
                    let saved_col = self.column;
                    self.parse_keyword()?; // consume EXISTS
                    self.skip_whitespace();
                    let next_is_brace = self.peek_char() == Some('{');
                    self.pos = saved_pos;
                    self.line = saved_line;
                    self.column = saved_col;
                    if next_is_brace {
                        self.parse_exists_expression()
                    } else {
                        self.parse_identifier_expression()
                    }
                } else if self.peek_keyword("true") {
                    self.parse_boolean_literal(true)
                } else if self.peek_keyword("false") {
                    self.parse_boolean_literal(false)
                } else if self.peek_keyword("null") {
                    self.parse_null_literal()
                } else {
                    self.parse_identifier_expression()
                }
            }
            Some('[') => self.parse_list_expression(),
            Some('{') => self.parse_map_expression(),
            _ => Err(self.error("Unexpected character in expression")),
        }
    }

    /// Parse primary expression
    pub(in super::super) fn parse_primary_expression(&mut self) -> Result<Expression> {
        self.skip_whitespace();

        match self.peek_char() {
            Some('(') => self.parse_parenthesized_expression(),
            Some('$') => self.parse_parameter(),
            Some('"') | Some('\'') => self.parse_string_literal(),
            Some(c) if c.is_ascii_digit() => self.parse_numeric_literal(),
            Some(_c) if self.is_identifier_start() => {
                // Check if it's a keyword first
                if self.peek_keyword("CASE") {
                    self.parse_case_expression()
                } else {
                    self.parse_identifier_expression()
                }
            }
            Some('[') => self.parse_list_expression(),
            Some('{') => self.parse_map_expression(),
            _ => Err(self.error("Unexpected character in expression")),
        }
    }

    /// Parse parenthesized expression
    pub(super) fn parse_parenthesized_expression(&mut self) -> Result<Expression> {
        self.expect_char('(')?;
        let expr = self.parse_expression()?;
        self.expect_char(')')?;
        Ok(expr)
    }

    /// Parse parameter
    pub(super) fn parse_parameter(&mut self) -> Result<Expression> {
        self.expect_char('$')?;
        let name = self.parse_identifier()?;
        Ok(Expression::Parameter(name))
    }

    /// Parse string literal
    pub(in super::super) fn parse_string_literal(&mut self) -> Result<Expression> {
        let quote = self.consume_char().unwrap();
        let mut value = String::new();

        while self.pos < self.input.len() {
            let ch = self.consume_char().unwrap();
            if ch == quote {
                break;
            } else if ch == '\\' && self.pos < self.input.len() {
                let next = self.consume_char().unwrap();
                match next {
                    'n' => value.push('\n'),
                    't' => value.push('\t'),
                    'r' => value.push('\r'),
                    '\\' => value.push('\\'),
                    _ => value.push(next),
                }
            } else {
                value.push(ch);
            }
        }

        Ok(Expression::Literal(Literal::String(value)))
    }

    /// Parse numeric literal
    pub(super) fn parse_numeric_literal(&mut self) -> Result<Expression> {
        let start = self.pos;

        // Parse integer part
        while self.pos < self.input.len() && self.is_digit() {
            self.consume_char();
        }

        // Check for decimal point
        if self.peek_char() == Some('.') {
            self.consume_char();
            while self.pos < self.input.len() && self.is_digit() {
                self.consume_char();
            }

            // Parse as float
            let value = self.input[start..self.pos]
                .parse::<f64>()
                .map_err(|_| self.error("Invalid float literal"))?;
            Ok(Expression::Literal(Literal::Float(value)))
        } else {
            // Parse as integer
            let value = self.input[start..self.pos]
                .parse::<i64>()
                .map_err(|_| self.error("Invalid integer literal"))?;
            Ok(Expression::Literal(Literal::Integer(value)))
        }
    }

    /// Parse boolean literal
    pub(super) fn parse_boolean_literal(&mut self, value: bool) -> Result<Expression> {
        if value {
            self.expect_keyword("true")?;
        } else {
            self.expect_keyword("false")?;
        }
        Ok(Expression::Literal(Literal::Boolean(value)))
    }

    /// Parse null literal
    pub(super) fn parse_null_literal(&mut self) -> Result<Expression> {
        self.expect_keyword("null")?;
        Ok(Expression::Literal(Literal::Null))
    }

    /// Parse map expression
    pub(super) fn parse_map_expression(&mut self) -> Result<Expression> {
        let property_map = self.parse_property_map()?;
        Ok(Expression::Map(property_map.properties))
    }
}
