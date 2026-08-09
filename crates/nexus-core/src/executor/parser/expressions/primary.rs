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
            // Leading-dot float such as `.5` (only when a digit follows the dot).
            Some('.') if self.peek_char_at(1).is_some_and(|c| c.is_ascii_digit()) => {
                self.parse_numeric_literal()
            }
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
            // Leading-dot float such as `.5` (only when a digit follows the dot).
            Some('.') if self.peek_char_at(1).is_some_and(|c| c.is_ascii_digit()) => {
                self.parse_numeric_literal()
            }
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

    /// Parse parenthesized expression, plus any `.property` read off its result.
    pub(super) fn parse_parenthesized_expression(&mut self) -> Result<Expression> {
        self.expect_char('(')?;
        let expr = self.parse_expression()?;
        self.expect_char(')')?;
        self.parse_dot_property_suffixes(expr)
    }

    /// Consume a chain of `.property` suffixes applied to an already-parsed base
    /// expression, producing [`Expression::PropertyOf`].
    ///
    /// `Expression::PropertyAccess` names its base by VARIABLE, so it can only
    /// spell `n.prop`. A base that is itself an expression — `(list[1]).existing`,
    /// `startNode(r).id` — had nowhere to go, and the parser simply stopped at the
    /// `.`; the projection list was then silently truncated there.
    ///
    /// `..` is never consumed: that is a slice range, not a property.
    pub(in super::super) fn parse_dot_property_suffixes(
        &mut self,
        base: Expression,
    ) -> Result<Expression> {
        let mut expr = base;
        while self.peek_char() == Some('.') && self.peek_char_at(1) != Some('.') {
            self.consume_char();
            let property = self.parse_identifier()?;
            expr = Expression::PropertyOf {
                base: Box::new(expr),
                property,
            };
        }
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
                    'b' => value.push('\u{0008}'), // backspace
                    'f' => value.push('\u{000C}'), // form feed
                    '0' => value.push('\u{0000}'), // null
                    '\\' => value.push('\\'),
                    // `\uXXXX` — a Unicode code point as exactly four hex digits.
                    'u' => {
                        let mut hex = String::with_capacity(4);
                        for _ in 0..4 {
                            match self.consume_char() {
                                Some(c) if c.is_ascii_hexdigit() => hex.push(c),
                                _ => {
                                    return Err(
                                        self.error("Invalid \\u escape: expected 4 hex digits")
                                    );
                                }
                            }
                        }
                        let cp = u32::from_str_radix(&hex, 16)
                            .map_err(|_| self.error("Invalid \\u escape"))?;
                        let decoded = char::from_u32(cp)
                            .ok_or_else(|| self.error("Invalid \\u code point"))?;
                        value.push(decoded);
                    }
                    _ => value.push(next),
                }
            } else {
                value.push(ch);
            }
        }

        Ok(Expression::Literal(Literal::String(value)))
    }

    /// Parse a numeric literal.
    ///
    /// Supports openCypher's full numeric grammar: decimal integers, radix
    /// prefixes (`0x1F` hex, `0o17` octal), floats with a fractional part
    /// (`1.5`, `.5`), scientific notation (`1e10`, `1.5E-3`), and `_`
    /// digit-group separators (`1_000`, `0x_FF`). The fractional `.` is only
    /// consumed when a digit follows it, so a range such as `1..5` and slice
    /// bounds are never misconsumed as a float.
    pub(super) fn parse_numeric_literal(&mut self) -> Result<Expression> {
        let start = self.pos;

        // Radix-prefixed integers: 0x.. (hex), 0o.. (octal).
        if self.peek_char() == Some('0') {
            match self.peek_char_at(1) {
                Some('x') | Some('X') => {
                    self.consume_char(); // 0
                    self.consume_char(); // x
                    let digits_start = self.pos;
                    while self
                        .peek_char()
                        .is_some_and(|c| c.is_ascii_hexdigit() || c == '_')
                    {
                        self.consume_char();
                    }
                    return self.finish_radix_int(digits_start, 16, "hexadecimal");
                }
                Some('o') | Some('O') => {
                    self.consume_char(); // 0
                    self.consume_char(); // o
                    let digits_start = self.pos;
                    while self
                        .peek_char()
                        .is_some_and(|c| ('0'..='7').contains(&c) || c == '_')
                    {
                        self.consume_char();
                    }
                    return self.finish_radix_int(digits_start, 8, "octal");
                }
                _ => {}
            }
        }

        // Decimal integer part (underscores allowed as separators).
        while self
            .peek_char()
            .is_some_and(|c| c.is_ascii_digit() || c == '_')
        {
            self.consume_char();
        }

        let mut is_float = false;

        // Fractional part — only when a digit follows the '.', so `1..5`
        // (range) is not misconsumed as `1.` then `.5`.
        if self.peek_char() == Some('.') && self.peek_char_at(1).is_some_and(|c| c.is_ascii_digit())
        {
            is_float = true;
            self.consume_char(); // .
            while self
                .peek_char()
                .is_some_and(|c| c.is_ascii_digit() || c == '_')
            {
                self.consume_char();
            }
        }

        // Exponent part: e / E with an optional sign.
        if matches!(self.peek_char(), Some('e') | Some('E')) {
            is_float = true;
            self.consume_char();
            if matches!(self.peek_char(), Some('+') | Some('-')) {
                self.consume_char();
            }
            while self
                .peek_char()
                .is_some_and(|c| c.is_ascii_digit() || c == '_')
            {
                self.consume_char();
            }
        }

        let raw: String = self.input[start..self.pos]
            .chars()
            .filter(|c| *c != '_')
            .collect();

        if is_float {
            let value = raw
                .parse::<f64>()
                .map_err(|_| self.error("Invalid float literal"))?;
            Ok(Expression::Literal(Literal::Float(value)))
        } else {
            let value = raw
                .parse::<i64>()
                .map_err(|_| self.error("Invalid integer literal"))?;
            Ok(Expression::Literal(Literal::Integer(value)))
        }
    }

    /// Finish parsing a radix-prefixed integer (`0x..`/`0o..`): strip `_`
    /// separators from `self.input[digits_start..self.pos]` and parse in the
    /// given `radix`. Errors on an empty or out-of-range digit run.
    fn finish_radix_int(
        &mut self,
        digits_start: usize,
        radix: u32,
        name: &str,
    ) -> Result<Expression> {
        let raw: String = self.input[digits_start..self.pos]
            .chars()
            .filter(|c| *c != '_')
            .collect();
        if raw.is_empty() {
            return Err(self.error(&format!("Invalid {name} literal: no digits")));
        }
        let value = i64::from_str_radix(&raw, radix)
            .map_err(|_| self.error(&format!("Invalid {name} literal")))?;
        Ok(Expression::Literal(Literal::Integer(value)))
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
