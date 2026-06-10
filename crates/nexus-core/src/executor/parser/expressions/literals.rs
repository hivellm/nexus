//! List and point literal parsers, including the array-slice lookahead
//! logic and coordinate extraction helper.

use super::super::CypherParser;
use super::super::ast::*;
use crate::{Error, Result};

impl CypherParser {
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
        // Track whether the caller used `longitude/latitude/height`
        // (implies WGS-84 unless an explicit `crs` overrides) or the
        // `x/y/z` aliases (Cartesian default). Explicit `crs:` always
        // wins.
        let mut wgs_keys_seen = false;
        let mut explicit_crs = false;

        // Parse key-value pairs
        while self.peek_char() != Some('}') {
            let key = self.parse_identifier()?;
            self.skip_whitespace();
            self.expect_char(':')?;
            self.skip_whitespace();

            match key.to_lowercase().as_str() {
                "x" => {
                    let expr = self.parse_expression()?;
                    x = Some(self.extract_number_from_expression(&expr)?);
                }
                "longitude" => {
                    wgs_keys_seen = true;
                    let expr = self.parse_expression()?;
                    x = Some(self.extract_number_from_expression(&expr)?);
                }
                "y" => {
                    let expr = self.parse_expression()?;
                    y = Some(self.extract_number_from_expression(&expr)?);
                }
                "latitude" => {
                    wgs_keys_seen = true;
                    let expr = self.parse_expression()?;
                    y = Some(self.extract_number_from_expression(&expr)?);
                }
                "z" => {
                    let expr = self.parse_expression()?;
                    z = Some(self.extract_number_from_expression(&expr)?);
                }
                "height" => {
                    wgs_keys_seen = true;
                    let expr = self.parse_expression()?;
                    z = Some(self.extract_number_from_expression(&expr)?);
                }
                "crs" => {
                    explicit_crs = true;
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

        // Implicit CRS inference: when the caller used the geographic
        // key aliases (longitude/latitude/height) without an explicit
        // `crs:` field, the point is WGS-84. This matches Neo4j's
        // behaviour and is what users expect from
        // `point({longitude: 13.4, latitude: 52.5})`. Explicit `crs:`
        // always wins (handled above by `explicit_crs`).
        if wgs_keys_seen && !explicit_crs {
            coordinate_system = crate::geospatial::CoordinateSystem::WGS84;
        }

        let point = if let Some(z_val) = z {
            crate::geospatial::Point::new_3d(x, y, z_val, coordinate_system)
        } else {
            crate::geospatial::Point::new_2d(x, y, coordinate_system)
        };

        Ok(Expression::Literal(Literal::Point(point)))
    }

    /// Extract number from expression (helper for point parsing).
    ///
    /// Accepts integer / float literals plus unary `+`/`-` applied to
    /// such literals. The unary case is necessary because the lexer
    /// tokenises `-1.0` as `UnaryOp { Minus, Literal::Float(1.0) }`,
    /// not as `Literal::Float(-1.0)`. Without this, point literals
    /// with negative coordinates (`{longitude: -73.9857, …}`) raised
    /// `Cypher syntax error: Point coordinates must be numbers`
    /// despite being canonical Cypher.
    pub(super) fn extract_number_from_expression(&self, expr: &Expression) -> Result<f64> {
        match expr {
            Expression::Literal(Literal::Integer(i)) => Ok(*i as f64),
            Expression::Literal(Literal::Float(f)) => Ok(*f),
            Expression::UnaryOp { op, operand } => match op {
                UnaryOperator::Minus => Ok(-self.extract_number_from_expression(operand)?),
                UnaryOperator::Plus => self.extract_number_from_expression(operand),
                UnaryOperator::Not => Err(Error::CypherSyntax(
                    "Point coordinates must be numbers".to_string(),
                )),
            },
            _ => Err(Error::CypherSyntax(
                "Point coordinates must be numbers".to_string(),
            )),
        }
    }
}
