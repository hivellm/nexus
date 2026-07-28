//! Structured expression parsers: map projections, CASE expressions,
//! EXISTS { … }, COLLECT { … } subqueries, and the pattern-until-brace
//! helper used by both EXISTS and pattern comprehensions.

use super::super::CypherParser;
use super::super::ast::*;
use crate::Result;

impl CypherParser {
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

    /// Parse `COLLECT { … }` subquery expression
    /// (phase6_opencypher-subquery-transactions §9).
    ///
    /// Reuses the same clause-parsing loop as `CALL { … }` so every
    /// MATCH / WITH / WHERE / RETURN combination accepted by the
    /// outer query is also accepted inside the collect subquery.
    /// The inner MUST terminate with a `RETURN` clause — that's the
    /// values the LIST is folded over.
    pub(super) fn parse_collect_subquery_expression(&mut self) -> Result<Expression> {
        self.expect_keyword("COLLECT")?;
        self.skip_whitespace();
        self.expect_char('{')?;
        self.skip_whitespace();

        let clauses = self.parse_subquery_clause_body()?;
        self.skip_whitespace();
        self.expect_char('}')?;

        if clauses.is_empty() {
            return Err(self.error(
                "ERR_COLLECT_SUBQUERY_EMPTY: COLLECT { … } must contain \
                 at least one clause",
            ));
        }

        // Cypher 25 requires the inner to end in RETURN — that's the
        // expression the LIST is built over. Reject inputs that drop
        // the RETURN so we don't silently emit an empty list.
        let last_is_return = matches!(clauses.last(), Some(Clause::Return(_)));
        if !last_is_return {
            return Err(self.error(
                "ERR_COLLECT_SUBQUERY_NO_RETURN: COLLECT { … } must \
                 terminate with a RETURN clause",
            ));
        }

        let inner = CypherQuery {
            clauses,
            params: std::collections::HashMap::new(),
            graph_scope: None,
        };
        Ok(Expression::CollectSubquery {
            inner: Box::new(inner),
        })
    }

    /// Parse the `{ … }` clause list shared by `COLLECT { … }` and the
    /// full `EXISTS { MATCH … }` subquery form. The caller has already
    /// consumed the opening `{`. Stops at the first `}` or the first token
    /// that is not a valid clause start, WITHOUT consuming that closing
    /// `}` — every caller must follow up with its own
    /// `self.expect_char('}')?` so a missing/malformed closing brace
    /// surfaces a clear syntax error instead of silently truncating the
    /// clause list.
    fn parse_subquery_clause_body(&mut self) -> Result<Vec<Clause>> {
        let mut clauses = Vec::new();
        while self.pos < self.input.len() {
            self.skip_whitespace();
            if self.peek_char() == Some('}') {
                break;
            }
            if self.is_clause_boundary() {
                let clause = self.parse_clause(clauses.last())?;
                clauses.push(clause);
            } else {
                break;
            }
        }
        Ok(clauses)
    }

