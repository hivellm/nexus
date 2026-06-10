//! Identifier-expression dispatch: function calls (including filter /
//! shortestPath / allShortestPaths special forms), property access,
//! label predicates, map projections, and array index / slice suffixes.

use super::super::CypherParser;
use super::super::ast::*;
use crate::{Error, Result};

impl CypherParser {
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

            let mut expr = Expression::FunctionCall {
                name: identifier,
                args,
            };

            // Handle postfix `[index]` after a function call, e.g. `labels(n)[0]`,
            // `head(collect(x))[0]`. One or more index/slice suffixes are folded
            // left-associatively into nested `ArrayIndex` / `ArraySlice` nodes so
            // the evaluator can handle them at runtime.
            while self.peek_char() == Some('[') {
                self.consume_char(); // consume '['
                self.skip_whitespace();

                // Detect slice syntax: func()[start..end]
                // Peek ahead to see if we have a '..' inside.
                let start_expr = if self.peek_char() != Some('.') && self.peek_char() != Some(']') {
                    Some(Box::new(self.parse_expression()?))
                } else {
                    None
                };

                self.skip_whitespace();

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
                    self.skip_whitespace();
                    self.expect_char(']')?;

                    if let Some(index) = start_expr {
                        expr = Expression::ArrayIndex {
                            base: Box::new(expr),
                            index,
                        };
                    } else {
                        return Err(self.error("Array index expected after '['"));
                    }
                }
            }

            Ok(expr)
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
        // or `var:$param` in WHERE/RETURN expression position. Reuses
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
}
