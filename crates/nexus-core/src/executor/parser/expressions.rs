//! Expression parsing: precedence climbing from OR down through comparison,
//! arithmetic, unary, and primary. Also hosts property access,
//! function-call, list/map/parenthesis forms, and the `try_parse_not_pattern`
//! fallback for negated patterns inside WHERE.

use super::CypherParser;
use super::ast::*;
use crate::{Error, Result};
use std::collections::HashMap;

impl CypherParser {
    /// Parse expression
    /// Parse expression (simplified for MVP)
    pub fn parse_expression(&mut self) -> Result<Expression> {
        self.skip_whitespace();
        self.parse_or_expression()
    }

    /// Parse OR expressions (lowest precedence)
    pub(super) fn parse_or_expression(&mut self) -> Result<Expression> {
        let mut left = self.parse_and_expression()?;

        while self.peek_keyword("OR") {
            self.parse_keyword()?;
            self.skip_whitespace();
            let right = self.parse_and_expression()?;
            left = Expression::BinaryOp {
                left: Box::new(left),
                op: BinaryOperator::Or,
                right: Box::new(right),
            };
        }

        Ok(left)
    }

    /// Parse AND expressions
    pub(super) fn parse_and_expression(&mut self) -> Result<Expression> {
        let mut left = self.parse_not_expression()?;

        while self.peek_keyword("AND") {
            self.parse_keyword()?;
            self.skip_whitespace();
            let right = self.parse_not_expression()?;
            left = Expression::BinaryOp {
                left: Box::new(left),
                op: BinaryOperator::And,
                right: Box::new(right),
            };
        }

        Ok(left)
    }

    /// Parse NOT expressions
    pub(super) fn parse_not_expression(&mut self) -> Result<Expression> {
        self.skip_whitespace();

        if self.peek_keyword("NOT") {
            self.parse_keyword()?;
            self.skip_whitespace();

            // Check if next is a parenthesized expression or a pattern
            let operand = if self.peek_char() == Some('(') {
                // Save position to potentially backtrack
                let saved_pos = self.pos;
                let saved_line = self.line;
                let saved_column = self.column;

                // Try to parse as a pattern (NOT (n)-[:REL]->() is shorthand for NOT EXISTS { pattern })
                if let Ok(pattern) = self.try_parse_not_pattern() {
                    // This is NOT pattern, convert to NOT EXISTS
                    return Ok(Expression::UnaryOp {
                        op: UnaryOperator::Not,
                        operand: Box::new(Expression::Exists {
                            pattern,
                            where_clause: None,
                        }),
                    });
                }

                // Not a pattern, restore and parse as regular parenthesized expression
                self.pos = saved_pos;
                self.line = saved_line;
                self.column = saved_column;

                self.expect_char('(')?;
                self.skip_whitespace();
                let expr = self.parse_or_expression()?;
                self.skip_whitespace();
                self.expect_char(')')?;
                expr
            } else {
                self.parse_comparison_expression()?
            };
            return Ok(Expression::UnaryOp {
                op: UnaryOperator::Not,
                operand: Box::new(operand),
            });
        }

        self.parse_comparison_expression()
    }

    /// Try to parse a pattern for NOT (pattern) syntax
    /// Returns Ok(Pattern) if successful, Err if not a pattern
    pub(super) fn try_parse_not_pattern(&mut self) -> Result<Pattern> {
        // We need to parse something like: (n)-[:REL]->()
        // The key indicator that this is a pattern is the relationship after the first node

        let mut elements = Vec::new();

        // Parse first node
        let node = self.parse_node_pattern()?;
        elements.push(PatternElement::Node(node));

        self.skip_whitespace();

        // Check if followed by a relationship pattern (-, <, >)
        // This is what distinguishes a pattern from a regular expression
        if self.peek_char() != Some('-') && self.peek_char() != Some('<') {
            return Err(self.error("Not a pattern - no relationship found"));
        }

        // Parse the rest of the pattern (relationships and nodes)
        while self.pos < self.input.len() {
            self.skip_whitespace();

            // Check if we've reached the end of the pattern
            if self.peek_char() == Some(')') {
                // Check if this could be the end (nothing follows or only RETURN/ORDER/etc.)
                break;
            }

            // Check if we have a relationship pattern
            if self.peek_char() == Some('-') || self.peek_char() == Some('<') {
                // Parse relationship
                let rel = self.parse_relationship_pattern()?;
                elements.push(PatternElement::Relationship(rel));

                self.skip_whitespace();

                // Parse the next node if there is one
                if self.peek_char() == Some('(') {
                    let node = self.parse_node_pattern()?;
                    elements.push(PatternElement::Node(node));
                }
            } else {
                break;
            }
        }

        Ok(Pattern {
            elements,
            path_variable: None,
        })
    }