    /// Parse an `EXISTS { … }` expression: either the abbreviated
    /// pattern-probe form (`EXISTS { pattern [WHERE expr] }`) or, when the
    /// body opens with a clause keyword (`MATCH`, `OPTIONAL MATCH`,
    /// `WITH`, or — structurally, to be rejected with
    /// `InvalidClauseComposition` — a write clause), the full subquery
    /// form (`EXISTS { MATCH … [WHERE …] [WITH …] [RETURN …] }`,
    /// openCypher TCK `ExistentialSubquery2`/`ExistentialSubquery3`). The
    /// abbreviated form's pattern never starts with a clause keyword (it
    /// starts with `(`), so `is_clause_boundary()` cleanly distinguishes
    /// the two without a `MATCH`-only special case.
    pub(super) fn parse_exists_expression(&mut self) -> Result<Expression> {
        self.expect_keyword("EXISTS")?; // consume EXISTS
        self.skip_whitespace();

        // Expect opening brace {
        self.expect_char('{')?;
        self.skip_whitespace();

        // A clause-keyword start (MATCH, OPTIONAL, WITH, or a write
        // clause) is always the full subquery form; anything else (a bare
        // pattern) is the abbreviated pattern-probe form.
        if self.is_clause_boundary() {
            return self.parse_exists_subquery_body();
        }

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
            inner: ExistsInner::Pattern {
                pattern,
                where_clause,
            },
        })
    }

    /// Parse the clause list of the full `EXISTS { MATCH … }` subquery
    /// form. The caller has already consumed the opening `{` and confirmed
    /// the body opens on a clause keyword. Write clauses (`SET`/`CREATE`/
    /// `DELETE`/`MERGE`/`REMOVE`/`FOREACH`) are rejected here — openCypher
    /// TCK `ExistentialSubquery2[3]` requires a compile-time
    /// `InvalidClauseComposition` `SyntaxError` for `EXISTS { MATCH … SET
    /// … }`, and the same check catches a write clause with no leading
    /// `MATCH` at all (`EXISTS { CREATE (x) }`).
    fn parse_exists_subquery_body(&mut self) -> Result<Expression> {
        let clauses = self.parse_subquery_clause_body()?;
        self.skip_whitespace();
        self.expect_char('}')?;

        if clauses.is_empty() {
            return Err(self.error(
                "ERR_EXISTS_SUBQUERY_EMPTY: EXISTS { … } must contain at least one clause",
            ));
        }

        if let Some(write_clause) = clauses.iter().find(|c| is_write_clause(c)) {
            return Err(self.error(&format!(
                "InvalidClauseComposition: {} is not allowed inside an \
                 EXISTS {{ … }} subquery",
                write_clause_name(write_clause)
            )));
        }

        // Fast path: a single MATCH (optionally filtered by one WHERE,
        // optionally followed by one non-aggregating RETURN) desugars
        // straight into the abbreviated pattern-probe form, so evaluation
        // reuses `evaluate_exists_pattern`'s short-circuiting
        // depth-first witness search instead of planning and running a
        // full operator pipeline on every outer row — openCypher TCK
        // `ExistentialSubquery2[1]`.
        if let Some((pattern, where_clause)) = try_desugar_exists_pattern(&clauses) {
            return Ok(Expression::Exists {
                inner: ExistsInner::Pattern {
                    pattern,
                    where_clause,
                },
            });
        }

        let inner = CypherQuery {
            clauses,
            params: std::collections::HashMap::new(),
            graph_scope: None,
        };
        Ok(Expression::Exists {
            inner: ExistsInner::Subquery {
                inner: Box::new(inner),
            },
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
}

/// True for clause kinds that mutate the graph and are therefore illegal
/// inside an `EXISTS { … }` subquery body (openCypher TCK
/// `ExistentialSubquery2[3]`).
fn is_write_clause(clause: &Clause) -> bool {
    matches!(
        clause,
        Clause::Create(_)
            | Clause::Merge(_)
            | Clause::Set(_)
            | Clause::Delete(_)
            | Clause::Remove(_)
            | Clause::Foreach(_)
    )
}

/// Display name for the write-clause token embedded in the
/// `InvalidClauseComposition` error message.
fn write_clause_name(clause: &Clause) -> &'static str {
    match clause {
        Clause::Create(_) => "CREATE",
        Clause::Merge(_) => "MERGE",
        Clause::Set(_) => "SET",
        Clause::Delete(_) => "DELETE",
        Clause::Remove(_) => "REMOVE",
        Clause::Foreach(_) => "FOREACH",
        _ => "clause",
    }
}

/// When `clauses` is exactly a single non-`OPTIONAL` `MATCH` — optionally
/// followed by one `WHERE`, optionally followed by one `RETURN` whose
/// items call no aggregate function — extracts the pattern and
/// where-expression the abbreviated `EXISTS { pattern [WHERE …] }` form
/// needs. Returns `None` for anything else (a second `MATCH`, `WITH`,
/// `UNION`, `OPTIONAL MATCH`, or a `RETURN` that aggregates): every one of
/// those can change the inner row count from what the bare pattern would
/// report (an aggregate `RETURN` in particular always yields exactly one
/// row, even over zero matches, so folding it away would silently turn a
/// non-match into a match).
fn try_desugar_exists_pattern(clauses: &[Clause]) -> Option<(Pattern, Option<Box<Expression>>)> {
    let (match_clause, rest) = match clauses.split_first()? {
        (Clause::Match(m), rest) if !m.optional => (m, rest),
        _ => return None,
    };

    let (where_expr, rest) = match rest.split_first() {
        Some((Clause::Where(w), rest)) => (Some(&w.expression), rest),
        _ => (None, rest),
    };

    match rest {
        [] => {}
        [Clause::Return(r)] if r.items.iter().all(|i| i.expression.is_aggregate_free()) => {}
        _ => return None,
    }

    Some((
        match_clause.pattern.clone(),
        where_expr.cloned().map(Box::new),
    ))
}
