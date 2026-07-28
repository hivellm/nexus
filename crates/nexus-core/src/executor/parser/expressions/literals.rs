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
        // or a path-bound one: [p = (pattern) WHERE ... | ...]. Pattern
        // comprehensions start with '(', an identifier followed by ':'
        // (label) or '-' (relationship), or an identifier followed by
        // '=' and then '(' (path-variable binding). The lookahead below
        // only decides whether to ATTEMPT a comprehension parse; the
        // attempt itself (`try_parse_pattern_comprehension_tail`) is
        // fully tentative — on any failure (a genuine syntax error, OR
        // the parsed shape failing the path-binding constraints, e.g.
        // `[a = (1 + 2)]`, a one-element list holding a parenthesized-
        // expression equality) position is restored to right after `[`
        // and control falls through to list-comprehension / list-literal
        // parsing below. This mirrors the shortestPath()/allShortestPaths()
        // tentative-pattern parse in `identifier.rs`.
        let saved_pos = self.pos;
        let saved_line = self.line;
        let saved_column = self.column;
        let mut comprehension_binding_variable: Option<String> = None;
        let is_pattern_comprehension = if self.peek_char() == Some('(') {
            // Starts with '(', likely a pattern
            true
        } else if self.is_identifier_start() {
            let identifier = self.parse_identifier()?;
            self.skip_whitespace();
            let next_char = self.peek_char();
            if next_char == Some('=') {
                // Tentative path-variable binding: `ident = (pattern) ...`.
                // Only treated as one when '=' is followed by the start of
                // a pattern; otherwise this identifier is not a pattern
                // comprehension at all and position is fully restored so
                // the list-comprehension / list-literal parsers below get
                // a clean slate.
                self.consume_char(); // consume '='
                self.skip_whitespace();
                if self.peek_char() == Some('(') {
                    comprehension_binding_variable = Some(identifier);
                    true
                } else {
                    self.pos = saved_pos;
                    self.line = saved_line;
                    self.column = saved_column;
                    false
                }
            } else {
                // Check if identifier is followed by ':' (label) or '-' (relationship)
                let is_pattern = next_char == Some(':') || next_char == Some('-');
                // Reset position
                self.pos = saved_pos;
                self.line = saved_line;
                self.column = saved_column;
                is_pattern
            }
        } else {
            false
        };

        if is_pattern_comprehension {
            // When a binding variable was captured above, `self.pos`
            // already sits right at the pattern's opening '(' (the
            // `ident =` prefix was consumed, not restored) — the tail
            // parser picks up from there.
            match self.try_parse_pattern_comprehension_tail(comprehension_binding_variable) {
                Ok(expr) => return Ok(expr),
                // Once the pattern has at least one relationship, a
                // shape violation (comma-separated parts, a missing
                // `| expr`) has no other valid Cypher reading to fall
                // back to — surface it directly instead of swallowing
                // it into a confusing fallback parse error.
                Err(e) if is_hard_pattern_comprehension_shape_error(&e) => return Err(e),
                Err(_) => {
                    self.pos = saved_pos;
                    self.line = saved_line;
                    self.column = saved_column;
                }
            }
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

    /// Attempts the pattern/WHERE/transform/`]` tail of a pattern
    /// comprehension once [`Self::parse_list_expression`]'s `[(` /
    /// `[ident:` / `[ident-` / `[ident = (` lookahead has identified a
    /// candidate. `self.pos` must already sit at the pattern's opening
    /// `(` — the caller has consumed any `ident =` prefix but has not
    /// restored position for it.
    ///
    /// Returns `Err` — always discarded by the caller, which restores
    /// position to right after `[` and falls back to list-comprehension
    /// / list-literal parsing — for a genuine parse failure, or when
    /// `binding_variable` is `Some` but the parsed pattern fails either
    /// of openCypher's path-binding shape requirements:
    /// - at least one relationship (a bare `(b)` is never a
    ///   comprehension — `[a = (b)]` is a one-element list holding a
    ///   boolean equality, never a path binding), and
    /// - a mandatory `| expr` transform (the non-path-bound forms
    ///   tolerate its absence, an extra permissiveness of this dialect's
    ///   parser, but a path binding with nothing to project is never
    ///   intentional).
    ///
    /// A path-bound pattern (`binding_variable` is `Some`) must also be
    /// a single connected component: comma-separated pattern parts
    /// (`[p = (a)-->(b), (c)-->(d) | p]`) have no single traversal
    /// order to bind `p` to, so the parsed pattern is rejected outright
    /// rather than letting the graph walk silently build a
    /// nodes-from-every-component path.
    fn try_parse_pattern_comprehension_tail(
        &mut self,
        binding_variable: Option<String>,
    ) -> Result<Expression> {
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

        if binding_variable.is_some() {
            // `has_relationship` is the disambiguator: it decides
            // whether every OTHER shape violation below is a hard,
            // non-swallowed error or a soft one the caller falls back
            // from. A pattern with zero relationships (`(b)`, or
            // `(b), (c)`) has a perfectly valid alternate reading as a
            // plain list/equality expression — `(b)` is a parenthesized
            // variable, and the comma in `(b), (c)` may be the outer
            // LIST LITERAL's own element separator, not a pattern's
            // comma-separated-parts syntax at all (`[a = (b), (c)]`
            // must parse as the two-element list `[a = (b), (c)]`, not
            // be misread as a botched comma-separated path binding).
            // Once at least one relationship HAS parsed, though, the
            // input is unambiguously an attempted path-bound
            // comprehension, and every remaining violation (a
            // comma-separated second part, a missing `| expr`) has no
            // other valid Cypher reading — those become hard errors,
            // tagged with an `ERR_PATTERN_COMPREHENSION_` sentinel
            // prefix `is_hard_pattern_comprehension_shape_error` checks
            // for, mirroring this crate's `ERR_*` error-code convention.
            let has_relationship = pattern
                .elements
                .iter()
                .any(|e| matches!(e, PatternElement::Relationship(_)));

            if has_relationship {
                let has_comma_separated_parts = pattern.elements.windows(2).any(|w| {
                    matches!(
                        (&w[0], &w[1]),
                        (PatternElement::Node(_), PatternElement::Node(_))
                    )
                });
                if has_comma_separated_parts {
                    return Err(Error::CypherSyntax(
                        "ERR_PATTERN_COMPREHENSION_COMMA_SEPARATED_PATH_BINDING: a path-bound \
                         pattern comprehension (`p = pattern | expr`) requires a single \
                         connected pattern; comma-separated pattern parts are not supported"
                            .to_string(),
                    ));
                }
                if transform_expression.is_none() {
                    return Err(Error::CypherSyntax(
                        "ERR_PATTERN_COMPREHENSION_MISSING_TRANSFORM_PATH_BINDING: a \
                         path-bound pattern comprehension (`p = pattern | expr`) requires a \
                         `| expr` transform"
                            .to_string(),
                    ));
                }
            } else {
                // No relationship at all — genuinely ambiguous with a
                // plain list/equality expression; swallow via a
                // generic (non-sentinel) `Err` so the caller falls back
                // to list/expression parsing.
                return Err(Error::CypherSyntax(
                    "a path-bound pattern comprehension (`p = pattern | expr`) requires at \
                     least one relationship and a `| expr` transform"
                        .to_string(),
                ));
            }
        }

        Ok(Expression::PatternComprehension {
            pattern,
            where_clause,
            transform_expression,
            binding_variable,
        })
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

/// Recognises the hard, non-swallowed errors `try_parse_pattern_comprehension_tail`
/// raises once a path-bound pattern (`binding_variable` is `Some`) has
/// already parsed at least one relationship — a comma-separated second
/// part, or a missing `| expr` transform — so `parse_list_expression`
/// can propagate them directly instead of swallowing them into the
/// generic tentative-parse fallback. Every OTHER failure from that
/// function (a genuine parse error, or any shape violation on a
/// relationship-less pattern) genuinely may describe something else
/// entirely — a parenthesized expression, a bare-node equality, or the
/// outer list literal's own comma separator — and stays swallowed.
fn is_hard_pattern_comprehension_shape_error(err: &Error) -> bool {
    matches!(
        err,
        Error::CypherSyntax(msg) if msg.starts_with("ERR_PATTERN_COMPREHENSION_")
    )
}