    /// Parse comparison expressions (=, <>, <, <=, >, >=, IS NULL, IS NOT NULL, STARTS WITH, ENDS WITH, CONTAINS, =~)
    pub(super) fn parse_comparison_expression(&mut self) -> Result<Expression> {
        let left = self.parse_additive_expression()?;

        // Check for IS NULL / IS NOT NULL
        self.skip_whitespace();
        if self.peek_keyword("IS") {
            self.parse_keyword()?;
            self.skip_whitespace();

            let negated = if self.peek_keyword("NOT") {
                self.parse_keyword()?;
                self.skip_whitespace();
                true
            } else {
                false
            };

            if self.peek_keyword("NULL") {
                self.parse_keyword()?;
                return Ok(Expression::IsNull {
                    expr: Box::new(left),
                    negated,
                });
            } else {
                return Err(self.error("Expected NULL after IS [NOT]"));
            }
        }

        // Check for string operators (STARTS WITH, ENDS WITH, CONTAINS)
        self.skip_whitespace();
        if self.peek_keyword("STARTS") {
            self.parse_keyword()?;
            self.skip_whitespace();
            if self.peek_keyword("WITH") {
                self.parse_keyword()?;
                self.skip_whitespace();
                let right = self.parse_additive_expression()?;
                return Ok(Expression::BinaryOp {
                    left: Box::new(left),
                    op: BinaryOperator::StartsWith,
                    right: Box::new(right),
                });
            } else {
                return Err(self.error("Expected WITH after STARTS"));
            }
        }

        if self.peek_keyword("ENDS") {
            self.parse_keyword()?;
            self.skip_whitespace();
            if self.peek_keyword("WITH") {
                self.parse_keyword()?;
                self.skip_whitespace();
                let right = self.parse_additive_expression()?;
                return Ok(Expression::BinaryOp {
                    left: Box::new(left),
                    op: BinaryOperator::EndsWith,
                    right: Box::new(right),
                });
            } else {
                return Err(self.error("Expected WITH after ENDS"));
            }
        }

        if self.peek_keyword("CONTAINS") {
            self.parse_keyword()?;
            self.skip_whitespace();
            let right = self.parse_additive_expression()?;
            return Ok(Expression::BinaryOp {
                left: Box::new(left),
                op: BinaryOperator::Contains,
                right: Box::new(right),
            });
        }

        // Check for IN operator
        self.skip_whitespace();
        if self.peek_keyword("IN") {
            self.parse_keyword()?;
            self.skip_whitespace();
            let right = self.parse_additive_expression()?;
            return Ok(Expression::BinaryOp {
                left: Box::new(left),
                op: BinaryOperator::In,
                right: Box::new(right),
            });
        }

        // Check for regex operator (=~)
        self.skip_whitespace();
        if self.peek_char() == Some('=') && self.peek_char_at(1) == Some('~') {
            self.consume_char(); // consume '='
            self.consume_char(); // consume '~'
            self.skip_whitespace();
            let right = self.parse_additive_expression()?;
            return Ok(Expression::BinaryOp {
                left: Box::new(left),
                op: BinaryOperator::RegexMatch,
                right: Box::new(right),
            });
        }

        // Check for comparison operators (=, <>, <, <=, >, >=)
        self.skip_whitespace();
        if let Some(op) = self.parse_comparison_operator() {
            self.skip_whitespace();
            let right = self.parse_additive_expression()?;
            return Ok(Expression::BinaryOp {
                left: Box::new(left),
                op,
                right: Box::new(right),
            });
        }

        Ok(left)
    }

    /// Parse additive expressions (+, -)
    pub(super) fn parse_additive_expression(&mut self) -> Result<Expression> {
        let mut left = self.parse_multiplicative_expression()?;

        loop {
            self.skip_whitespace();
            if let Some(op) = self.parse_additive_operator() {
                self.skip_whitespace();
                let right = self.parse_multiplicative_expression()?;
                left = Expression::BinaryOp {
                    left: Box::new(left),
                    op,
                    right: Box::new(right),
                };
            } else {
                break;
            }
        }

        Ok(left)
    }

    /// Parse multiplicative expressions (*, /, %)
    pub(super) fn parse_multiplicative_expression(&mut self) -> Result<Expression> {
        let mut left = self.parse_unary_expression()?;

        loop {
            self.skip_whitespace();
            if let Some(op) = self.parse_multiplicative_operator() {
                self.skip_whitespace();
                let right = self.parse_unary_expression()?;
                left = Expression::BinaryOp {
                    left: Box::new(left),
                    op,
                    right: Box::new(right),
                };
            } else {
                break;
            }
        }

        Ok(left)
    }

    /// Parse unary expressions
    pub(super) fn parse_unary_expression(&mut self) -> Result<Expression> {
        self.skip_whitespace();

        // Check for unary operators
        if let Some(op) = self.parse_unary_operator() {
            self.skip_whitespace();
            let operand = self.parse_simple_expression()?;
            return Ok(Expression::UnaryOp {
                op,
                operand: Box::new(operand),
            });
        }

        self.parse_simple_expression()
    }

    /// Parse comparison operator only (not AND/OR)
    pub(super) fn parse_comparison_operator(&mut self) -> Option<BinaryOperator> {
        self.skip_whitespace();

        match self.peek_char() {
            Some('=') => {
                self.consume_char();
                Some(BinaryOperator::Equal)
            }
            Some('!') if self.peek_char_at(1) == Some('=') => {
                self.consume_char();
                self.consume_char();
                Some(BinaryOperator::NotEqual)
            }
            Some('<') => {
                self.consume_char();
                if self.peek_char() == Some('=') {
                    self.consume_char();
                    Some(BinaryOperator::LessThanOrEqual)
                } else if self.peek_char() == Some('>') {
                    self.consume_char();
                    Some(BinaryOperator::NotEqual)
                } else {
                    Some(BinaryOperator::LessThan)
                }
            }
            Some('>') => {
                self.consume_char();
                if self.peek_char() == Some('=') {
                    self.consume_char();
                    Some(BinaryOperator::GreaterThanOrEqual)
                } else {
                    Some(BinaryOperator::GreaterThan)
                }
            }
            _ => None,
        }
    }

