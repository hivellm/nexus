//! Operator-precedence climbing: OR → AND → NOT → comparison →
//! additive → multiplicative → unary → simple/primary.
//! Also contains operator-recognition helpers and `peek_char_at`.

use super::super::CypherParser;
use super::super::ast::*;
use crate::{Error, Result};

impl CypherParser {
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
    pub(in super::super) fn parse_comparison_operator(&mut self) -> Option<BinaryOperator> {
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
    pub(in super::super) fn peek_char_at(&self, offset: usize) -> Option<char> {
        self.input[self.pos..].chars().nth(offset)
    }

    /// Parse comparison operator
    /// Parse additive operator
    pub(in super::super) fn parse_additive_operator(&mut self) -> Option<BinaryOperator> {
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
    pub(in super::super) fn parse_multiplicative_operator(&mut self) -> Option<BinaryOperator> {
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