    /// Parse simple expression (no binary operators)
    pub(super) fn parse_simple_expression(&mut self) -> Result<Expression> {
        self.skip_whitespace();

        match self.peek_char() {
            Some('(') => self.parse_parenthesized_expression(),
            Some('$') => self.parse_parameter(),
            Some('"') | Some('\'') => self.parse_string_literal(),
            Some(c) if c.is_ascii_digit() => self.parse_numeric_literal(),
            Some(c) if self.is_identifier_start() => {
                // Check if it's a keyword first
                if self.peek_keyword("CASE") {
                    self.parse_case_expression()
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

    /// Parse binary operator
    pub(super) fn parse_binary_operator(&mut self) -> Option<BinaryOperator> {
        self.skip_whitespace();

        // Check for keyword operators first (AND, OR)
        if self.peek_keyword("AND") {
            self.parse_keyword().ok()?;
            return Some(BinaryOperator::And);
        } else if self.peek_keyword("OR") {
            self.parse_keyword().ok()?;
            return Some(BinaryOperator::Or);
        }

        // Then check for symbol operators
        match self.peek_char() {
            Some('=') => {
                self.consume_char();
                Some(BinaryOperator::Equal)
            }
            Some('!') if self.peek_char_at(1) == Some('=') => {
                self.consume_char();
                self.consume_char();
                Some(BinaryOperator::NotEqual)
            }
            Some('<') => {
                self.consume_char();
                if self.peek_char() == Some('=') {
                    self.consume_char();
                    Some(BinaryOperator::LessThanOrEqual)
                } else {
                    Some(BinaryOperator::LessThan)
                }
            }
            Some('>') => {
                self.consume_char();
                if self.peek_char() == Some('=') {
                    self.consume_char();
                    Some(BinaryOperator::GreaterThanOrEqual)
                } else {
                    Some(BinaryOperator::GreaterThan)
                }
            }
            Some('+') => {
                self.consume_char();
                Some(BinaryOperator::Add)
            }
            Some('-') => {
                self.consume_char();
                Some(BinaryOperator::Subtract)
            }
            Some('*') => {
                self.consume_char();
                Some(BinaryOperator::Multiply)
            }
            Some('/') => {
                self.consume_char();
                Some(BinaryOperator::Divide)
            }
            _ => None,
        }
    }

    /// Peek the character `offset` characters ahead of `self.pos`.
    ///
    /// Slicing from `self.pos` first (an O(1) byte-slice op) and then
    /// walking `offset` chars keeps the cost proportional to `offset`
    /// rather than `self.pos + offset` — a measurable win for the
    /// two-char lookahead patterns that drive operator detection
    /// (`==`, `!=`, `..`, …).
    pub(super) fn peek_char_at(&self, offset: usize) -> Option<char> {
        self.input[self.pos..].chars().nth(offset)
    }

    /// Parse primary expression
    pub(super) fn parse_primary_expression(&mut self) -> Result<Expression> {
        self.skip_whitespace();

        match self.peek_char() {
            Some('(') => self.parse_parenthesized_expression(),
            Some('$') => self.parse_parameter(),
            Some('"') | Some('\'') => self.parse_string_literal(),
            Some(c) if c.is_ascii_digit() => self.parse_numeric_literal(),
            Some(c) if self.is_identifier_start() => {
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
    pub(super) fn parse_string_literal(&mut self) -> Result<Expression> {
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

    /// Parse identifier expression
    pub(super) fn parse_identifier_expression(&mut self) -> Result<Expression> {
        let mut identifier = self.parse_identifier()?;

        // phase6_opencypher-geospatial-predicates §4 — namespaced
        // function call `ns.func(...)` (e.g. `point.withinBBox`,
        // `spatial.distance`). Lookahead: if we see `.identifier(`,
        // merge the two tokens into a single dotted identifier and
        // fall into the function-call branch below. When no `(`
        // follows, restore the cursor so the existing
        // PropertyAccess branch re-parses the `.tail` suffix — the
        // precedence of `n.prop` access must not change.
        if self.peek_char() == Some('.') {
            let saved_pos = self.pos;
            let saved_line = self.line;
            let saved_column = self.column;
            self.consume_char();
            if self.is_identifier_start() {
                let tail = self.parse_identifier()?;
                if self.peek_char() == Some('(') {
                    identifier = format!("{identifier}.{tail}");
                } else {
                    self.pos = saved_pos;
                    self.line = saved_line;
                    self.column = saved_column;
                }
            } else {
                self.pos = saved_pos;
                self.line = saved_line;
                self.column = saved_column;
            }
        }

        // Check for function call
        if self.peek_char() == Some('(') {
            // Special case: point() function creates a Point literal
            if identifier.to_lowercase() == "point" {
                return self.parse_point_literal();
            }

            self.consume_char(); // consume '('
            let mut args = Vec::new();

            self.skip_whitespace();

            // Check for count(*) special case
            if self.peek_char() == Some('*') {
                self.consume_char(); // consume '*'
                self.skip_whitespace();
                // count(*) has no arguments - empty args list means count all
            } else {
                // Check for DISTINCT keyword (for COUNT(DISTINCT ...))
                let has_distinct = if self.peek_keyword("DISTINCT") {
                    self.expect_keyword("DISTINCT")?;
                    self.skip_whitespace();
                    true
                } else {
                    false
                };

                // If DISTINCT was found, add it as a marker in args
                if has_distinct {
                    args.push(Expression::Variable("__DISTINCT__".to_string()));
                }

                // Parse arguments
                while self.peek_char() != Some(')') {
                    // Special handling for filter() - filter(x IN list WHERE predicate)
                    let arg = if identifier.to_lowercase() == "filter" && args.is_empty() {
                        // Try to parse filter syntax: variable IN list WHERE predicate
                        let saved_pos = self.pos;
                        let saved_line = self.line;
                        let saved_column = self.column;

                        // Try parsing filter syntax
                        let result = (|| -> Result<Expression> {
                            // Parse variable name
                            let variable = self.parse_identifier()?;
                            self.skip_whitespace();

                            // Expect IN keyword
                            if !self.peek_keyword("IN") {
                                return Err(Error::CypherSyntax(format!(
                                    "Expected IN keyword in filter() at line {}, column {}",
                                    self.line, self.column
                                )));
                            }
                            self.expect_keyword("IN")?;
                            self.skip_whitespace();

                            // Parse list expression
                            let list_expression = Box::new(self.parse_expression()?);
                            self.skip_whitespace();

                            // Parse optional WHERE clause
                            let where_clause = if self.peek_keyword("WHERE") {
                                self.expect_keyword("WHERE")?;
                                self.skip_whitespace();
                                Some(Box::new(self.parse_expression()?))
                            } else {
                                None
                            };

                            // Convert to ListComprehension (filter has no transformation, just filtering)
                            Ok(Expression::ListComprehension {
                                variable,
                                list_expression,
                                where_clause,
                                transform_expression: None,
                            })
                        })();

                        match result {
                            Ok(expr) => expr,
                            Err(_) => {
                                // Failed to parse as filter syntax - restore position and parse as normal expression
                                self.pos = saved_pos;
                                self.line = saved_line;
                                self.column = saved_column;
                                self.parse_expression()?
                            }
                        }
                    }
                    // Special handling for shortestPath() and allShortestPaths() - they accept patterns directly
                    else if (identifier.to_lowercase() == "shortestpath"
                        || identifier.to_lowercase() == "allshortestpaths")
                        && self.peek_char() == Some('(')
                    {
                        // Try to parse as pattern - if it fails, fall back to expression
                        let saved_pos = self.pos;
                        let saved_line = self.line;
                        let saved_column = self.column;

                        // Try parsing as pattern
                        match self.parse_pattern() {
                            Ok(pattern) => {
                                // Successfully parsed as pattern - create PatternComprehension
                                Expression::PatternComprehension {
                                    pattern,
                                    where_clause: None,
                                    transform_expression: None,
                                }
                            }
                            Err(_) => {
                                // Failed to parse as pattern - restore position and parse as expression
                                self.pos = saved_pos;
                                self.line = saved_line;
                                self.column = saved_column;
                                self.parse_expression()?
                            }
                        }
                    } else {
                        // Normal argument parsing
                        self.parse_expression()?
                    };

                    args.push(arg);

                    if self.peek_char() == Some(',') {
                        self.consume_char();
                        self.skip_whitespace();
                    }
                }
            }

            self.expect_char(')')?;

            // Special handling: if this is filter() and we successfully converted it to ListComprehension,
            // return the ListComprehension directly instead of wrapping it in a FunctionCall
            if identifier.to_lowercase() == "filter"
                && args.len() == 1
                && matches!(args[0], Expression::ListComprehension { .. })
            {
                return Ok(args.into_iter().next().unwrap());
            }

            Ok(Expression::FunctionCall {
                name: identifier,
                args,
            })
        }
        // Check for map projection: n {.name, .age}
        else if self.peek_char() == Some('{') {
            let source = Box::new(Expression::Variable(identifier));
            let items = self.parse_map_projection_items()?;
            Ok(Expression::MapProjection { source, items })
        }
        // phase6_opencypher-quickwins §5 — `var[expr]` directly after
        // an identifier (no intervening `.property`). The evaluator
        // disambiguates node/map vs list at runtime; the parser just
        // emits an `ArrayIndex` and lets runtime decide which lookup
        // semantics to use.
        else if self.peek_char() == Some('[') {
            self.consume_char(); // consume '['
            self.skip_whitespace();
            let index = Box::new(self.parse_expression()?);
            self.skip_whitespace();
            self.expect_char(']')?;
            Ok(Expression::ArrayIndex {
                base: Box::new(Expression::Variable(identifier)),
                index,
            })
        }
        // phase6_opencypher-quickwins §8 — label predicate `var:Label`
        // or `var:$param` in WHERE/RETURN expression position. Re-uses
        // the Filter operator's text-mode short-circuit by emitting a
        // synthetic `FunctionCall` whose `expression_to_string` render
        // reproduces the `variable:label` shape the short-circuit
        // understands. (No dedicated AST variant — the filter path
        // already pattern-matches on the rendered string.)
        else if self.peek_char() == Some(':') && self.peek_char_at(1) != Some(':') {
            self.consume_char(); // consume ':'
            self.skip_whitespace();
            let label_source = if self.peek_char() == Some('$') {
                self.consume_char();
                format!("${}", self.parse_identifier()?)
            } else {
                self.parse_identifier()?
            };
            Ok(Expression::FunctionCall {
                name: "__label_predicate__".to_string(),
                args: vec![
                    Expression::Variable(identifier),
                    Expression::Literal(Literal::String(label_source)),
                ],
            })
        }
        // Check for property access
        else if self.peek_char() == Some('.') {
            self.consume_char();
            let property = self.parse_identifier()?;
            let mut expr = Expression::PropertyAccess {
                variable: identifier,
                property,
            };

            // Check for array indexing after property access: n.tags[0]
            while self.peek_char() == Some('[') {
                self.consume_char(); // consume '['
                self.skip_whitespace();

                // Check if this is a slice by looking ahead for '..'
                // We need to check this BEFORE parsing the start expression,
                // because parse_numeric_literal() will consume a single '.' as part of a float
                let is_slice = {
                    let saved_pos = self.pos;
                    let mut check_pos = 0;

                    // Skip whitespace
                    while let Some(c) = self.peek_char_at(check_pos) {
                        if c.is_whitespace() {
                            check_pos += 1;
                        } else {
                            break;
                        }
                    }

                    // Check if we start with '..' (Case: [..end] or [:end])
                    if self.peek_char_at(check_pos) == Some('.')
                        && self.peek_char_at(check_pos + 1) == Some('.')
                    {
                        self.pos = saved_pos;
                        true
                    } else if let Some(c) = self.peek_char_at(check_pos) {
                        // Check if we have a number (including negative) followed by '..'
                        if c.is_ascii_digit() || c == '-' {
                            // Skip the '-' if present
                            let mut num_end = if c == '-' { check_pos + 1 } else { check_pos };
                            // Skip digits
                            while let Some(ch) = self.peek_char_at(num_end) {
                                if ch.is_ascii_digit() {
                                    num_end += 1;
                                } else {
                                    break;
                                }
                            }

                            // Skip whitespace after number
                            let mut after_num = num_end;
                            while let Some(ch) = self.peek_char_at(after_num) {
                                if ch.is_whitespace() {
                                    after_num += 1;
                                } else {
                                    break;
                                }
                            }

                            // Check for '..' after number
                            let is_slice = self.peek_char_at(after_num) == Some('.')
                                && self.peek_char_at(after_num + 1) == Some('.');
                            self.pos = saved_pos;
                            is_slice
                        } else {
                            self.pos = saved_pos;
                            false
                        }
                    } else {
                        self.pos = saved_pos;
                        false
                    }
                };

                let start_expr = if is_slice {
                    // For slice, we need to parse the start expression carefully
                    // Check if we start with a number (including negative)
                    if let Some(c) = self.peek_char() {
                        if c.is_ascii_digit() || c == '-' {
                            // Parse number manually to avoid consuming the '.' after it
                            let start = self.pos;
                            // Consume '-' if present
                            if c == '-' {
                                self.consume_char();
                            }
                            while self.pos < self.input.len() {
                                if let Some(ch) = self.peek_char() {
                                    if ch.is_ascii_digit() {
                                        self.consume_char();
                                    } else {
                                        break;
                                    }
                                } else {
                                    break;
                                }
                            }
                            let num_str = &self.input[start..self.pos];
                            if !num_str.is_empty() && num_str != "-" {
                                let num = num_str
                                    .parse::<i64>()
                                    .map_err(|_| self.error("Invalid number in slice"))?;
                                Some(Box::new(Expression::Literal(Literal::Integer(num))))
                            } else {
                                // Not a number, parse as regular expression
                                self.pos = start; // Reset position
                                Some(Box::new(self.parse_expression()?))
                            }
                        } else if c == '.' || c == ':' {
                            None
                        } else {
                            // Parse as regular expression
                            Some(Box::new(self.parse_expression()?))
                        }
                    } else {
                        None
                    }
                } else {
                    // Regular indexing - parse normally
                    if self.peek_char() != Some('.') && self.peek_char() != Some(':') {
                        Some(Box::new(self.parse_expression()?))
                    } else {
                        None
                    }
                };

                self.skip_whitespace();

                // Check for '..' (slice operator)
                if self.peek_char() == Some('.') && self.peek_char_at(1) == Some('.') {
                    self.consume_char(); // consume first '.'
                    self.consume_char(); // consume second '.'
                    self.skip_whitespace();

                    let end_expr = if self.peek_char() != Some(']') {
                        // Check if we start with a number (including negative)
                        if let Some(c) = self.peek_char() {
                            if c.is_ascii_digit() || c == '-' {
                                // Parse number manually
                                let start = self.pos;
                                // Consume '-' if present
                                if c == '-' {
                                    self.consume_char();
                                }
                                while self.pos < self.input.len() {
                                    if let Some(ch) = self.peek_char() {
                                        if ch.is_ascii_digit() {
                                            self.consume_char();
                                        } else {
                                            break;
                                        }
                                    } else {
                                        break;
                                    }
                                }
                                let num_str = &self.input[start..self.pos];
                                if !num_str.is_empty() && num_str != "-" {
                                    let num = num_str
                                        .parse::<i64>()
                                        .map_err(|_| self.error("Invalid number in slice"))?;
                                    Some(Box::new(Expression::Literal(Literal::Integer(num))))
                                } else {
                                    // Not a number, parse as regular expression
                                    self.pos = start; // Reset position
                                    Some(Box::new(self.parse_expression()?))
                                }
                            } else {
                                Some(Box::new(self.parse_expression()?))
                            }
                        } else {
                            None
                        }
                    } else {
                        None
                    };

                    self.skip_whitespace();
                    self.expect_char(']')?;

                    expr = Expression::ArraySlice {
                        base: Box::new(expr),
                        start: start_expr,
                        end: end_expr,
                    };
                } else {
                    // Regular array indexing
                    self.skip_whitespace();
                    self.expect_char(']')?;

                    if let Some(index) = start_expr {
                        expr = Expression::ArrayIndex {
                            base: Box::new(expr),
                            index,
                        };
                    } else {
                        return Err(self.error("Array index or slice expected"));
                    }
                }
            }

            Ok(expr)
        } else {
            Ok(Expression::Variable(identifier))
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

    /// Parse list expression
    pub(super) fn parse_list_expression(&mut self) -> Result<Expression> {
        self.expect_char('[')?;
        self.skip_whitespace();

        // Check if this is a pattern comprehension: [(pattern) WHERE ... | ...]
        // Pattern comprehensions start with '(' or an identifier followed by ':' or '-'
        let saved_pos = self.pos;
        let is_pattern_comprehension = if self.peek_char() == Some('(') {
            // Starts with '(', likely a pattern
            true
        } else if self.is_identifier_start() {
            // Check if identifier is followed by ':' (label) or '-' (relationship)
            let _identifier = self.parse_identifier()?;
            self.skip_whitespace();
            let next_char = self.peek_char();
            let is_pattern = next_char == Some(':') || next_char == Some('-');
            // Reset position
            self.pos = saved_pos;
            is_pattern
        } else {
            false
        };

        if is_pattern_comprehension {
            // Parse pattern comprehension: [(pattern) WHERE ... | ...]
            let pattern = self.parse_pattern_until_where_or_brace()?;
            self.skip_whitespace();

            // Parse optional WHERE clause
            let where_clause = if self.peek_keyword("WHERE") {
                self.expect_keyword("WHERE")?;
                self.skip_whitespace();
                Some(Box::new(self.parse_expression()?))
            } else {
                None
            };
            self.skip_whitespace();

            // Parse optional transformation expression (after |)
            let transform_expression = if self.peek_char() == Some('|') {
                self.consume_char();
                self.skip_whitespace();
                Some(Box::new(self.parse_expression()?))
            } else {
                None
            };
            self.skip_whitespace();

            self.expect_char(']')?;

            return Ok(Expression::PatternComprehension {
                pattern,
                where_clause,
                transform_expression,
            });
        }

        // Check if this is a list comprehension: [x IN list WHERE ... | ...]
        if self.is_identifier_start() {
            let saved_pos = self.pos;
            let variable = self.parse_identifier()?;
            self.skip_whitespace();

            // Check if next token is IN (indicating list comprehension)
            if self.peek_keyword("IN") {
                // This is a list comprehension
                self.expect_keyword("IN")?;
                self.skip_whitespace();

                // Parse list expression
                let list_expression = Box::new(self.parse_expression()?);
                self.skip_whitespace();

                // Parse optional WHERE clause
                let where_clause = if self.peek_keyword("WHERE") {
                    self.expect_keyword("WHERE")?;
                    self.skip_whitespace();
                    Some(Box::new(self.parse_expression()?))
                } else {
                    None
                };
                self.skip_whitespace();

                // Parse optional transformation expression (after |)
                let transform_expression = if self.peek_char() == Some('|') {
                    self.consume_char();
                    self.skip_whitespace();
                    Some(Box::new(self.parse_expression()?))
                } else {
                    None
                };
                self.skip_whitespace();

                self.expect_char(']')?;

                return Ok(Expression::ListComprehension {
                    variable,
                    list_expression,
                    where_clause,
                    transform_expression,
                });
            } else {
                // Not a list comprehension, reset position and parse as regular list
                self.pos = saved_pos;
            }
        }

        // Regular list expression
        let mut elements = Vec::new();

        while self.peek_char() != Some(']') {
            let expr = self.parse_expression()?;
            elements.push(expr);

            if self.peek_char() == Some(',') {
                self.consume_char();
                self.skip_whitespace();
            }
        }

        self.expect_char(']')?;

        let mut expr = Expression::List(elements);

        // Check for array indexing or slicing after list: ['a', 'b'][0] or ['a', 'b'][1..3]
        while self.peek_char() == Some('[') {
            self.consume_char(); // consume '['
            self.skip_whitespace();

            // Check if this is a slice by looking ahead for '..'
            // We need to check this BEFORE parsing the start expression,
            // because parse_numeric_literal() will consume a single '.' as part of a float
            let is_slice = {
                let saved_pos = self.pos;
                let mut check_pos = 0;

                // Skip whitespace
                while let Some(c) = self.peek_char_at(check_pos) {
                    if c.is_whitespace() {
                        check_pos += 1;
                    } else {
                        break;
                    }
                }

                // Check if we start with '..' (Case: [..end] or [:end])
                if self.peek_char_at(check_pos) == Some('.')
                    && self.peek_char_at(check_pos + 1) == Some('.')
                {
                    self.pos = saved_pos;
                    true
                } else if let Some(c) = self.peek_char_at(check_pos) {
                    // Check if we have a number (including negative) followed by '..'
                    if c.is_ascii_digit() || c == '-' {
                        // Skip the '-' if present
                        let mut num_end = if c == '-' { check_pos + 1 } else { check_pos };
                        // Skip digits
                        while let Some(ch) = self.peek_char_at(num_end) {
                            if ch.is_ascii_digit() {
                                num_end += 1;
                            } else {
                                break;
                            }
                        }

                        // Skip whitespace after number
                        let mut after_num = num_end;
                        while let Some(ch) = self.peek_char_at(after_num) {
                            if ch.is_whitespace() {
                                after_num += 1;
                            } else {
                                break;
                            }
                        }

                        // Check for '..' after number
                        let is_slice = self.peek_char_at(after_num) == Some('.')
                            && self.peek_char_at(after_num + 1) == Some('.');
                        self.pos = saved_pos;
                        is_slice
                    } else {
                        self.pos = saved_pos;
                        false
                    }
                } else {
                    self.pos = saved_pos;
                    false
                }
            };

            let start_expr = if is_slice {
                // For slice, we need to parse the start expression carefully
                // Check if we start with a number (including negative)
                if let Some(c) = self.peek_char() {
                    if c.is_ascii_digit() || c == '-' {
                        // Parse number manually to avoid consuming the '.' after it
                        let start = self.pos;
                        // Consume '-' if present
                        if c == '-' {
                            self.consume_char();
                        }
                        while self.pos < self.input.len() {
                            if let Some(ch) = self.peek_char() {
                                if ch.is_ascii_digit() {
                                    self.consume_char();
                                } else {
                                    break;
                                }
                            } else {
                                break;
                            }
                        }
                        let num_str = &self.input[start..self.pos];
                        if !num_str.is_empty() && num_str != "-" {
                            let num = num_str
                                .parse::<i64>()
                                .map_err(|_| self.error("Invalid number in slice"))?;
                            Some(Box::new(Expression::Literal(Literal::Integer(num))))
                        } else {
                            // Not a number, parse as regular expression
                            self.pos = start; // Reset position
                            Some(Box::new(self.parse_expression()?))
                        }
                    } else if c == '.' || c == ':' {
                        None
                    } else {
                        // Parse as regular expression
                        Some(Box::new(self.parse_expression()?))
                    }
                } else {
                    None
                }
            } else {
                // Regular indexing - parse normally
                if self.peek_char() != Some('.') && self.peek_char() != Some(':') {
                    Some(Box::new(self.parse_expression()?))
                } else {
                    None
                }
            };

            self.skip_whitespace();

            // Check for '..' (slice operator)
            if self.peek_char() == Some('.') && self.peek_char_at(1) == Some('.') {
                self.consume_char(); // consume first '.'
                self.consume_char(); // consume second '.'
                self.skip_whitespace();

                let end_expr = if self.peek_char() != Some(']') {
                    Some(Box::new(self.parse_expression()?))
                } else {
                    None
                };

                self.skip_whitespace();
                self.expect_char(']')?;

                expr = Expression::ArraySlice {
                    base: Box::new(expr),
                    start: start_expr,
                    end: end_expr,
                };
            } else {
                // Regular array indexing
                self.skip_whitespace();
                self.expect_char(']')?;

                if let Some(index) = start_expr {
                    expr = Expression::ArrayIndex {
                        base: Box::new(expr),
                        index,
                    };
                } else {
                    return Err(self.error("Array index or slice expected"));
                }
            }
        }

        Ok(expr)
    }

    /// Parse point literal
    /// Syntax: point({x: 1, y: 2}) or point({x: 1, y: 2, z: 3}) or point({longitude: -122, latitude: 37, crs: 'wgs-84'})
    pub(super) fn parse_point_literal(&mut self) -> Result<Expression> {
        self.expect_char('(')?;
        self.skip_whitespace();
        self.expect_char('{')?;
        self.skip_whitespace();

        let mut x: Option<f64> = None;
        let mut y: Option<f64> = None;
        let mut z: Option<f64> = None;
        let mut coordinate_system = crate::geospatial::CoordinateSystem::Cartesian;

        // Parse key-value pairs
        while self.peek_char() != Some('}') {
            let key = self.parse_identifier()?;
            self.skip_whitespace();
            self.expect_char(':')?;
            self.skip_whitespace();

            match key.to_lowercase().as_str() {
                "x" | "longitude" => {
                    let expr = self.parse_expression()?;
                    x = Some(self.extract_number_from_expression(&expr)?);
                }
                "y" | "latitude" => {
                    let expr = self.parse_expression()?;
                    y = Some(self.extract_number_from_expression(&expr)?);
                }
                "z" | "height" => {
                    let expr = self.parse_expression()?;
                    z = Some(self.extract_number_from_expression(&expr)?);
                }
                "crs" => {
                    let expr = self.parse_string_literal()?;
                    let crs_str = if let Expression::Literal(Literal::String(s)) = expr {
                        s.to_lowercase()
                    } else {
                        return Err(self.error("CRS must be a string literal"));
                    };
                    coordinate_system = match crs_str.as_str() {
                        "cartesian" | "cartesian-3d" => {
                            crate::geospatial::CoordinateSystem::Cartesian
                        }
                        "wgs-84" | "wgs-84-3d" => crate::geospatial::CoordinateSystem::WGS84,
                        _ => {
                            return Err(
                                self.error(&format!("Unknown coordinate system: {}", crs_str))
                            );
                        }
                    };
                }
                _ => {
                    return Err(self.error(&format!("Unknown point property: {}", key)));
                }
            }

            self.skip_whitespace();
            if self.peek_char() == Some(',') {
                self.consume_char();
                self.skip_whitespace();
            }
        }

        self.expect_char('}')?;
        self.skip_whitespace();
        self.expect_char(')')?;

        let x = x.ok_or_else(|| self.error("Point must have x or longitude"))?;
        let y = y.ok_or_else(|| self.error("Point must have y or latitude"))?;

        let point = if let Some(z_val) = z {
            crate::geospatial::Point::new_3d(x, y, z_val, coordinate_system)
        } else {
            crate::geospatial::Point::new_2d(x, y, coordinate_system)
        };

        Ok(Expression::Literal(Literal::Point(point)))
    }

    /// Extract number from expression (helper for point parsing)
    pub(super) fn extract_number_from_expression(&self, expr: &Expression) -> Result<f64> {
        match expr {
            Expression::Literal(Literal::Integer(i)) => Ok(*i as f64),
            Expression::Literal(Literal::Float(f)) => Ok(*f),
            _ => Err(Error::CypherSyntax(
                "Point coordinates must be numbers".to_string(),
            )),
        }
    }

    /// Parse map expression
    pub(super) fn parse_map_expression(&mut self) -> Result<Expression> {
        let property_map = self.parse_property_map()?;
        Ok(Expression::Map(property_map.properties))
    }

    /// Parse map projection items: {.name, .age AS age_alias, fullName: n.name}
    pub(super) fn parse_map_projection_items(&mut self) -> Result<Vec<MapProjectionItem>> {
        self.expect_char('{')?;
        self.skip_whitespace();

        let mut items = Vec::new();

        loop {
            self.skip_whitespace();

            // Check for closing brace
            if self.peek_char() == Some('}') {
                self.consume_char();
                break;
            }

            // Check if it's a property projection (.name) or virtual key (name: expr)
            if self.peek_char() == Some('.') {
                // Property projection: .name or .name AS alias
                self.consume_char(); // consume '.'
                let property = self.parse_identifier()?;
                self.skip_whitespace();

                // Check for AS alias
                let alias = if self.peek_keyword("AS") {
                    self.expect_keyword("AS")?;
                    self.skip_whitespace();
                    Some(self.parse_identifier()?)
                } else {
                    None
                };

                items.push(MapProjectionItem::Property { property, alias });
            } else {
                // Virtual key: name: expression
                let key = self.parse_identifier()?;
                self.skip_whitespace();
                self.expect_char(':')?;
                self.skip_whitespace();
                let expression = self.parse_expression()?;

                items.push(MapProjectionItem::VirtualKey { key, expression });
            }

            self.skip_whitespace();

            // Check for comma separator
            if self.peek_char() == Some(',') {
                self.consume_char();
                self.skip_whitespace();
            } else if self.peek_char() != Some('}') {
                return Err(self.error("Expected ',' or '}' in map projection"));
            }
        }

        Ok(items)
    }

    /// Parse case expression
    pub(super) fn parse_case_expression(&mut self) -> Result<Expression> {
        self.expect_keyword("CASE")?; // consume CASE

        let input = if self.peek_char() != Some('W') {
            Some(Box::new(self.parse_expression()?))
        } else {
            None
        };

        let mut when_clauses = Vec::new();

        while self.peek_keyword("WHEN") {
            self.expect_keyword("WHEN")?;
            let condition = self.parse_expression()?;
            self.expect_keyword("THEN")?;
            let result = self.parse_expression()?;
            when_clauses.push(WhenClause { condition, result });
        }

        let else_clause = if self.peek_keyword("ELSE") {
            self.expect_keyword("ELSE")?;
            Some(Box::new(self.parse_expression()?))
        } else {
            None
        };

        self.expect_keyword("END")?;

        Ok(Expression::Case {
            input,
            when_clauses,
            else_clause,
        })
    }

    /// Parse EXISTS expression
    pub(super) fn parse_exists_expression(&mut self) -> Result<Expression> {
        self.expect_keyword("EXISTS")?; // consume EXISTS
        self.skip_whitespace();

        // Expect opening brace {
        self.expect_char('{')?;
        self.skip_whitespace();

        // Parse the pattern inside the braces
        // We need to stop before WHERE or closing brace
        let pattern = self.parse_pattern_until_where_or_brace()?;
        self.skip_whitespace();

        // Parse optional WHERE clause
        let where_clause = if self.peek_keyword("WHERE") {
            self.expect_keyword("WHERE")?;
            self.skip_whitespace();
            Some(Box::new(self.parse_expression()?))
        } else {
            None
        };
        self.skip_whitespace();

        // Expect closing brace }
        self.expect_char('}')?;

        Ok(Expression::Exists {
            pattern,
            where_clause,
        })
    }

    /// Parse pattern until WHERE keyword or closing brace
    /// This is used for EXISTS and Pattern Comprehensions
    pub(super) fn parse_pattern_until_where_or_brace(&mut self) -> Result<Pattern> {
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

            // Check for WHERE keyword (stop parsing pattern)
            if self.peek_keyword("WHERE") {
                self.pos = saved_pos;
                self.line = saved_line;
                self.column = saved_column;
                break;
            }

            // Check for closing brace (stop parsing pattern)
            if self.peek_char() == Some('}') {
                self.pos = saved_pos;
                self.line = saved_line;
                self.column = saved_column;
                break;
            }

            // Check for pipe (|) - used in comprehensions
            if self.peek_char() == Some('|') {
                self.pos = saved_pos;
                self.line = saved_line;
                self.column = saved_column;
                break;
            }

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
            } else {
                // Restore position if no relationship or comma found
                self.pos = saved_pos;
                self.line = saved_line;
                self.column = saved_column;
                break;
            }
        }

        Ok(Pattern {
            elements,
            path_variable: None, // Set by caller if path variable assignment detected
        })
    }

    /// Parse comparison operator
    /// Parse additive operator
    pub(super) fn parse_additive_operator(&mut self) -> Option<BinaryOperator> {
        match self.peek_char() {
            Some('+') => {
                self.consume_char();
                Some(BinaryOperator::Add)
            }
            Some('-') => {
                self.consume_char();
                Some(BinaryOperator::Subtract)
            }
            _ => None,
        }
    }

    /// Parse multiplicative operator
    pub(super) fn parse_multiplicative_operator(&mut self) -> Option<BinaryOperator> {
        match self.peek_char() {
            Some('*') => {
                self.consume_char();
                Some(BinaryOperator::Multiply)
            }
            Some('/') => {
                self.consume_char();
                Some(BinaryOperator::Divide)
            }
            Some('%') => {
                self.consume_char();
                Some(BinaryOperator::Modulo)
            }
            Some('^') => {
                self.consume_char();
                Some(BinaryOperator::Power)
            }
            _ => None,
        }
    }
}
